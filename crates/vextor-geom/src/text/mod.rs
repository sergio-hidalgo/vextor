//! Text layout (spec 05c): system fonts via `fontdb`, shaping via
//! `harfrust`, glyph outlines via `skrifa`.

mod fonts;
mod layout;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use kurbo::Rect;
use vextor_core::{FontSpec, NodeId, Text, TextAlign};

pub use fonts::{FaceStyle, FontDb, ResolvedFace};
pub use layout::{LineBox, MissingFont, TextLayout, layout};

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
