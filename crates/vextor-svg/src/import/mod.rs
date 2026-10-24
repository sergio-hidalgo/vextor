//! SVG and image import (spec 13).
//!
//! An SVG goes through `usvg`, which resolves CSS, `<use>`, units and
//! shapes into groups, paths, images and text; [`nodes`] maps that tree to
//! the model (with [`defs`] for clips and patterns), inverting what the writer emits so that import → export →
//! import is stable. Raster images (PNG, JPEG, WebP) become one embedded
//! image node ([`image`]).
//!
//! Everything imported is hostile until checked (`security-check` §2):
//! sizes are checked before parsing, [`guard`] bounds the XML (nodes,
//! nesting, `<use>` expansion), external files are never read and images
//! must be embedded PNG, JPEG or WebP no larger than [`MAX_IMAGE_SIDE`].

mod defs;
mod guard;
mod image;
mod nodes;
mod paint;
mod parse;
mod text;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use vextor_core::{
    Artboard, ClipboardPayload, Container, Document, MAX_FILE_BYTES, MAX_NODES, Node, NodeId,
    NodeKind, Rect, Transform,
};
use vextor_geom::Fonts;

pub use self::image::import_image;

/// Images wider or taller than this many pixels are refused.
pub const MAX_IMAGE_SIDE: u32 = 16_384;

/// Why an import failed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ImportError {
    #[error("the file is larger than {} MB", MAX_FILE_BYTES / (1024 * 1024))]
    TooLarge,
    #[error("it has more than {MAX_NODES} objects")]
    TooManyObjects,
    #[error("its elements are nested too deeply")]
    TooDeep,
    #[error("it has coordinates too large to draw")]
    TooFar,
    #[error("compressed SVG (.svgz) is not supported")]
    Compressed,
    #[error("it is not a valid SVG file ({0})")]
    Invalid(String),
    #[error("it has nothing that can be drawn")]
    Empty,
    #[error("it is not a PNG, JPEG or WebP image")]
    UnsupportedImage,
    #[error("the image is larger than {MAX_IMAGE_SIDE} × {MAX_IMAGE_SIDE} pixels")]
    ImageTooLarge,
}

/// What was imported: objects ready to paste, the size of the SVG's
/// viewport (or the image) and what could not be kept as it was.
#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    /// Roots bottom → top, each placed in the imported file's space (its
    /// viewport's top-left at the origin).
    pub payload: ClipboardPayload,
    /// Width and height of the viewport, in px.
    pub size: (f64, f64),
    pub report: Report,
}

/// Features left out or changed on the way in, for the summary message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// How many objects were imported.
    pub objects: usize,
    /// Each feature left out, with how many times.
    pub dropped: BTreeMap<Dropped, usize>,
    /// Gradients drawn slightly differently (vextor has no gradient
    /// transform: a stretched radial gradient stays round).
    pub approximated: usize,
    /// Why each text imported as outlines instead of text.
    pub outlined: Vec<String>,
}

/// A feature vextor does not have.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dropped {
    Filter,
    Mask,
    BlendMode,
    /// External, unsupported (GIF, nested SVG) or too large.
    Image,
    /// `paint-order` stroke first.
    PaintOrder,
    /// A clip path on a clip path.
    NestedClip,
    /// Text that could not be laid out (no fonts at all).
    Text,
}

impl Dropped {
    fn label(self, n: usize) -> String {
        let (one, many) = match self {
            Self::Filter => ("filter", "filters"),
            Self::Mask => ("mask", "masks"),
            Self::BlendMode => ("blend mode", "blend modes"),
            Self::Image => ("image", "images"),
            Self::PaintOrder => ("stroke-first paint order", "stroke-first paint orders"),
            Self::NestedClip => ("nested clip", "nested clips"),
            Self::Text => ("text without fonts", "texts without fonts"),
        };
        format!("{n} {}", if n == 1 { one } else { many })
    }
}

impl Report {
    pub(crate) fn drop(&mut self, what: Dropped) {
        *self.dropped.entry(what).or_default() += 1;
    }

    /// True when everything came in as it was.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.dropped.is_empty() && self.approximated == 0 && self.outlined.is_empty()
    }

    /// One line for the message bar: what came in, what was left out.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut s = format!(
            "Imported {} object{}.",
            self.objects,
            if self.objects == 1 { "" } else { "s" }
        );
        if !self.dropped.is_empty() {
            let parts: Vec<String> = self.dropped.iter().map(|(d, &n)| d.label(n)).collect();
            let _ = write!(s, " Left out: {}.", parts.join(", "));
        }
        if !self.outlined.is_empty() {
            let mut reasons = self.outlined.clone();
            reasons.sort();
            reasons.dedup();
            let n = self.outlined.len();
            let _ = write!(
                s,
                " {n} text{} as outlines ({}).",
                if n == 1 { "" } else { "s" },
                reasons.join("; ")
            );
        }
        if self.approximated > 0 {
            let n = self.approximated;
            let _ = write!(
                s,
                " {n} gradient{} approximated.",
                if n == 1 { "" } else { "s" }
            );
        }
        s
    }
}

/// True when `text` (clipboard content) looks like an SVG document.
#[must_use]
pub fn looks_like_svg(text: &str) -> bool {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    t.starts_with('<') && t.contains("<svg")
}

/// Imports SVG `bytes`, laying out text with `fonts`.
///
/// # Errors
/// An [`ImportError`] when the file is too large, malformed, hostile or
/// draws nothing.
pub fn import_svg(bytes: &[u8], fonts: &Fonts) -> Result<Imported, ImportError> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(ImportError::TooLarge);
    }
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return Err(ImportError::Compressed);
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ImportError::Invalid("not UTF-8 text".to_owned()))?;
    let xml = guard::parse(text)?;
    let tree = parse::tree(&xml, fonts)?;
    let (payload, report) = nodes::map(&tree, fonts)?;
    let size = tree.size();
    Ok(Imported {
        payload,
        size: (f64::from(size.width()), f64::from(size.height())),
        report,
    })
}

/// A new document named `name` holding `imported`: one artboard the size
/// of its viewport, one layer with its objects (for File ▸ Open of an
/// SVG).
///
/// # Errors
/// [`ImportError::Empty`] when the objects cannot be placed (never for a
/// payload [`import_svg`] returned).
pub fn into_document(imported: &Imported, name: &str) -> Result<Document, ImportError> {
    let mut doc = Document::new(name);
    let (w, h) = imported.size;
    let id = doc.alloc_artboard_id();
    doc.artboards.push(Artboard {
        id,
        name: name.to_owned(),
        rect: Rect::new(0.0, 0.0, w, h),
        background: None,
    });
    let layer = doc.layers[0];
    let (cmd, _) = doc
        .paste(&imported.payload, layer, 0, Transform::IDENTITY, "Import")
        .map_err(|_| ImportError::Empty)?;
    doc.apply(cmd).map_err(|_| ImportError::Empty)?;
    Ok(doc)
}

/// `imported`'s objects as one group (named `name`) when there are
/// several, so an import is picked and moved as one object.
#[must_use]
pub fn grouped(mut imported: Imported, name: Option<&str>) -> Imported {
    let p = &mut imported.payload;
    let children: std::collections::BTreeSet<NodeId> = p
        .nodes
        .iter()
        .filter_map(Node::container)
        .flat_map(|c| c.children.iter().copied())
        .collect();
    let roots: Vec<NodeId> = p
        .nodes
        .iter()
        .map(|n| n.id)
        .filter(|id| !children.contains(id))
        .collect();
    if roots.len() < 2 {
        return imported;
    }
    let next = p
        .nodes
        .iter()
        .map(|n| n.id.0)
        .chain(
            p.resources
                .iter()
                .flat_map(|r| r.content.iter().map(|n| n.id.0)),
        )
        .max()
        .unwrap_or(0)
        + 1;
    let mut group = Node::new(
        NodeId(next),
        NodeKind::Group(Container {
            children: roots,
            clip: None,
        }),
    );
    group.name = name.and_then(vextor_core::clean_name);
    p.nodes.push(group);
    imported
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_text_is_recognised_with_or_without_a_prolog() {
        assert!(looks_like_svg("<svg viewBox=\"0 0 1 1\"/>"));
        assert!(looks_like_svg(
            "\u{feff}  <?xml version=\"1.0\"?>\n<!-- x -->\n<svg/>"
        ));
        assert!(!looks_like_svg("hello <svg>"));
        assert!(!looks_like_svg("<div>no</div>"));
    }

    #[test]
    fn the_summary_lists_what_was_left_out() {
        let mut r = Report {
            objects: 3,
            ..Report::default()
        };
        assert_eq!(r.summary(), "Imported 3 objects.");
        r.drop(Dropped::Filter);
        r.drop(Dropped::Filter);
        r.drop(Dropped::Mask);
        r.outlined.push("font “Inter” is not installed".to_owned());
        r.approximated = 1;
        assert_eq!(
            r.summary(),
            "Imported 3 objects. Left out: 2 filters, 1 mask. 1 text as outlines \
             (font “Inter” is not installed). 1 gradient approximated."
        );
    }

    #[test]
    fn huge_and_compressed_input_is_refused_before_parsing() {
        let fonts = Fonts::default();
        assert_eq!(
            import_svg(&[0x1f, 0x8b, 8, 0], &fonts),
            Err(ImportError::Compressed)
        );
        let big = vec![b' '; usize::try_from(MAX_FILE_BYTES).expect("fits") + 1];
        assert_eq!(import_svg(&big, &fonts), Err(ImportError::TooLarge));
    }
}
