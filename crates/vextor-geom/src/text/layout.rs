//! Laying out a [`Text`]: lines split on `\n`, shaped with `harfrust`,
//! glyph outlines from `skrifa`, all in the node's own space (y down,
//! origin at the top-left of the first line's box).

use std::ops::Range;

use harfrust::{Direction, ShaperData, ShaperInstance, Tag, UnicodeBuffer, Variation};
use kurbo::{BezPath, Point, Rect, Shape};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{GlyphId, MetadataProvider};
use vextor_core::{Text, TextAlign};

use super::fonts::{FontDb, ResolvedFace};

/// Where a text's glyphs and lines are, in the node's space.
#[derive(Clone, Debug, Default)]
pub struct TextLayout {
    /// Every glyph outline, as one compound path (non-zero fill).
    pub glyphs: BezPath,
    /// One per line of the content, top to bottom; never empty.
    pub lines: Vec<LineBox>,
    /// Union of the line boxes: what selects and resizes the text.
    pub bounds: Rect,
    /// Tight box around the glyph outlines; `None` when nothing is inked.
    pub ink: Option<Rect>,
    /// Set when the text is not drawn in the font it asks for.
    pub missing: Option<MissingFont>,
}

/// One line: its box, baseline and caret stops.
#[derive(Clone, Debug, PartialEq)]
pub struct LineBox {
    /// Byte range of the line in the content, without its `\n`.
    pub range: Range<usize>,
    /// Width = advance of the line (letter spacing included), height =
    /// `size × line_height`.
    pub rect: Rect,
    /// y of the baseline.
    pub baseline: f64,
    /// `(byte index, x)` at every character boundary of the line, from
    /// `range.start` to `range.end` inclusive, ascending.
    pub carets: Vec<(usize, f64)>,
}

/// The requested family is not drawn as asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingFont {
    pub wanted: String,
    /// The family drawn instead; `None` when there are no fonts at all
    /// (yet: system fonts are still loading).
    pub shown: Option<String>,
}

/// Estimated metrics, as fractions of the size, used without a font.
const EST_ASCENT: f64 = 0.8;
const EST_DESCENT: f64 = 0.2;
const EST_ADVANCE: f64 = 0.5;

/// Lays out `text` with the fonts in `fonts`.
#[must_use]
pub fn layout(text: &Text, fonts: &FontDb) -> TextLayout {
    let face = fonts.resolve(&text.font);
    let shaped = face.as_ref().and_then(|f| Face::load(fonts, f, text));
    let missing = match (&face, &shaped) {
        (Some(f), Some(_)) => f.missing.then(|| MissingFont {
            wanted: text.font.family.clone(),
            shown: Some(f.family.clone()),
        }),
        // No fonts, or the face would not parse: estimated, not drawn.
        _ => Some(MissingFont {
            wanted: text.font.family.clone(),
            shown: None,
        }),
    };

    let size = text.size;
    let (ascent, descent) = shaped
        .as_ref()
        .map_or((EST_ASCENT * size, EST_DESCENT * size), |f| {
            (f.ascent, f.descent)
        });
    let line_px = size * text.line_height;
    let half_leading = (line_px - (ascent + descent)) / 2.0;

    let mut glyphs = BezPath::new();
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, line) in text.content.split('\n').enumerate() {
        let range = start..start + line.len();
        start = range.end + 1;
        #[allow(clippy::cast_precision_loss)] // line counts are tiny
        let top = i as f64 * line_px;
        let baseline = top + half_leading + ascent;
        let run = match &shaped {
            Some(f) => f.shape(line, range.start, text.letter_spacing, baseline),
            None => estimate(line, range.start, size, text.letter_spacing),
        };
        lines.push((range, top, baseline, run));
    }

    let widest = lines
        .iter()
        .map(|(.., run)| run.width)
        .fold(0.0_f64, f64::max);
    let mut out_lines = Vec::with_capacity(lines.len());
    for (range, top, baseline, run) in lines {
        let dx = match text.align {
            TextAlign::Left => 0.0,
            TextAlign::Centre => (widest - run.width) / 2.0,
            TextAlign::Right => widest - run.width,
        };
        for el in run.glyphs.elements() {
            glyphs.push(kurbo::Affine::translate((dx, 0.0)) * *el);
        }
        out_lines.push(LineBox {
            range,
            rect: Rect::new(dx, top, dx + run.width, top + line_px),
            baseline,
            carets: run.carets.into_iter().map(|(b, x)| (b, x + dx)).collect(),
        });
    }

    let bounds = out_lines
        .iter()
        .map(|l| l.rect)
        .reduce(|a, b| a.union(b))
        .unwrap_or_default();
    let ink = (!glyphs.is_empty()).then(|| glyphs.bounding_box());
    TextLayout {
        glyphs,
        lines: out_lines,
        bounds,
        ink,
        missing,
    }
}

impl TextLayout {
    /// The line holding byte index `index` and the caret's x there.
    /// Indexes past the end land on the end of the last line.
    #[must_use]
    pub fn caret(&self, index: usize) -> (usize, f64) {
        for (i, line) in self.lines.iter().enumerate() {
            if index <= line.range.end {
                let x = line
                    .carets
                    .iter()
                    .find(|(b, _)| *b >= index)
                    .map_or(line.rect.x1, |(_, x)| *x);
                return (i, x);
            }
        }
        let last = self.lines.len().saturating_sub(1);
        (last, self.lines.last().map_or(0.0, |l| l.rect.x1))
    }

    /// The byte index whose caret is nearest to `p` (node space).
    #[must_use]
    pub fn index_at(&self, p: Point) -> usize {
        let line = self
            .lines
            .iter()
            .find(|l| p.y < l.rect.y1)
            .or(self.lines.last());
        line.and_then(|l| {
            l.carets
                .iter()
                .min_by(|a, b| (a.1 - p.x).abs().total_cmp(&(b.1 - p.x).abs()))
                .map(|(b, _)| *b)
        })
        .unwrap_or(0)
    }
}

/// A shaped line before alignment.
struct Run {
    glyphs: BezPath,
    width: f64,
    carets: Vec<(usize, f64)>,
}

/// A loaded, parsed face at the text's size and weight.
struct Face {
    data: std::sync::Arc<Vec<u8>>,
    index: u32,
    /// px per font unit.
    scale: f64,
    size: f64,
    ascent: f64,
    descent: f64,
    /// `wght` value for a variable font, when it has the axis.
    weight: Option<f32>,
}

impl Face {
    fn load(fonts: &FontDb, face: &ResolvedFace, text: &Text) -> Option<Self> {
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
    fn shape(&self, line: &str, offset: usize, spacing: f64, baseline: f64) -> Run {
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
        };
        let mut carets = Vec::with_capacity(line.len() + 1);
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
                if let Some(glyph) = outlines.get(GlyphId::new(infos[g].glyph_id)) {
                    // A glyph that fails to draw is left out, never a panic.
                    let _ = glyph.draw(DrawSettings::unhinted(px, &location), &mut pen);
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
fn estimate(line: &str, offset: usize, size: f64, spacing: f64) -> Run {
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
        width: x,
        carets,
    }
}

/// Collects glyph outlines, flipping font y-up into node y-down.
struct Pen {
    path: BezPath,
    origin: Point,
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
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.path.line_to(p);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (c, p) = (self.p(cx0, cy0), self.p(x, y));
        self.path.quad_to(c, p);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (c0, c1, p) = (self.p(cx0, cy0), self.p(cx1, cy1), self.p(x, y));
        self.path.curve_to(c0, c1, p);
    }
    fn close(&mut self) {
        self.path.close_path();
    }
}
