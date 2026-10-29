//! `.vxtr` files as bytes: what [`to_bytes`] writes and what
//! [`from_bytes`] accepts. No file I/O; the app reads and writes the bytes.
//!
//! Loaded bytes are treated as hostile (`security-check` §2): size, format
//! version, node count and every document invariant are checked, and any
//! failure is a [`FormatError`], never a panic.

use std::io::Write;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::document::{Document, FORMAT_VERSION};
use crate::node::NodeKind;

mod shared;

pub(crate) use shared::{Images, Preview, get as shared_image, share as share_image};

/// Oldest `.vxtr` format version still read (version 1 had no text).
pub const MIN_FORMAT_VERSION: u32 = 1;

/// Largest `.vxtr` file that is loaded, in bytes (50 MB).
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// Most nodes a loaded document may hold.
pub const MAX_NODES: usize = 20_000;

/// Why bytes could not be turned into a [`Document`], or a document into
/// bytes. The messages are shown to the user as is.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum FormatError {
    #[error("the file is {} MB, more than the 50 MB limit", .0.div_ceil(1024 * 1024))]
    TooLarge(u64),
    #[error("the file is incomplete (it was cut off)")]
    Truncated,
    #[error("not a Vextor file ({0})")]
    NotVextor(String),
    #[error(
        "the file uses format version {0}; this Vextor reads versions {MIN_FORMAT_VERSION} to {FORMAT_VERSION}"
    )]
    UnknownVersion(u32),
    #[error("the file has {0} objects, more than the {MAX_NODES} limit")]
    TooManyNodes(usize),
    #[error("the file is damaged: {0}")]
    Invalid(String),
    #[error("the document could not be written: {0}")]
    Encode(String),
}

/// Serialises `doc` as compact `.vxtr` JSON (spec 14): no indentation,
/// default fields left out, each distinct embedded image stored once.
/// Numbers keep their shortest exact form, so nothing is rounded.
///
/// # Errors
/// [`FormatError::Encode`] if serialisation fails (not expected for a
/// validated document).
pub fn to_bytes(doc: &Document) -> Result<Vec<u8>, FormatError> {
    let mut bytes = Vec::new();
    to_writer(&mut bytes, doc)?;
    Ok(bytes)
}

/// [`to_bytes`] straight into `w`, without a full-size copy in memory.
///
/// # Errors
/// [`FormatError::Encode`] when serialisation or `w` fails.
pub fn to_writer<W: Write>(w: W, doc: &Document) -> Result<(), FormatError> {
    to_writer_with_preview(w, doc, None)
}

/// [`to_writer`] with `preview`, a PNG of the document, stored as
/// `preview_png` (format 5, spec 14d) for the Quick Look extension.
/// Loading ignores it: the picture is generated, never trusted.
///
/// # Errors
/// [`FormatError::Encode`] when serialisation or `w` fails.
pub fn to_writer_with_preview<W: Write>(
    mut w: W,
    doc: &Document,
    preview: Option<&[u8]>,
) -> Result<(), FormatError> {
    let _images = shared::SaveScope::start(preview);
    serde_json::to_writer(&mut w, doc).map_err(|e| FormatError::Encode(e.to_string()))?;
    w.write_all(b"\n")
        .map_err(|e| FormatError::Encode(e.to_string()))
}

/// [`to_bytes`] indented, for reading and diffing a file by hand.
///
/// # Errors
/// [`FormatError::Encode`] if serialisation fails.
pub fn to_bytes_pretty(doc: &Document) -> Result<Vec<u8>, FormatError> {
    let _images = shared::SaveScope::start(None);
    let mut bytes = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(
        &mut bytes,
        serde_json::ser::PrettyFormatter::with_indent(b"  "),
    );
    doc.serialize(&mut ser)
        .map_err(|e| FormatError::Encode(e.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Parses `.vxtr` bytes. Checks, in order: size, format version, JSON
/// shape, node count, document invariants.
///
/// # Errors
/// The first check that fails, as a [`FormatError`].
pub fn from_bytes(bytes: &[u8]) -> Result<Document, FormatError> {
    let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if len > MAX_FILE_BYTES {
        return Err(FormatError::TooLarge(len));
    }

    // Read only the version first, so a file from a newer Vextor is refused
    // with that reason rather than as a shape mismatch.
    let probe: Probe = serde_json::from_slice(bytes).map_err(|e| json_error(&e))?;
    let version = match probe.format_version {
        None => return Err(FormatError::NotVextor("no format_version".to_owned())),
        Some(v @ MIN_FORMAT_VERSION..=FORMAT_VERSION) => v,
        Some(v) => return Err(FormatError::UnknownVersion(v)),
    };

    let images = probe
        .images
        .into_iter()
        .map(|b| STANDARD.decode(b))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| FormatError::Invalid(format!("a shared image is not base64: {e}")))?;
    let _images = shared::LoadScope::start(images);
    let mut doc: Document = serde_json::from_slice(bytes).map_err(|e| json_error(&e))?;
    if doc.nodes.len() > MAX_NODES {
        return Err(FormatError::TooManyNodes(doc.nodes.len()));
    }
    // Names from before spec 10 had no rules (an outlined text could name
    // a letter shape after a tab): tidy them rather than refuse the file.
    for node in doc.nodes.values_mut() {
        node.name = node.name.as_deref().and_then(crate::node::clean_name);
    }
    if let Err(violations) = doc.validate() {
        let first = violations
            .first()
            .map_or_else(String::new, ToString::to_string);
        return Err(FormatError::Invalid(first));
    }
    migrate(&mut doc, version)?;
    Ok(doc)
}

/// Brings a loaded older version up to [`FORMAT_VERSION`]; saving then
/// writes the current version.
///
/// 1 → 2: same shape, text objects added. A version-1 file holding text
/// was not written by Vextor and is refused. 2 → 3: the export options
/// gained `text_as_outlines` (absent = off). 3 → 4: same model; files are
/// compact, leave default fields out and store each image once (spec 14),
/// all read by the same code. 4 → 5: same model; files may carry a
/// `preview_png` for Finder (spec 14d), which loading skips.
fn migrate(doc: &mut Document, version: u32) -> Result<(), FormatError> {
    if version < 2
        && doc
            .nodes
            .values()
            .any(|n| matches!(n.kind, NodeKind::Text(_)))
    {
        return Err(FormatError::Invalid(
            "text objects need format version 2".to_owned(),
        ));
    }
    doc.format_version = FORMAT_VERSION;
    Ok(())
}

/// What is read before the document itself: the version, and the shared
/// images (version 4) the image nodes refer to.
#[derive(Deserialize)]
struct Probe {
    format_version: Option<u32>,
    #[serde(default)]
    images: Vec<String>,
}

fn json_error(e: &serde_json::Error) -> FormatError {
    if e.is_eof() {
        FormatError::Truncated
    } else {
        FormatError::NotVextor(e.to_string())
    }
}
