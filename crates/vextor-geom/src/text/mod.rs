//! Text layout (spec 05c): system fonts via `fontdb`, shaping via
//! `harfrust`, glyph outlines via `skrifa`.

mod fonts;
mod layout;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use kurbo::{BezPath, Rect};
use vextor_core::{FillRule, FontSpec, Geometry, NodeId, Shape, Style, Text, TextAlign};

use crate::convert::bezpath_to_path;

pub use fonts::{FaceStyle, FontDb, ResolvedFace};
pub use layout::{GlyphSpan, LineBox, MissingFont, TextLayout, layout};

/// Most layouts kept; the cache starts over beyond it.
const CACHE_LIMIT: usize = 4_096;

/// The fonts plus a cache of text layouts, shared by everything that needs
/// to know where text is (hit-testing, bounds, rendering, export).
#[derive(Default)]
pub struct Fonts {
    db: Arc<FontDb>,
    cache: Mutex<HashMap<NodeId, (Key, Arc<TextLayout>)>>,
}

/// What a layout depends on; paint and frame are left out.
#[derive(Clone)]
struct Key {
    content: String,
    font: FontSpec,
    size: f64,
    line_height: f64,
    letter_spacing: f64,
    align: TextAlign,
}

impl Key {
    fn of(t: &Text) -> Self {
        Self {
            content: t.content.clone(),
            font: t.font.clone(),
            size: t.size,
            line_height: t.line_height,
            letter_spacing: t.letter_spacing,
            align: t.align,
        }
    }

    fn matches(&self, t: &Text) -> bool {
        self.content == t.content
            && self.font == t.font
            && self.size.to_bits() == t.size.to_bits()
            && self.line_height.to_bits() == t.line_height.to_bits()
            && self.letter_spacing.to_bits() == t.letter_spacing.to_bits()
            && self.align == t.align
    }
}

impl Fonts {
    /// Layouts over `db`.
    #[must_use]
    pub fn new(db: Arc<FontDb>) -> Self {
        Self {
            db,
            cache: Mutex::default(),
        }
    }

    /// The font database.
    #[must_use]
    pub fn db(&self) -> &Arc<FontDb> {
        &self.db
    }

    /// The layout of text node `id` holding `text`, cached until the text
    /// changes.
    #[must_use]
    pub fn layout(&self, id: NodeId, text: &Text) -> Arc<TextLayout> {
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((key, l)) = cache.get(&id)
            && key.matches(text)
        {
            return Arc::clone(l);
        }
        let l = Arc::new(layout(text, &self.db));
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(id, (Key::of(text), Arc::clone(&l)));
        l
    }
}

/// The box a text selects and resizes by: its line boxes, grown by the
/// frame's padding when it has a frame (node space).
#[must_use]
pub fn text_box(text: &Text, layout: &TextLayout) -> Rect {
    let b = layout.bounds;
    match &text.frame {
        Some(f) => {
            let [top, right, bottom, left] = f.padding;
            Rect::new(b.x0 - left, b.y0 - top, b.x1 + right, b.y1 + bottom)
        }
        None => b,
    }
}

/// A text as ordinary shapes (Convert to Outlines), in the text's own
/// space: its frame as a rounded rect (`None` without a frame), then one
/// path per glyph with the text's paint, named after the characters it
/// draws, so the letters come apart when the result is ungrouped. Blank
/// glyphs (spaces) and text not laid out with a font give no paths.
#[must_use]
pub fn text_outlines(text: &Text, layout: &TextLayout) -> (Option<Shape>, Vec<(String, Shape)>) {
    let frame = text.frame.as_ref().map(|f| {
        let b = text_box(text, layout);
        Shape {
            geometry: Geometry::Rect {
                rect: vextor_core::Rect::new(b.x0, b.y0, b.width(), b.height()),
                radius: f.radius,
            },
            style: Style {
                fill: f.fill,
                stroke: f.stroke.clone(),
            },
        }
    });
    let elements = layout.glyphs.elements();
    let glyphs = layout
        .glyph_spans
        .iter()
        .filter_map(|span| {
            let path = BezPath::from_vec(elements.get(span.elements.clone())?.to_vec());
            let name = text
                .content
                .get(span.text.clone())
                .unwrap_or_default()
                .to_owned();
            let shape = Shape {
                geometry: Geometry::Path(bezpath_to_path(&path, FillRule::NonZero)),
                style: text.style.clone(),
            };
            Some((name, shape))
        })
        .collect();
    (frame, glyphs)
}
