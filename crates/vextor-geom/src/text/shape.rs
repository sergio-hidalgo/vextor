//! Shaping one line of a [`Text`] with `harfrust` and drawing its glyph
//! outlines with `skrifa` (with letter spacing and caret positions), or
//! estimating it when no font can be read.

use harfrust::{Direction, ShaperData, ShaperInstance, Tag, UnicodeBuffer, Variation};
use kurbo::{BezPath, Point};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{GlyphId, MetadataProvider};
use vextor_core::Text;

use super::fonts::{FontDb, ResolvedFace};
use super::layout::GlyphSpan;

/// Estimated advance, as a fraction of the size, used without a font.
const EST_ADVANCE: f64 = 0.5;

/// A shaped line before alignment.
pub(super) struct Run {
    pub(super) glyphs: BezPath,
    /// Element ranges relative to `glyphs`.
    pub(super) spans: Vec<GlyphSpan>,
    pub(super) width: f64,
    pub(super) carets: Vec<(usize, f64)>,
}

/// A loaded, parsed face at the text's size and weight.
pub(super) struct Face {
    pub(super) data: std::sync::Arc<Vec<u8>>,
    pub(super) index: u32,
    /// px per font unit.
    pub(super) scale: f64,
    pub(super) size: f64,
    pub(super) ascent: f64,
    pub(super) descent: f64,
    /// `wght` value for a variable font, when it has the axis.
    pub(super) weight: Option<f32>,
}

impl Face {
    pub(super) fn load(fonts: &FontDb, face: &ResolvedFace, text: &Text) -> Option<Self> {
        let (data, index) = fonts.face_data(face.id)?;
        let font = skrifa::FontRef::from_index(&data, index).ok()?;
        let units = f64::from(
            font.metrics(Size::unscaled(), LocationRef::default())
                .units_per_em,
        );
        if units <= 0.0 {
            return None;
        }
        let weight = font
            .axes()
            .iter()
            .any(|a| a.tag() == Tag::new(b"wght"))
            .then_some(f32::from(text.font.weight));
        let location = font.axes().location(weight.map(|w| ("wght", w)));
        let m = font.metrics(Size::unscaled(), &location);
        let scale = text.size / units;
        Some(Self {
            scale,
            size: text.size,
            ascent: f64::from(m.ascent) * scale,
            descent: -f64::from(m.descent) * scale,
            weight,
            data: data.clone(),
            index,
        })
    }

    /// Shapes `line` (starting at byte `offset` of the content) left to
    /// right with its baseline at `baseline`.
    pub(super) fn shape(&self, line: &str, offset: usize, spacing: f64, baseline: f64) -> Run {
        let fallback = || estimate(line, offset, self.size, spacing);
        let (Ok(font), Ok(hb)) = (
            skrifa::FontRef::from_index(&self.data, self.index),
            harfrust::FontRef::from_index(&self.data, self.index),
        ) else {
            return fallback();
        };
        let variations: Vec<Variation> = self
            .weight
            .map(|value| Variation {
                tag: Tag::new(b"wght"),
                value,
            })
            .into_iter()
            .collect();
        let instance = ShaperInstance::from_variations(&hb, &variations);
        let shaper_data = ShaperData::new(&hb);
        let shaper = shaper_data.shaper(&hb).instance(Some(&instance)).build();
        let mut buf = UnicodeBuffer::new();
        buf.push_str(line);
        buf.guess_segment_properties();
        // Bidirectional text is out of scope: every line runs left to right.
        buf.set_direction(Direction::LeftToRight);
        let out = shaper.shape(buf, harfrust::ShapeOptions::new());

        let location = font.axes().location(self.weight.map(|w| ("wght", w)));
        #[allow(clippy::cast_possible_truncation)] // font sizes fit f32
        let px = Size::new(self.size as f32);
        let outlines = font.outline_glyphs();

        let infos = out.glyph_infos();
        let positions = out.glyph_positions();
        let clusters = cluster_starts(infos, line.len());
        let mut pen = Pen {
            path: BezPath::new(),
            origin: Point::ZERO,
            last: Point::ZERO,
            start: Point::ZERO,
        };
        let mut carets = Vec::with_capacity(line.len() + 1);
        let mut spans = Vec::new();
        let mut x = 0.0;
        let mut g = 0;
        for (k, &start) in clusters.iter().enumerate() {
            let end = clusters.get(k + 1).copied().unwrap_or(line.len());
            let cluster_x = x;
            while g < infos.len() && infos[g].cluster as usize == start {
                let pos = positions[g];
                pen.origin = Point::new(
                    x + f64::from(pos.x_offset) * self.scale,
                    baseline - f64::from(pos.y_offset) * self.scale,
                );
                let before = pen.path.elements().len();
                if let Some(glyph) = outlines.get(GlyphId::new(infos[g].glyph_id)) {
                    // A glyph that fails to draw is left out, never a panic.
                    let _ = glyph.draw(DrawSettings::unhinted(px, &location), &mut pen);
                }
                let after = pen.path.elements().len();
                if after > before {
                    spans.push(GlyphSpan {
                        elements: before..after,
                        text: offset + start..offset + end,
                    });
                }
                x += f64::from(pos.x_advance) * self.scale;
                g += 1;
            }
            let chars: Vec<usize> = line[start..end].char_indices().map(|(i, _)| i).collect();
            #[allow(clippy::cast_precision_loss)] // per-cluster char counts are tiny
            let n = chars.len().max(1) as f64;
            x += spacing * n;
            let width = x - cluster_x;
            for (j, rel) in chars.iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                carets.push((offset + start + rel, cluster_x + width * j as f64 / n));
            }
        }
        carets.push((offset + line.len(), x));
        Run {
            glyphs: pen.path,
            spans,
            width: x,
            carets,
        }
    }
}

/// The distinct cluster start bytes in glyph order, ascending; a glyph
/// run that covers no character (or a buffer that skipped some) still
/// yields a start at 0 so every character belongs to a cluster.
fn cluster_starts(infos: &[harfrust::GlyphInfo], len: usize) -> Vec<usize> {
    let mut starts: Vec<usize> = infos
        .iter()
        .map(|i| i.cluster as usize)
        .filter(|&c| c < len)
        .collect();
    starts.sort_unstable();
    starts.dedup();
    if len > 0 && starts.first() != Some(&0) {
        starts.insert(0, 0);
    }
    starts
}

/// A line measured without a font: fixed advance per character, no glyphs.
pub(super) fn estimate(line: &str, offset: usize, size: f64, spacing: f64) -> Run {
    let step = EST_ADVANCE * size + spacing;
    let mut carets: Vec<(usize, f64)> = Vec::with_capacity(line.len() + 1);
    let mut x = 0.0;
    for (i, _) in line.char_indices() {
        carets.push((offset + i, x));
        x += step;
    }
    carets.push((offset + line.len(), x));
    Run {
        glyphs: BezPath::new(),
        spans: Vec::new(),
        width: x,
        carets,
    }
}

/// Collects glyph outlines, flipping font y-up into node y-down.
/// Quadratics are raised to cubics, as editable paths store them, so text
/// and its converted outlines are the same curves.
struct Pen {
    path: BezPath,
    origin: Point,
    /// Current point, for raising quadratics.
    last: Point,
    /// Start of the current contour (the current point after a close).
    start: Point,
}

impl Pen {
    fn p(&self, x: f32, y: f32) -> Point {
        Point::new(self.origin.x + f64::from(x), self.origin.y - f64::from(y))
    }
}

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.path.move_to(p);
        self.last = p;
        self.start = p;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.path.line_to(p);
        self.last = p;
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (q, p) = (self.p(cx0, cy0), self.p(x, y));
        let c1 = self.last + (q - self.last) * (2.0 / 3.0);
        let c2 = p + (q - p) * (2.0 / 3.0);
        self.path.curve_to(c1, c2, p);
        self.last = p;
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (c0, c1, p) = (self.p(cx0, cy0), self.p(cx1, cy1), self.p(x, y));
        self.path.curve_to(c0, c1, p);
        self.last = p;
    }
    fn close(&mut self) {
        self.path.close_path();
        self.last = self.start;
    }
}
