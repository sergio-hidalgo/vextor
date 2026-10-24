//! `.vxtr` files as bytes: what [`to_bytes`] writes and what
//! [`from_bytes`] accepts. No file I/O; the app reads and writes the bytes.
//!
//! Loaded bytes are treated as hostile (`security-check` §2): size, format
//! version, node count and every document invariant are checked, and any
//! failure is a [`FormatError`], never a panic.

use serde::Deserialize;
use thiserror::Error;

use crate::document::{Document, FORMAT_VERSION};
use crate::node::NodeKind;

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

/// Serialises `doc` as pretty-printed `.vxtr` JSON.
///
/// # Errors
/// [`FormatError::Encode`] if serialisation fails (not expected for a
/// validated document).
pub fn to_bytes(doc: &Document) -> Result<Vec<u8>, FormatError> {
    let mut bytes =
        serde_json::to_vec_pretty(doc).map_err(|e| FormatError::Encode(e.to_string()))?;
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
/// gained `text_as_outlines` (absent = off).
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

/// The only field read before the version is known.
#[derive(Deserialize)]
struct Probe {
    format_version: Option<u32>,
}

fn json_error(e: &serde_json::Error) -> FormatError {
    if e.is_eof() {
        FormatError::Truncated
    } else {
        FormatError::NotVextor(e.to_string())
    }
}
