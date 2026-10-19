//! A minimal SVG writer for the system clipboard (spec 05): copied objects
//! as one standalone `<svg>`, framed by their bounds.
//!
//! Covers shapes, groups, transforms, opacity, solid paints, strokes, clips
//! and text (spec 05c: live `<text>`, one `<tspan>` per line at the
//! baselines Vextor lays out, the frame as a `<rect>` behind it). Not yet
//! (full export is spec 12): gradients are written as their first stop's
//! colour, patterns as no paint, images are left out. The only document
//! strings written are text content and font family names, both escaped
//! (`security-check` §3).

use std::fmt::Write as _;

use kurbo::{BezPath, PathEl};
use vextor_core::{
    ClipboardPayload, Document, EPS, FillRule, Geometry, LineCap, LineJoin, Node, NodeId, NodeKind,
    Paint, Rect, Resource, Shape, Stroke, Style, Text, TextAlign, Transform,
};
use vextor_geom::{Fonts, text_box, to_bezpath, union_bounds};

/// Decimals written for every number.
const PRECISION: usize = 2;

/// The copied objects as a standalone SVG, or `None` when nothing visible
/// was copied.
#[must_use]
pub fn clipboard_svg(payload: &ClipboardPayload, fonts: &Fonts) -> Option<String> {
    // Paste into an empty scratch document: ids stay unique, resources
    // resolve, and the roots land at their world positions.
    let mut doc = Document::new("clipboard");
    let layer = doc.layers[0];
    let (cmd, roots) = doc
        .paste(payload, layer, 0, Transform::IDENTITY, "Copy")
        .ok()?;
    doc.apply(cmd).ok()?;
    let b = union_bounds(&doc, fonts, &roots)?;

    let mut w = Writer {
        doc: &doc,
        fonts,
        body: String::new(),
        defs: String::new(),
        clips: 0,
    };
    for &id in &roots {
        w.node(id, 1);
    }
    let mut out = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}">"#,
        num(b.x0),
        num(b.y0),
        num(b.width()),
        num(b.height())
    );
    out.push('\n');
    if !w.defs.is_empty() {
        let _ = writeln!(out, "  <defs>\n{}  </defs>", w.defs);
    }
    out.push_str(&w.body);
    out.push_str("</svg>\n");
    Some(out)
}

struct Writer<'a> {
    doc: &'a Document,
    fonts: &'a Fonts,
    body: String,
    defs: String,
    clips: usize,
}

impl Writer<'_> {
    fn node(&mut self, id: NodeId, depth: usize) {
        let Some(node) = self.doc.nodes.get(&id) else {
            return;
        };
        if !node.visible || depth > 64 {
            return;
        }
        let pad = "  ".repeat(depth);
        let common = common_attrs(node);
        match &node.kind {
            NodeKind::Layer(c) | NodeKind::Group(c) => {
                let clip = c.clip.and_then(|clip| self.clip_path(clip));
                let clip_attr = clip
                    .map(|n| format!(r#" clip-path="url(#clip-{n})""#))
                    .unwrap_or_default();
                let _ = writeln!(self.body, "{pad}<g{common}{clip_attr}>");
                for &child in c.children.iter().filter(|&&ch| Some(ch) != c.clip) {
                    self.node(child, depth + 1);
                }
                let _ = writeln!(self.body, "{pad}</g>");
            }
            NodeKind::Shape(s) => {
                let element = shape_element(s);
                let paint = self.style_attrs(s);
                let _ = writeln!(self.body, "{pad}<{element}{common}{paint}/>");
            }
            NodeKind::Text(t) => self.text(id, t, &common, &pad),
            NodeKind::Image(_) => {}
        }
    }

    /// A text as live `<text>`: one `<tspan>` per non-empty line at its
    /// baseline, anchored like its alignment; with a frame, a `<rect>`
    /// behind it and both in a `<g>`.
    fn text(&mut self, id: NodeId, t: &Text, common: &str, pad: &str) {
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

    /// Writes a `<clipPath>` for shape `id`; its number, or `None`.
    fn clip_path(&mut self, id: NodeId) -> Option<usize> {
        let node = self.doc.nodes.get(&id)?;
        let NodeKind::Shape(s) = &node.kind else {
            return None;
        };
        self.clips += 1;
        let n = self.clips;
        let element = shape_element(s);
        let transform = transform_attr(node.transform);
        let rule = match fill_rule(s) {
            FillRule::EvenOdd => r#" clip-rule="evenodd""#,
            FillRule::NonZero => "",
        };
        let _ = writeln!(
            self.defs,
            r#"    <clipPath id="clip-{n}"><{element}{transform}{rule}/></clipPath>"#
        );
        Some(n)
    }

    fn style_attrs(&self, s: &Shape) -> String {
        let mut a = String::new();
        if !matches!(s.geometry, Geometry::Line { .. }) {
            self.paint_attrs(&mut a, "fill", s.style.fill, true);
            if fill_rule(s) == FillRule::EvenOdd {
                a.push_str(r#" fill-rule="evenodd""#);
            }
        }
        if let Some(st) = &s.style.stroke {
            self.stroke_attrs(&mut a, st);
        }
        a
    }

    /// `name="…"` (and `name-opacity`) unless it is the SVG default.
    fn paint_attrs(&self, a: &mut String, name: &str, paint: Paint, is_fill: bool) {
        let (value, alpha) = match paint {
            Paint::None | Paint::Pattern(_) => ("none".to_owned(), 255),
            Paint::CurrentColor => ("currentColor".to_owned(), 255),
            Paint::Solid(c) => (color(c), c.a),
            Paint::Gradient(r) => match self.doc.resources.get(&r) {
                Some(Resource::Gradient(g)) => g
                    .stops
                    .first()
                    .map_or_else(|| ("none".to_owned(), 255), |s| (color(s.color), s.color.a)),
                _ => ("none".to_owned(), 255),
            },
        };
        let default = if is_fill { "#000" } else { "none" };
        if value != default {
            let _ = write!(a, r#" {name}="{value}""#);
        }
        if alpha < 255 && value != "none" {
            let _ = write!(a, r#" {name}-opacity="{}""#, num(f64::from(alpha) / 255.0));
        }
    }

    fn stroke_attrs(&self, a: &mut String, st: &Stroke) {
        self.paint_attrs(a, "stroke", st.paint, false);
        if st.paint == Paint::None {
            return;
        }
        if !same(st.width, 1.0) {
            let _ = write!(a, r#" stroke-width="{}""#, num(st.width));
        }
        match st.cap {
            LineCap::Butt => {}
            LineCap::Round => a.push_str(r#" stroke-linecap="round""#),
            LineCap::Square => a.push_str(r#" stroke-linecap="square""#),
        }
        match st.join {
            LineJoin::Miter => {}
            LineJoin::Round => a.push_str(r#" stroke-linejoin="round""#),
            LineJoin::Bevel => a.push_str(r#" stroke-linejoin="bevel""#),
        }
        if !same(st.miter_limit, 4.0) && st.join == LineJoin::Miter {
            let _ = write!(a, r#" stroke-miterlimit="{}""#, num(st.miter_limit));
        }
        if !st.dash.is_empty() {
            let dash: Vec<String> = st.dash.iter().map(|d| num(*d)).collect();
            let _ = write!(a, r#" stroke-dasharray="{}""#, dash.join(" "));
            if !same(st.dash_offset, 0.0) {
                let _ = write!(a, r#" stroke-dashoffset="{}""#, num(st.dash_offset));
            }
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

/// Equal within [`EPS`].
fn same(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}

fn fill_rule(s: &Shape) -> FillRule {
    match &s.geometry {
        Geometry::Path(p) => p.fill_rule,
        _ => FillRule::NonZero,
    }
}

/// `transform` and `opacity` attributes of a node.
fn common_attrs(node: &Node) -> String {
    let mut a = transform_attr(node.transform);
    if node.opacity < 1.0 {
        let _ = write!(a, r#" opacity="{}""#, num(node.opacity));
    }
    a
}

fn transform_attr(t: Transform) -> String {
    let [linear @ .., dx, dy] = t.0;
    if t == Transform::IDENTITY {
        String::new()
    } else if linear
        .iter()
        .zip([1.0, 0.0, 0.0, 1.0])
        .all(|(v, want)| same(*v, want))
    {
        format!(r#" transform="translate({} {})""#, num(dx), num(dy))
    } else {
        let m: Vec<String> = t.0.iter().map(|v| num(*v)).collect();
        format!(r#" transform="matrix({})""#, m.join(" "))
    }
}

/// The element name and geometry attributes of a shape.
fn shape_element(s: &Shape) -> String {
    match &s.geometry {
        Geometry::Rect { rect, radius }
            if same(radius.tl, radius.tr)
                && same(radius.tl, radius.br)
                && same(radius.tl, radius.bl) =>
        {
            let mut e = format!(
                r#"rect x="{}" y="{}" width="{}" height="{}""#,
                num(rect.x),
                num(rect.y),
                num(rect.width),
                num(rect.height)
            );
            if radius.tl > 0.0 {
                let _ = write!(e, r#" rx="{}""#, num(radius.tl));
            }
            e
        }
        Geometry::Ellipse { center, rx, ry } if same(*rx, *ry) => format!(
            r#"circle cx="{}" cy="{}" r="{}""#,
            num(center.x),
            num(center.y),
            num(*rx)
        ),
        Geometry::Ellipse { center, rx, ry } => format!(
            r#"ellipse cx="{}" cy="{}" rx="{}" ry="{}""#,
            num(center.x),
            num(center.y),
            num(*rx),
            num(*ry)
        ),
        Geometry::Line { from, to } => format!(
            r#"line x1="{}" y1="{}" x2="{}" y2="{}""#,
            num(from.x),
            num(from.y),
            num(to.x),
            num(to.y)
        ),
        g => format!(r#"path d="{}""#, path_data(&to_bezpath(g))),
    }
}

/// Absolute path data with `M L Q C Z`.
fn path_data(path: &BezPath) -> String {
    let mut d = String::new();
    let pt = |d: &mut String, p: kurbo::Point| {
        let _ = write!(d, "{} {}", num(p.x), num(p.y));
    };
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                d.push('M');
                pt(&mut d, p);
            }
            PathEl::LineTo(p) => {
                d.push('L');
                pt(&mut d, p);
            }
            PathEl::QuadTo(a, p) => {
                d.push('Q');
                pt(&mut d, a);
                d.push(' ');
                pt(&mut d, p);
            }
            PathEl::CurveTo(a, b, p) => {
                d.push('C');
                pt(&mut d, a);
                d.push(' ');
                pt(&mut d, b);
                d.push(' ');
                pt(&mut d, p);
            }
            PathEl::ClosePath => d.push('Z'),
        }
    }
    d
}

/// `#rgb` when lossless, else `#rrggbb` (alpha goes in `*-opacity`).
fn color(c: vextor_core::Rgba) -> String {
    let short = |v: u8| v >> 4 == v & 0xf;
    if short(c.r) && short(c.g) && short(c.b) {
        format!("#{:x}{:x}{:x}", c.r & 0xf, c.g & 0xf, c.b & 0xf)
    } else {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    }
}

/// A number with [`PRECISION`] decimals, trailing zeros and the leading
/// `0` of `0.x` stripped, `-0` as `0`.
fn num(v: f64) -> String {
    let mut s = format!("{v:.PRECISION$}");
    if s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    if s == "-0" {
        return "0".to_owned();
    }
    if let Some(rest) = s.strip_prefix("0.") {
        return format!(".{rest}");
    }
    if let Some(rest) = s.strip_prefix("-0.") {
        return format!("-.{rest}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_short() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(0.5), ".5");
        assert_eq!(num(-0.25), "-.25");
        assert_eq!(num(-0.001), "0");
        assert_eq!(num(12.345_6), "12.35");
        assert_eq!(num(100.0), "100");
    }

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

    #[test]
    fn colours_use_the_short_form_when_lossless() {
        assert_eq!(color(vextor_core::Rgba::rgb(255, 0, 0)), "#f00");
        assert_eq!(color(vextor_core::Rgba::rgb(0x12, 0x34, 0x56)), "#123456");
    }
}
