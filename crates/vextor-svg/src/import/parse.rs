//! Parsing an SVG with `usvg`: no file is ever read (`<image>` must
//! embed PNG, JPEG or WebP data) and text is laid out with our fonts.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use vextor_core::ImageMime;
use vextor_geom::Fonts;

use super::ImportError;
use super::image::{Raster, sniff};

/// The colour `currentColor` resolves to while importing, read back as
/// [`vextor_core::Paint::CurrentColor`]. Unlikely in a real drawing.
pub(super) const CURRENT_COLOR: vextor_core::Rgba = vextor_core::Rgba::rgb(1, 2, 3);

/// The `usvg` tree of `xml`: no file is ever read (`<image>` must embed
/// PNG, JPEG or WebP data), text is laid out with `fonts`.
pub(super) fn tree(
    xml: &usvg::roxmltree::Document<'_>,
    fonts: &Fonts,
) -> Result<Tree, ImportError> {
    let rejected = Arc::new(AtomicUsize::new(0));
    let (r1, r2) = (Arc::clone(&rejected), Arc::clone(&rejected));
    let mut options = usvg::Options {
        resources_dir: None,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(move |_mime, data, _| {
                let kind = embedded(&data);
                if kind.is_none() {
                    r1.fetch_add(1, Ordering::Relaxed);
                }
                kind
            }),
            // A path or URL: never opened.
            resolve_string: Box::new(move |_, _| {
                r2.fetch_add(1, Ordering::Relaxed);
                None
            }),
        },
        // `currentColor` resolves to the inherited `color`; the root's is set
        // to a sentinel so the paint can be told apart and kept.
        style_sheet: Some(format!(
            "svg {{ color: rgb({}, {}, {}) }}",
            CURRENT_COLOR.r, CURRENT_COLOR.g, CURRENT_COLOR.b
        )),
        ..usvg::Options::default()
    };
    let db = fonts.db();
    if xml.descendants().any(|n| n.tag_name().name() == "text") {
        let mut fontdb = db.database().clone();
        // A missing family falls back to a generic one: make those ours,
        // so text in a missing font still has outlines.
        if let Some(family) = db.fallback_family() {
            family.clone_into(&mut options.font_family);
            for set in [
                usvg::fontdb::Database::set_serif_family,
                usvg::fontdb::Database::set_sans_serif_family,
                usvg::fontdb::Database::set_monospace_family,
                usvg::fontdb::Database::set_cursive_family,
                usvg::fontdb::Database::set_fantasy_family,
            ] {
                set(&mut fontdb, family);
            }
        }
        options.fontdb = Arc::new(fontdb);
    }
    let tree = usvg::Tree::from_xmltree(xml, &options).map_err(|e| match e {
        // usvg's own bound on what `<use>` loops expand to.
        usvg::Error::ElementsLimitReached
        | usvg::Error::ParsingFailed(usvg::roxmltree::Error::NodesLimitReached) => {
            ImportError::TooManyObjects
        }
        e => ImportError::Invalid(e.to_string()),
    })?;
    let size = tree.size();
    if !(size.width().is_finite() && size.height().is_finite()) {
        return Err(ImportError::Invalid("its size is not a number".to_owned()));
    }
    let ids = xml
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .map(str::to_owned)
        .collect();
    Ok(Tree {
        parsed: tree,
        ids,
        rejected_images: rejected.load(Ordering::Relaxed),
    })
}

/// A parsed SVG and how many images it asked for that were refused.
pub(super) struct Tree {
    pub(super) parsed: usvg::Tree,
    /// Every `id` in the file: usvg makes up ids for copies it creates,
    /// which must not become names.
    pub(super) ids: HashSet<String>,
    pub(super) rejected_images: usize,
}

impl Tree {
    pub(super) fn size(&self) -> usvg::Size {
        self.parsed.size()
    }
}

/// An embedded image `usvg` may keep: PNG, JPEG or WebP whose header
/// says it is within [`super::MAX_IMAGE_SIDE`].
fn embedded(data: &Arc<Vec<u8>>) -> Option<usvg::ImageKind> {
    let Raster { mime, .. } = sniff(data).ok()?;
    let data = Arc::clone(data);
    Some(match mime {
        ImageMime::Png => usvg::ImageKind::PNG(data),
        ImageMime::Jpeg => usvg::ImageKind::JPEG(data),
        ImageMime::Webp => usvg::ImageKind::WEBP(data),
    })
}
