//! Escaping every document string that reaches the output (`security-check`
//! §3): text content, font family names, names in `aria-label`, ids.

/// Text with `& < > " '` as entities and characters XML 1.0 does not allow
/// left out.
pub(crate) fn xml_escape(s: &str) -> String {
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
pub(crate) fn attr_escape(s: &str) -> String {
    xml_escape(s).replace("&apos;", "'")
}

/// Text inside JSX: as XML, plus `{ }` (which would open an expression)
/// as numeric entities.
pub(crate) fn jsx_text_escape(s: &str) -> String {
    xml_escape(s).replace('{', "&#123;").replace('}', "&#125;")
}

/// A style sheet as element content: only `& < >` as entities, so CSS
/// quotes stay readable in SVG files and inline HTML alike.
pub(crate) fn css_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A style sheet inside a JSX template literal: `` ` ``, `\` and `${`
/// escaped.
pub(crate) fn template_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${")
}

/// `'Family', sans-serif`: the family quoted for CSS, with the characters
/// that could end the quote or the declaration removed, and a generic
/// fallback for pages without the font.
pub(crate) fn font_family(family: &str) -> String {
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
        assert_eq!(attr_escape(r#"a"b'c"#), "a&quot;b'c");
    }

    #[test]
    fn jsx_text_cannot_open_an_expression() {
        assert_eq!(
            jsx_text_escape("{alert(1)} <b>"),
            "&#123;alert(1)&#125; &lt;b&gt;"
        );
        assert_eq!(template_escape("a`${b}\\"), "a\\`\\${b}\\\\");
        assert_eq!(
            css_escape(".a{x:'A & B'}</style>"),
            ".a{x:'A &amp; B'}&lt;/style&gt;"
        );
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
}
