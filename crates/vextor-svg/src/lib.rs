//! Vextor SVG writer (spec 12) and reader (spec 13). Output rules live in
//! the `svg-output` skill.
//!
//! [`export`] writes an artboard or a selection as an SVG file, inline
//! HTML, a JSX component or a CSS data URI, following the document's
//! [`SvgExportOptions`]. The output is deterministic: the same document
//! and options give the same bytes.

mod element;
mod escape;
mod formats;
mod ids;
mod import;
mod number;
mod path_data;
mod writer;

use std::collections::BTreeSet;

use vextor_core::{ArtboardId, Document, NodeId, Parent, SvgExportOptions, Transform};
use vextor_geom::{Fonts, visual_bounds};

pub use element::Spans;
pub use ids::slug;
pub use import::{
    Dropped, ImportError, Imported, MAX_IMAGE_SIDE, Report, grouped, import_image, import_svg,
    into_document, looks_like_svg,
};

use writer::Target;

/// What to export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope<'a> {
    /// One artboard: its area, its background and the objects on it.
    Artboard(ArtboardId),
    /// These objects (a node whose parent is listed too is written once,
    /// inside it), framed by what they paint.
    Selection(&'a [NodeId]),
}

/// The shape of the code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Format {
    /// A standalone `.svg` file.
    #[default]
    Svg,
    /// `<svg>` to paste into HTML: no `xmlns`, `role="img"` and an
    /// `aria-label`.
    Html,
    /// A React component.
    Jsx,
    /// `url("data:image/svg+xml,…")` for CSS `background` or `mask`.
    CssDataUri,
}

impl Format {
    /// Every format, in menu order.
    pub const ALL: [Self; 4] = [Self::Svg, Self::Html, Self::Jsx, Self::CssDataUri];

    /// The name menus show.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Svg => "SVG",
            Self::Html => "Inline HTML",
            Self::Jsx => "JSX",
            Self::CssDataUri => "CSS data URI",
        }
    }
}

/// Exported code.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Export {
    pub code: String,
    /// Where each drawn node's element is in `code` (none for a data URI).
    pub spans: Spans,
}

/// `scope` of `doc` as `format` with `options`; `None` when the artboard
/// does not exist or the selection paints nothing.
#[must_use]
pub fn export(
    doc: &Document,
    fonts: &Fonts,
    scope: Scope<'_>,
    format: Format,
    options: &SvgExportOptions,
) -> Option<Export> {
    let (target, name) = target(doc, fonts, scope)?;
    let svg = writer::build(doc, fonts, options, &target);
    // A data URI is always one line.
    let minify = options.minify || format == Format::CssDataUri;
    let (code, spans) = formats::finish(svg, format, &name, minify);
    Some(Export { code, spans })
}

/// What `scope` covers, and the name the output is labelled with.
fn target(doc: &Document, fonts: &Fonts, scope: Scope<'_>) -> Option<(Target, String)> {
    match scope {
        Scope::Artboard(id) => {
            let a = doc.artboards.iter().find(|a| a.id == id)?;
            let r = a.rect;
            let area = kurbo::Rect::new(r.x, r.y, r.x + r.width, r.y + r.height);
            let target = Target {
                area,
                roots: doc
                    .layers
                    .iter()
                    .map(|&l| (l, Transform::IDENTITY))
                    .collect(),
                background: a.background,
                within: Some(area),
                key: format!("{}\n{}\n{}", doc.name, a.name, a.id.0),
            };
            Some((target, a.name.clone()))
        }
        Scope::Selection(ids) => {
            let roots = selection_roots(doc, ids);
            let area = roots
                .iter()
                .filter_map(|&id| visual_bounds(doc, fonts, id))
                .reduce(|a, b| a.union(b))?;
            let name = match roots.as_slice() {
                [one] => doc.nodes.get(one).and_then(|n| n.name.clone()),
                _ => None,
            }
            .unwrap_or_else(|| doc.name.clone());
            let key = roots
                .iter()
                .fold(doc.name.clone(), |k, id| format!("{k}\n{}", id.0));
            let target = Target {
                area,
                roots: roots
                    .iter()
                    .map(|&id| (id, doc.parent_transform(id)))
                    .collect(),
                background: None,
                within: None,
                key,
            };
            Some((target, name))
        }
    }
}

/// The selected nodes not inside another selected node, in paint order.
fn selection_roots(doc: &Document, ids: &[NodeId]) -> Vec<NodeId> {
    let selected: BTreeSet<NodeId> = ids.iter().copied().collect();
    let inside_selected = |id: NodeId| {
        let mut at = id;
        while let Some(Parent::Node(p)) = doc.parent_of(at) {
            if selected.contains(&p) {
                return true;
            }
            at = p;
        }
        false
    };
    doc.paint_order()
        .into_iter()
        .filter(|id| selected.contains(id) && !inside_selected(*id))
        .collect()
}
