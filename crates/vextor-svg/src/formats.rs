//! The copy-as targets (`svg-output`): a standalone SVG file, inline HTML,
//! a JSX component and a CSS data URI, all from the same tree.

use std::fmt::Write as _;

use crate::Format;
use crate::element::{Element, Markup, Spans, write};

const XMLNS: &str = "http://www.w3.org/2000/svg";

/// `svg` written as `format`; `name` labels it (`aria-label`, the JSX
/// component). Returns the code and the spans of drawn nodes in it.
pub(crate) fn finish(
    mut svg: Element,
    format: Format,
    name: &str,
    minify: bool,
) -> (String, Spans) {
    let mut out = String::new();
    let mut spans = Vec::new();
    let markup = |jsx, depth| Markup { jsx, minify, depth };
    match format {
        Format::Svg => {
            svg.attrs.insert(0, ("xmlns", XMLNS.to_owned()));
            write(&svg, markup(false, 0), &mut out, &mut spans);
        }
        Format::Html => {
            svg.set("role", "img");
            let label = name.trim();
            if !label.is_empty() {
                svg.set("aria-label", label);
            }
            write(&svg, markup(false, 0), &mut out, &mut spans);
        }
        Format::Jsx => {
            let _ = write!(
                out,
                "export function {}(props) {{\n  return (\n",
                component_name(name)
            );
            if minify {
                out.push_str("    ");
            }
            write(&svg, markup(true, 2), &mut out, &mut spans);
            if minify {
                out.push('\n');
            }
            out.push_str("  );\n}\n");
        }
        Format::CssDataUri => {
            svg.attrs.insert(0, ("xmlns", XMLNS.to_owned()));
            let mut file = String::new();
            write(&svg, markup(false, 0), &mut file, &mut Vec::new());
            out = format!("url(\"data:image/svg+xml,{}\")", percent_encode(&file));
            if !minify {
                out.push('\n');
            }
            spans.clear();
        }
    }
    (out, spans)
}

/// `name` as a React component name: its ASCII words capitalised and
/// joined ("button-primary" → `ButtonPrimary`); `Svg` in front of a
/// leading digit; `Icon` when nothing is left.
pub(crate) fn component_name(name: &str) -> String {
    let mut out = String::new();
    for word in name.split(|c: char| !c.is_ascii_alphanumeric()) {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            out.extend(chars);
        }
    }
    if out.is_empty() {
        return "Icon".to_owned();
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert_str(0, "Svg");
    }
    out
}

/// Percent-encodes everything but letters, digits and the characters a
/// data URI inside `url("…")` may hold as they are.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 4);
    for b in s.bytes() {
        let keep = b.is_ascii_alphanumeric() || b"-_.~!*'()/:;=,+@?&$".contains(&b);
        if keep {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_names_are_pascal_case_identifiers() {
        assert_eq!(component_name("button-primary"), "ButtonPrimary");
        assert_eq!(component_name("icon 24"), "Icon24");
        assert_eq!(component_name("24 icon"), "Svg24Icon");
        assert_eq!(component_name("<script>"), "Script");
        assert_eq!(component_name("日本"), "Icon");
    }

    #[test]
    fn data_uris_encode_markup_characters() {
        assert_eq!(
            percent_encode(r##"<svg fill="#f00"> é"##),
            "%3Csvg%20fill=%22%23f00%22%3E%20%C3%A9"
        );
    }
}
