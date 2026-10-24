//! The element tree an export builds, and writing it out as SVG markup or
//! as JSX: indented or minified, with the byte range of every element that
//! draws a document node (the code panel highlights the selection).

use std::fmt::Write as _;
use std::ops::Range;

use vextor_core::NodeId;

use crate::escape::{attr_escape, css_escape, jsx_text_escape, template_escape, xml_escape};

/// One SVG element. Attribute values are raw; they are escaped when
/// written.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Element {
    pub name: &'static str,
    pub attrs: Vec<(&'static str, String)>,
    pub children: Vec<Element>,
    /// Character data: a `<tspan>`'s text, a `<style>` sheet.
    pub text: Option<String>,
    /// The document node this element draws.
    pub node: Option<NodeId>,
}

impl Element {
    pub(crate) fn new(name: &'static str) -> Self {
        Self {
            name,
            ..Self::default()
        }
    }

    /// Adds attribute `name`.
    pub(crate) fn set(&mut self, name: &'static str, value: impl Into<String>) {
        self.attrs.push((name, value.into()));
    }

    /// The value of attribute `name`.
    #[cfg(test)]
    pub(crate) fn get(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Byte ranges of the elements that draw document nodes, in output order.
pub type Spans = Vec<(NodeId, Range<usize>)>;

/// How a tree is written.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Markup {
    pub jsx: bool,
    pub minify: bool,
    /// Indentation of the root element, in levels.
    pub depth: usize,
}

/// Writes `root` and its subtree to `out`, recording spans.
pub(crate) fn write(root: &Element, m: Markup, out: &mut String, spans: &mut Spans) {
    let mut w = Out { m, buf: out, spans };
    w.element(root, m.depth, false);
}

struct Out<'a> {
    m: Markup,
    buf: &'a mut String,
    spans: &'a mut Spans,
}

impl Out<'_> {
    fn indent(&mut self, depth: usize) {
        if !self.m.minify {
            for _ in 0..depth {
                self.buf.push_str("  ");
            }
        }
    }

    fn newline(&mut self) {
        if !self.m.minify {
            self.buf.push('\n');
        }
    }

    /// One element; `inline` keeps it on its parent's line (the lines of a
    /// `<text>`, where whitespace is content).
    fn element(&mut self, el: &Element, depth: usize, inline: bool) {
        if !inline {
            self.indent(depth);
        }
        let start = self.buf.len();
        self.buf.push('<');
        self.buf.push_str(el.name);
        for (name, value) in &el.attrs {
            self.buf.push(' ');
            if self.m.jsx {
                self.buf.push_str(&jsx_name(name));
            } else {
                self.buf.push_str(name);
            }
            self.buf.push_str("=\"");
            self.buf.push_str(&attr_escape(value));
            self.buf.push('"');
        }
        if self.m.jsx && depth == self.m.depth {
            self.buf.push_str(" {...props}");
        }
        if el.children.is_empty() && el.text.is_none() {
            self.buf.push_str("/>");
        } else {
            self.buf.push('>');
            if let Some(text) = &el.text {
                self.text(el.name, text);
            }
            let inline_children = inline || el.name == "text";
            if !el.children.is_empty() && !inline_children {
                self.newline();
            }
            for child in &el.children {
                self.element(child, depth + 1, inline_children);
            }
            if !el.children.is_empty() && !inline_children {
                self.indent(depth);
            }
            self.buf.push_str("</");
            self.buf.push_str(el.name);
            self.buf.push('>');
        }
        if let Some(node) = el.node {
            self.spans.push((node, start..self.buf.len()));
        }
        if !inline {
            self.newline();
        }
    }

    fn text(&mut self, element: &str, text: &str) {
        let escaped = match (element, self.m.jsx) {
            ("style", false) => css_escape(text),
            ("style", true) => format!("{{`{}`}}", template_escape(text)),
            (_, false) => xml_escape(text),
            (_, true) => jsx_text_escape(text),
        };
        self.buf.push_str(&escaped);
    }
}

/// An SVG attribute name as React spells it: `stroke-width` →
/// `strokeWidth`, `class` → `className`, `xml:space` → `xmlSpace`;
/// `aria-*` stays as it is.
pub(crate) fn jsx_name(name: &str) -> String {
    match name {
        "class" => return "className".to_owned(),
        n if n.starts_with("aria-") => return n.to_owned(),
        _ => {}
    }
    let mut out = String::with_capacity(name.len());
    let mut upper = false;
    for c in name.chars() {
        if c == '-' || c == ':' {
            upper = true;
        } else if upper {
            out.push(c.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Presentation attributes that move into the `<style>` sheet when the
/// document asks for classes.
const STYLE_ATTRS: [&str; 16] = [
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-opacity",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "letter-spacing",
];

/// Moves the presentation attributes of every drawing element in `root`
/// into classes (`s1`, `s2`… with `prefix`), one per distinct set, in
/// order of first use. Returns the style sheet, or `None` when nothing
/// had any.
pub(crate) fn extract_classes(root: &mut Element, prefix: &str) -> Option<String> {
    let mut rules: Vec<(String, String)> = Vec::new();
    move_into_classes(root, prefix, &mut rules);
    if rules.is_empty() {
        return None;
    }
    let mut css = String::new();
    for (decls, class) in &rules {
        let _ = write!(css, ".{class}{{{decls}}}");
    }
    Some(css)
}

fn move_into_classes(el: &mut Element, prefix: &str, rules: &mut Vec<(String, String)>) {
    // Gradient stops and clip shapes keep their attributes.
    if el.name != "stop" && el.name != "clipPath" {
        let decls: Vec<String> = el
            .attrs
            .iter()
            .filter(|(n, _)| STYLE_ATTRS.contains(n))
            .map(|(n, v)| format!("{n}:{}", css_value(n, v)))
            .collect();
        if !decls.is_empty() {
            let decls = decls.join(";");
            let class = if let Some((_, c)) = rules.iter().find(|(d, _)| *d == decls) {
                c.clone()
            } else {
                let c = format!("{prefix}s{}", rules.len() + 1);
                rules.push((decls, c.clone()));
                c
            };
            el.attrs.retain(|(n, _)| !STYLE_ATTRS.contains(n));
            // After the id, if any, so the element still reads id-first.
            let at = usize::from(el.attrs.first().is_some_and(|(n, _)| *n == "id"));
            el.attrs.insert(at, ("class", class));
        }
    }
    for child in &mut el.children {
        move_into_classes(child, prefix, rules);
    }
}

/// A presentation attribute's value as CSS: lengths CSS does not take
/// bare get `px`.
fn css_value(name: &str, value: &str) -> String {
    let bare = value.parse::<f64>().is_ok();
    if bare && matches!(name, "font-size" | "letter-spacing") && value != "0" {
        format!("{value}px")
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Element {
        let mut svg = Element::new("svg");
        svg.set("viewBox", "0 0 4 4");
        let mut rect = Element::new("rect");
        rect.set("width", "4");
        rect.set("fill", "#f00");
        rect.node = Some(NodeId(7));
        svg.children.push(rect);
        let mut text = Element::new("text");
        let mut line = Element::new("tspan");
        line.text = Some("a<b".to_owned());
        text.children.push(line);
        svg.children.push(text);
        svg
    }

    fn written(el: &Element, jsx: bool, minify: bool) -> (String, Spans) {
        let mut out = String::new();
        let mut spans = Vec::new();
        let m = Markup {
            jsx,
            minify,
            depth: 0,
        };
        write(el, m, &mut out, &mut spans);
        (out, spans)
    }

    #[test]
    fn markup_is_indented_or_minified() {
        let (pretty, spans) = written(&tree(), false, false);
        assert_eq!(
            pretty,
            "<svg viewBox=\"0 0 4 4\">\n  <rect width=\"4\" fill=\"#f00\"/>\n  <text><tspan>a&lt;b</tspan></text>\n</svg>\n"
        );
        assert_eq!(
            &pretty[spans[0].1.clone()],
            r##"<rect width="4" fill="#f00"/>"##
        );
        let (min, _) = written(&tree(), false, true);
        assert_eq!(
            min,
            r##"<svg viewBox="0 0 4 4"><rect width="4" fill="#f00"/><text><tspan>a&lt;b</tspan></text></svg>"##
        );
    }

    #[test]
    fn jsx_camel_cases_and_spreads_props_on_the_root() {
        let mut t = tree();
        t.children[0].set("stroke-width", "2");
        let (jsx, _) = written(&t, true, true);
        assert!(
            jsx.starts_with(r#"<svg viewBox="0 0 4 4" {...props}>"#),
            "{jsx}"
        );
        assert!(jsx.contains(r#"strokeWidth="2""#));
        assert_eq!(jsx_name("xml:space"), "xmlSpace");
        assert_eq!(jsx_name("aria-label"), "aria-label");
        assert_eq!(jsx_name("class"), "className");
    }

    #[test]
    fn shared_styles_become_one_class() {
        let mut t = tree();
        let mut other = t.children[0].clone();
        other.attrs.insert(0, ("id", "b".to_owned()));
        t.children.push(other);
        t.children[1].set("font-size", "12");
        let css = extract_classes(&mut t, "").expect("has styles");
        assert_eq!(css, ".s1{fill:#f00}.s2{font-size:12px}");
        assert_eq!(t.children[0].get("class"), Some("s1"));
        assert_eq!(t.children[2].attrs[0], ("id", "b".to_owned()));
        assert_eq!(t.children[2].get("class"), Some("s1"));
        assert_eq!(t.children[0].get("fill"), None);
    }
}
