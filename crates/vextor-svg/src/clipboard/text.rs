//! Text in the clipboard SVG (spec 05c): live `<text>`, one `<tspan>` per
//! line, the frame as a `<rect>` behind it; content and family escaped
//! (`security-check` §3).

use std::fmt::Write as _;

use vextor_core::{Geometry, NodeId, Rect, Shape, Style, Text, TextAlign};
use vextor_geom::text_box;

use super::{Writer, num, same, shape_element};

impl Writer<'_> {
    /// A text as live `<text>`: one `<tspan>` per non-empty line at its
    /// baseline, anchored like its alignment; with a frame, a `<rect>`
    /// behind it and both in a `<g>`.
    pub(super) fn text(&mut self, id: NodeId, t: &Text, common: &str, pad: &str) {
        let layout = self.fonts.layout(id, t);
        let mut inner = pad.to_owned();
        if let Some(f) = &t.frame {
            let _ = writeln!(self.body, "{pad}<g{common}>");
            inner.push_str("  ");
            let b = text_box(t, &layout);
            let rect = Shape {
                geometry: Geometry::Rect {
                    rect: Rect::new(b.x0, b.y0, b.width(), b.height()),
                    radius: f.radius,
                },
                style: Style {
                    fill: f.fill,
                    stroke: f.stroke.clone(),
                },
            };
            let paint = self.style_attrs(&rect);
            let _ = writeln!(self.body, "{inner}<{}{paint}/>", shape_element(&rect));
        }
        let own = if t.frame.is_some() { "" } else { common };
        let mut a = format!(
            r#" font-family="{}" font-size="{}""#,
            attr_escape(&font_family(&t.font.family)),
            num(t.size)
        );
        if t.font.weight != 400 {
            let _ = write!(a, r#" font-weight="{}""#, t.font.weight);
        }
        if t.font.italic {
            a.push_str(r#" font-style="italic""#);
        }
        if !same(t.letter_spacing, 0.0) {
            let _ = write!(a, r#" letter-spacing="{}""#, num(t.letter_spacing));
        }
        let (anchor, x) = match t.align {
            TextAlign::Left => ("", layout.bounds.x0),
            TextAlign::Centre => (r#" text-anchor="middle""#, layout.bounds.center().x),
            TextAlign::Right => (r#" text-anchor="end""#, layout.bounds.x1),
        };
        a.push_str(anchor);
        self.paint_attrs(&mut a, "fill", t.style.fill, true);
        if let Some(st) = &t.style.stroke {
            self.stroke_attrs(&mut a, st);
        }
        if needs_preserve(&t.content) {
            a.push_str(r#" xml:space="preserve""#);
        }
        let _ = write!(self.body, "{inner}<text{own}{a}>");
        for line in &layout.lines {
            let content = &t.content[line.range.clone()];
            if content.is_empty() {
                continue;
            }
            let _ = write!(
                self.body,
                r#"<tspan x="{}" y="{}">{}</tspan>"#,
                num(x),
                num(line.baseline),
                xml_escape(content)
            );
        }
        let _ = writeln!(self.body, "</text>");
        if t.frame.is_some() {
            let _ = writeln!(self.body, "{pad}</g>");
        }
    }
}

/// `'Family', sans-serif`: the family quoted for CSS, with the characters
/// that could end the quote or the declaration removed, and a generic
/// fallback for pages without the font.
fn font_family(family: &str) -> String {
    let clean: String = family
        .chars()
        .filter(|c| {
            !matches!(c, '\'' | '"' | '\\' | ';' | '<' | '>' | '{' | '}') && !c.is_control()
        })
        .collect();
    let clean = clean.trim();
    if clean.is_empty() {
        "sans-serif".to_owned()
    } else {
        format!("'{clean}', sans-serif")
    }
}

/// Text with `& < > " '` as entities and characters XML 1.0 does not allow
/// left out.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if c.is_control() || matches!(c, '\u{FFFE}' | '\u{FFFF}') => {}
            c => out.push(c),
        }
    }
    out
}

/// An attribute value in double quotes: `& < > "` as entities; the
/// apostrophes CSS quoting uses stay as they are.
fn attr_escape(s: &str) -> String {
    xml_escape(s).replace("&apos;", "'")
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
    fn markup_and_quotes_are_escaped() {
        assert_eq!(
            xml_escape(r#"<script>alert("x")</script> & 'q'"#),
            "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt; &amp; &apos;q&apos;"
        );
        assert_eq!(xml_escape("a\u{0}b\u{1b}c\u{FFFF}"), "abc");
    }

    #[test]
    fn font_families_cannot_break_out_of_their_quotes() {
        assert_eq!(font_family("Inter"), "'Inter', sans-serif");
        assert_eq!(
            font_family(r#"Evil"' onload="x"; }<b>"#),
            "'Evil onload=x b', sans-serif"
        );
        assert_eq!(font_family("'\""), "sans-serif");
    }

    #[test]
    fn only_collapsible_spaces_ask_for_preserve() {
        assert!(!needs_preserve("Buy now\nsave"));
        assert!(needs_preserve("Buy  now"));
        assert!(needs_preserve(" lead"));
        assert!(needs_preserve("ok\ntrail "));
    }
}
