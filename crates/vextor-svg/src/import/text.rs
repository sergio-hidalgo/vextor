//! `<text>` → a text object when it fits the model (spec 05c: one style,
//! lines one under the other, an installed font), else its outlines as
//! `usvg` laid them out, with the reason in the report.
//!
//! The writer puts each line in a `<tspan x y>` at the baseline Vextor
//! lays out; here that is undone: the first line's baseline gives the
//! text's position, the spacing of the lines its line height.

use vextor_core::{
    EPS, FONT_WEIGHTS, FontSpec, MAX_TEXT_CHARS, Node, NodeId, NodeKind, Paint, Style, Text,
    TextAlign, Transform,
};

use super::Dropped;
use super::nodes::{Mapper, push, rect};

/// How far (px) a line may sit from even spacing and still be a line of
/// the same text: the writer rounds baselines to its precision.
const LINE_TOLERANCE: f64 = 0.02;

impl Mapper<'_> {
    pub(super) fn text(
        &mut self,
        t: &usvg::Text,
        depth: usize,
        out: &mut Vec<Node>,
    ) -> Option<NodeId> {
        match self.live_text(t) {
            Ok(node) => Some(push(out, node)),
            Err(reason) => {
                let flat = t.flattened();
                let id = self.group(flat, depth, out);
                match id {
                    Some(id) => {
                        self.report.outlined.push(reason);
                        if let Some(n) = out.iter_mut().rev().find(|n| n.id == id)
                            && n.name.is_none()
                        {
                            n.name = self.name(t.id());
                        }
                    }
                    None => self.report.drop(Dropped::Text),
                }
                id
            }
        }
    }

    /// `t` as a text node, or why it cannot be one.
    fn live_text(&mut self, t: &usvg::Text) -> Result<Node, String> {
        let lines = Lines::of(t)?;
        let size = f64::from(lines.span.font_size().get());
        let db = self.fonts.db();
        let (font, wanted) = font_spec(lines.span.font(), db)?;
        let mut text = Text::new(lines.content, font.family.clone());
        text.font = font;
        text.size = size;
        text.line_height = line_height(&lines.baselines, size)?;
        text.letter_spacing = f64::from(lines.span.letter_spacing());
        text.align = match lines.anchor {
            usvg::TextAnchor::Start => TextAlign::Left,
            usvg::TextAnchor::Middle => TextAlign::Centre,
            usvg::TextAnchor::End => TextAlign::Right,
        };
        let layout = vextor_geom::layout(&text, db);
        if layout.missing.is_some() {
            return Err(format!("font “{wanted}” is not installed in that style"));
        }
        let bbox = rect(t.bounding_box());
        text.style = Style {
            fill: lines
                .span
                .fill()
                .map_or(Paint::None, |f| self.paint(f.paint(), f.opacity(), bbox)),
            stroke: lines.span.stroke().and_then(|s| self.stroke(s, bbox)),
        };
        // Where the writer put the first line (see `writer/text.rs`).
        let spacing = text.letter_spacing;
        let b = layout.bounds;
        let anchor_x = match text.align {
            TextAlign::Left => b.x0,
            TextAlign::Centre => b.center().x - spacing / 2.0,
            TextAlign::Right => b.x1 - spacing,
        };
        let baseline = layout.lines.first().map_or(0.0, |l| l.baseline);
        let id = self.node_id();
        let mut node = Node::new(id, NodeKind::Text(text));
        node.transform = Transform::translate(lines.x - anchor_x, lines.baselines[0] - baseline);
        node.name = self.name(t.id());
        Ok(node)
    }
}

/// A `<text>`'s lines, when they fit the model: one style, one x, one
/// anchor, each line its own chunk.
struct Lines<'a> {
    span: &'a usvg::TextSpan,
    anchor: usvg::TextAnchor,
    x: f64,
    baselines: Vec<f64>,
    content: String,
}

impl<'a> Lines<'a> {
    fn of(t: &'a usvg::Text) -> Result<Self, String> {
        let chunks = t.chunks();
        let first = chunks.first().ok_or("it is empty")?;
        let span = single_span(first)?;
        if t.writing_mode() != usvg::WritingMode::LeftToRight {
            return Err("it is vertical".to_owned());
        }
        let moved = t.dx().iter().chain(t.dy()).chain(t.rotate());
        if moved.into_iter().any(|v| v.abs() > 1e-6) {
            return Err("its letters are moved or rotated one by one".to_owned());
        }
        let x0 = first.x().ok_or("it has no position")?;
        let mut baselines = Vec::with_capacity(chunks.len());
        let mut lines = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            if !same_style(single_span(chunk)?, span) {
                return Err("it mixes styles".to_owned());
            }
            let (Some(x), Some(y)) = (chunk.x(), chunk.y()) else {
                return Err("its lines are not placed one under the other".to_owned());
            };
            if (x - x0).abs() > 1e-3 || chunk.anchor() != first.anchor() {
                return Err("its lines are not aligned alike".to_owned());
            }
            baselines.push(f64::from(y));
            lines.push(chunk.text());
        }
        let content = lines.join("\n");
        if content.chars().count() > MAX_TEXT_CHARS {
            return Err(format!("it is longer than {MAX_TEXT_CHARS} characters"));
        }
        Ok(Self {
            span,
            anchor: first.anchor(),
            x: f64::from(x0),
            baselines,
            content,
        })
    }
}

/// The model font for `font` (its first named family, installed), and the
/// family as the file wrote it.
fn font_spec<'a>(
    font: &'a usvg::Font,
    db: &vextor_geom::FontDb,
) -> Result<(FontSpec, &'a str), String> {
    let wanted = font
        .families()
        .iter()
        .find_map(|f| match f {
            usvg::FontFamily::Named(n) => Some(n.as_str()),
            _ => None,
        })
        .ok_or("it asks for a generic font")?;
    let family = db
        .family(wanted)
        .ok_or_else(|| format!("font “{wanted}” is not installed"))?
        .to_owned();
    if font.stretch() != usvg::FontStretch::Normal {
        return Err("its font is condensed or expanded".to_owned());
    }
    let spec = FontSpec {
        family,
        weight: font
            .weight()
            .clamp(*FONT_WEIGHTS.start(), *FONT_WEIGHTS.end()),
        italic: font.style() != usvg::FontStyle::Normal,
    };
    Ok((spec, wanted))
}

/// The one span of `chunk`, covering all of it, in a style the model has.
fn single_span(chunk: &usvg::TextChunk) -> Result<&usvg::TextSpan, String> {
    let [span] = chunk.spans() else {
        return Err("it mixes styles".to_owned());
    };
    if !matches!(chunk.text_flow(), usvg::TextFlow::Linear) {
        return Err("it runs along a path".to_owned());
    }
    if span.start() != 0 || span.end() != chunk.text().len() {
        return Err("it mixes styles".to_owned());
    }
    let d = span.decoration();
    let plain = d.underline().is_none()
        && d.overline().is_none()
        && d.line_through().is_none()
        && span.baseline_shift().is_empty()
        && span.dominant_baseline() == usvg::DominantBaseline::Auto
        && !span.small_caps()
        && span.text_length().is_none()
        && span.word_spacing().abs() < 1e-6
        && span.is_visible()
        && span.paint_order() == usvg::PaintOrder::FillAndStroke;
    if !plain {
        return Err("it uses text styling vextor does not have".to_owned());
    }
    Ok(span)
}

fn same_style(a: &usvg::TextSpan, b: &usvg::TextSpan) -> bool {
    let font = |s: &usvg::TextSpan| {
        let f = s.font();
        (f.families().to_vec(), f.weight(), f.style(), f.stretch())
    };
    font(a) == font(b)
        && a.font_size() == b.font_size()
        && (a.letter_spacing() - b.letter_spacing()).abs() < 1e-6
        && same_fill(a.fill(), b.fill())
        && same_stroke(a.stroke(), b.stroke())
}

fn same_fill(a: Option<&usvg::Fill>, b: Option<&usvg::Fill>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.paint() == b.paint() && a.opacity() == b.opacity(),
        _ => false,
    }
}

fn same_stroke(a: Option<&usvg::Stroke>, b: Option<&usvg::Stroke>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.paint() == b.paint()
                && a.opacity() == b.opacity()
                && a.width() == b.width()
                && a.linecap() == b.linecap()
                && a.linejoin() == b.linejoin()
                && a.miterlimit() == b.miterlimit()
                && a.dasharray() == b.dasharray()
                && a.dashoffset().to_bits() == b.dashoffset().to_bits()
        }
        _ => false,
    }
}

/// The line height (× size) that spaces `baselines` evenly.
// Line counts are bounded by `MAX_TEXT_CHARS`: exact in an f64.
#[allow(clippy::cast_precision_loss)]
fn line_height(baselines: &[f64], size: f64) -> Result<f64, String> {
    let default = Text::new("", "x").line_height;
    let (Some(&first), Some(&last)) = (baselines.first(), baselines.last()) else {
        return Ok(default);
    };
    if baselines.len() < 2 {
        return Ok(default);
    }
    let gaps = (baselines.len() - 1) as f64;
    let step = (last - first) / gaps;
    let even = baselines
        .iter()
        .enumerate()
        .all(|(i, y)| (y - (first + step * i as f64)).abs() <= LINE_TOLERANCE);
    if step <= EPS || !even {
        return Err("its lines are not evenly spaced".to_owned());
    }
    Ok(step / size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evenly_spaced_baselines_give_the_line_height() {
        let lh = line_height(&[10.0, 29.2, 48.4], 16.0).expect("even");
        assert!((lh - 1.2).abs() < 1e-9);
        assert!(line_height(&[10.0, 20.0, 50.0], 16.0).is_err());
        assert!((line_height(&[10.0], 16.0).expect("one line") - 1.2).abs() < 1e-9);
    }
}
