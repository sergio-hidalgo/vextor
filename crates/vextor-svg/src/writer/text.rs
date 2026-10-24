//! Text (spec 05c): live `<text>` with one `<tspan>` per line at the
//! baselines Vextor lays out, or the glyph outlines as one `<path>` when
//! the document exports text as outlines; a frame is a `<rect>` behind it,
//! both in a `<g>`.

use vextor_core::{FillRule, Geometry, Node, Rect, Style, Text, TextAlign, Transform, Vec2};
use vextor_geom::{TextLayout, text_box};

use super::{Writer, geometry};
use crate::element::Element;
use crate::escape::font_family;
use crate::path_data::path_data;

impl Writer<'_> {
    pub(super) fn text(&mut self, node: &Node, t: &Text, xf: Transform) -> Option<Element> {
        let layout = self.fonts.layout(node.id, t);
        let (offset, transform) = self.placement(xf, t.paints());
        let frame = t.frame.as_ref().and_then(|f| {
            let b = text_box(t, &layout);
            let geometry = Geometry::Rect {
                rect: Rect::new(b.x0, b.y0, b.width(), b.height()),
                radius: f.radius,
            };
            let mut rect = geometry::element(&geometry, offset, self.num)?;
            let style = Style {
                fill: f.fill,
                stroke: f.stroke.clone(),
            };
            self.paint_attrs(&mut rect, &style, true, FillRule::NonZero);
            Some(rect)
        });
        let glyphs = if self.opts.text_as_outlines {
            self.outlines(&layout, offset)
        } else {
            self.live(t, &layout, offset)
        };
        let mut el = match (frame, glyphs) {
            (None, None) => return None,
            (None, Some(mut glyphs)) => {
                self.id_attr(&mut glyphs, node);
                self.place(&mut glyphs, transform, node.opacity);
                self.paint_attrs(&mut glyphs, &t.style, true, FillRule::NonZero);
                glyphs
            }
            (Some(rect), glyphs) => {
                let mut g = Element::new("g");
                self.id_attr(&mut g, node);
                self.place(&mut g, transform, node.opacity);
                g.children.push(rect);
                if let Some(mut glyphs) = glyphs {
                    self.paint_attrs(&mut glyphs, &t.style, true, FillRule::NonZero);
                    g.children.push(glyphs);
                }
                g
            }
        };
        el.node = Some(node.id);
        Some(el)
    }

    /// The glyph outlines as one `<path>` (no font needed on the page).
    fn outlines(&self, layout: &TextLayout, offset: Vec2) -> Option<Element> {
        let mut glyphs = layout.glyphs.clone();
        glyphs.apply_affine(kurbo::Affine::translate((offset.x, offset.y)));
        let d = path_data(&glyphs, self.num);
        if d.is_empty() {
            return None;
        }
        let mut el = Element::new("path");
        el.set("d", d);
        Some(el)
    }

    /// Live `<text>`: font attributes, then one `<tspan>` per non-empty
    /// line at its baseline, anchored like the text's alignment.
    fn live(&self, t: &Text, layout: &TextLayout, offset: Vec2) -> Option<Element> {
        let mut el = Element::new("text");
        el.set("font-family", font_family(&t.font.family));
        el.set("font-size", self.num.fmt(t.size));
        if t.font.weight != 400 {
            el.set("font-weight", t.font.weight.to_string());
        }
        if t.font.italic {
            el.set("font-style", "italic");
        }
        if self.num.units(t.letter_spacing) != 0 {
            el.set("letter-spacing", self.num.fmt(t.letter_spacing));
        }
        // Vextor's line widths count the spacing after the last character;
        // SVG renderers leave it out when they anchor a line.
        let trailing = t.letter_spacing;
        let (anchor, x) = match t.align {
            TextAlign::Left => (None, layout.bounds.x0),
            TextAlign::Centre => (Some("middle"), layout.bounds.center().x - trailing / 2.0),
            TextAlign::Right => (Some("end"), layout.bounds.x1 - trailing),
        };
        if let Some(anchor) = anchor {
            el.set("text-anchor", anchor);
        }
        if needs_preserve(&t.content) {
            el.set("xml:space", "preserve");
        }
        for line in &layout.lines {
            let content = &t.content[line.range.clone()];
            if content.is_empty() {
                continue;
            }
            let mut span = Element::new("tspan");
            span.set("x", self.num.fmt(x + offset.x));
            span.set("y", self.num.fmt(line.baseline + offset.y));
            span.text = Some(content.to_owned());
            el.children.push(span);
        }
        (!el.children.is_empty()).then_some(el)
    }
}

/// Spaces that SVG would collapse: leading, trailing or doubled.
fn needs_preserve(content: &str) -> bool {
    content
        .split('\n')
        .any(|l| l.starts_with(' ') || l.ends_with(' ') || l.contains("  "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_collapsible_spaces_ask_for_preserve() {
        assert!(!needs_preserve("Buy now\nsave"));
        assert!(needs_preserve("Buy  now"));
        assert!(needs_preserve(" lead"));
        assert!(needs_preserve("ok\ntrail "));
    }
}
