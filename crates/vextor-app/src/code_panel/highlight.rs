//! Syntax colours for the code panel, and the selected objects' elements
//! picked out with a background: SVG/HTML markup, the JSX around it and
//! the CSS of a data URI or `<style>` read well enough with five colours.

use std::ops::Range;

use eframe::egui::{Color32, FontId, TextFormat, text::LayoutJob};

use crate::theme;

/// What a stretch of code is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    /// `<`, `>`, `/`, `=`, braces.
    Punct,
    /// Element names.
    Tag,
    /// Attribute names.
    Attr,
    /// Quoted values.
    Value,
    /// Text content and everything else.
    Text,
}

impl Token {
    fn color(self) -> Color32 {
        match self {
            Self::Punct => theme::GREY_500,
            Self::Tag => theme::YELLOW_SOFT,
            Self::Attr => theme::GREY_300,
            Self::Value => theme::YELLOW_LIGHT,
            Self::Text => theme::GREY_100,
        }
    }
}

/// `code` split into tokens, as byte ranges covering all of it in order.
pub fn tokens(code: &str) -> Vec<(Range<usize>, Token)> {
    let bytes = code.as_bytes();
    let mut out: Vec<(Range<usize>, Token)> = Vec::new();
    let mut push = |range: Range<usize>, token: Token| {
        if range.is_empty() {
            return;
        }
        match out.last_mut() {
            Some((last, t)) if *t == token && last.end == range.start => last.end = range.end,
            _ => out.push((range, token)),
        }
    };
    let mut i = 0;
    let mut in_tag = false;
    while i < bytes.len() {
        let b = bytes[i];
        if !in_tag {
            if b == b'<'
                && bytes
                    .get(i + 1)
                    .is_some_and(|n| n.is_ascii_alphabetic() || *n == b'/')
            {
                let slash = usize::from(bytes[i + 1] == b'/');
                push(i..i + 1 + slash, Token::Punct);
                let start = i + 1 + slash;
                let end = scan(bytes, start, |c| {
                    c.is_ascii_alphanumeric() || c == b':' || c == b'-'
                });
                push(start..end, Token::Tag);
                i = end;
                in_tag = true;
            } else {
                push(i..i + 1, Token::Text);
                i += 1;
            }
            continue;
        }
        match b {
            b'"' => {
                let end = scan(bytes, i + 1, |c| c != b'"').min(bytes.len() - 1) + 1;
                push(i..end, Token::Value);
                i = end;
            }
            b'>' => {
                push(i..i + 1, Token::Punct);
                in_tag = false;
                i += 1;
            }
            b'/' | b'=' | b'{' | b'}' | b'.' => {
                push(i..i + 1, Token::Punct);
                i += 1;
            }
            c if c.is_ascii_alphabetic() => {
                let end = scan(bytes, i, |c| {
                    c.is_ascii_alphanumeric() || c == b':' || c == b'-'
                });
                push(i..end, Token::Attr);
                i = end;
            }
            _ => {
                push(i..i + 1, Token::Text);
                i += 1;
            }
        }
    }
    out
}

/// The first index from `start` whose byte does not satisfy `keep`.
fn scan(bytes: &[u8], start: usize, keep: impl Fn(u8) -> bool) -> usize {
    bytes[start.min(bytes.len())..]
        .iter()
        .position(|&c| !keep(c))
        .map_or(bytes.len(), |p| start + p)
}

/// `code` coloured as `tokens`, with `marked` byte ranges on a highlight
/// background.
pub fn layout_job(code: &str, font: &FontId, marked: &[Range<usize>]) -> LayoutJob {
    let mut job = LayoutJob::default();
    let background = theme::LOGO_AMBER.gamma_multiply(0.35);
    for (range, token) in tokens(code) {
        // Split each token where a highlight starts or ends.
        let mut cuts: Vec<usize> = marked
            .iter()
            .flat_map(|m| [m.start, m.end])
            .filter(|&c| c > range.start && c < range.end)
            .collect();
        cuts.sort_unstable();
        cuts.dedup();
        let mut start = range.start;
        for end in cuts.into_iter().chain([range.end]) {
            let lit = marked.iter().any(|m| m.start <= start && end <= m.end);
            let format = TextFormat {
                font_id: font.clone(),
                color: token.color(),
                background: if lit {
                    background
                } else {
                    Color32::TRANSPARENT
                },
                ..TextFormat::default()
            };
            job.append(&code[start..end], 0.0, format);
            start = end;
        }
    }
    job.wrap.max_width = f32::INFINITY;
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(code: &str) -> Vec<(&str, Token)> {
        tokens(code)
            .into_iter()
            .map(|(r, t)| (&code[r], t))
            .collect()
    }

    #[test]
    fn markup_splits_into_tags_attributes_and_values() {
        assert_eq!(
            kinds(r#"<rect width="4"/>x</svg>"#),
            [
                ("<", Token::Punct),
                ("rect", Token::Tag),
                (" ", Token::Text),
                ("width", Token::Attr),
                ("=", Token::Punct),
                ("\"4\"", Token::Value),
                ("/>", Token::Punct),
                ("x", Token::Text),
                ("</", Token::Punct),
                ("svg", Token::Tag),
                (">", Token::Punct),
            ]
        );
    }

    #[test]
    fn text_with_angle_brackets_and_unclosed_quotes_never_panics() {
        for code in [
            "a < b",
            "<a b=\"",
            "<",
            "<a",
            "url(\"data:image/svg+xml,%3C\")",
        ] {
            let covered: usize = tokens(code).iter().map(|(r, _)| r.len()).sum();
            assert_eq!(covered, code.len(), "{code}");
        }
    }

    #[test]
    fn highlights_cut_tokens_at_their_edges() {
        let code = r#"<a b="1"/>"#;
        let marked = vec![Range { start: 3, end: 8 }];
        let job = layout_job(code, &FontId::monospace(12.0), &marked);
        let lit: String = job
            .sections
            .iter()
            .filter(|s| s.format.background != Color32::TRANSPARENT)
            .map(|s| &job.text[s.byte_range.start.0..s.byte_range.end.0])
            .collect();
        assert_eq!(lit, r#"b="1""#);
        assert_ne!(Token::Tag.color(), Token::Attr.color());
    }
}
