//! Laying out a [`Text`]: lines split on `\n`, each shaped (`shape.rs`)
//! then aligned, all in the node's own space (y down,
//! origin at the top-left of the first line's box).

use std::ops::Range;

use kurbo::{BezPath, Point, Rect, Shape};
use vextor_core::{Text, TextAlign};

use super::fonts::FontDb;
use super::shape::{Face, estimate};

/// Where a text's glyphs and lines are, in the node's space.
#[derive(Clone, Debug, Default)]
pub struct TextLayout {
    /// Every glyph outline, as one compound path (non-zero fill).
    pub glyphs: BezPath,
    /// Which elements of `glyphs` each drawn glyph is, in order, with the
    /// characters it draws (one glyph can draw several, e.g. a ligature).
    pub glyph_spans: Vec<GlyphSpan>,
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

/// One glyph inside [`TextLayout::glyphs`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlyphSpan {
    /// Element range of the glyph in `glyphs`.
    pub elements: Range<usize>,
    /// Byte range of the characters it draws in the content.
    pub text: Range<usize>,
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
    let mut glyph_spans = Vec::new();
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
        let base = glyphs.elements().len();
        for el in run.glyphs.elements() {
            glyphs.push(kurbo::Affine::translate((dx, 0.0)) * *el);
        }
        glyph_spans.extend(run.spans.into_iter().map(|s| GlyphSpan {
            elements: s.elements.start + base..s.elements.end + base,
            text: s.text,
        }));
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
        glyph_spans,
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
