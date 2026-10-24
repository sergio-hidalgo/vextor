//! Bounds an SVG's XML before `usvg` sees it (`security-check` §2).
//!
//! `usvg` expands every `<use>` into a copy of what it references and
//! recurses through the tree, so a few nested `<use>` (each pointing at a
//! group of `<use>`) can ask for billions of objects, and deep nesting can
//! overflow the stack. Here the tree is measured as `usvg` would expand it
//! (elements counted, depth followed through `<use>`, reference loops cut)
//! and refused past the limits.

use std::collections::HashMap;

use usvg::roxmltree::{self, Document, Node, NodeId};

use super::ImportError;

/// XML nodes (elements, text, comments) parsed at most; bounds memory.
const XML_NODES_LIMIT: u32 = 400_000;

/// Elements, `<use>` expanded, at most: four for each object the model
/// takes (definitions, gradient stops and wrappers count too).
const EXPANDED_LIMIT: u64 = 4 * vextor_core::MAX_NODES as u64;

/// Numbers in geometry, larger than this, are refused: an arc's radius
/// sets how many curves usvg splits it into (10⁴⁸ asks for gigabytes),
/// and no web component is a billion px across.
const MAX_MAGNITUDE: f64 = 1e9;

/// Attributes that hold names or links, not numbers.
const NOT_NUMBERS: [&str; 6] = ["id", "class", "href", "font-family", "lang", "space"];

/// Nesting depth, `<use>` included, at most.
pub(super) const MAX_DEPTH: usize = 128;

const XLINK: &str = "http://www.w3.org/1999/xlink";

/// The parsed XML of `text`, once it is known to be bounded.
pub(super) fn parse(text: &str) -> Result<Document<'_>, ImportError> {
    let options = roxmltree::ParsingOptions {
        // Many exported SVGs carry a DOCTYPE. Internal entities are bounded
        // by roxmltree (depth and count); external ones are never resolved.
        allow_dtd: true,
        nodes_limit: XML_NODES_LIMIT,
        entity_resolver: None,
    };
    let doc = Document::parse_with_options(text, options).map_err(|e| match e {
        roxmltree::Error::NodesLimitReached => ImportError::TooManyObjects,
        e => ImportError::Invalid(e.to_string()),
    })?;
    if xml_depth(&doc) > MAX_DEPTH {
        return Err(ImportError::TooDeep);
    }
    if has_huge_number(&doc) {
        return Err(ImportError::TooFar);
    }
    let ids: HashMap<&str, Node<'_, '_>> = doc
        .descendants()
        .filter_map(|n| n.attribute("id").map(|id| (id, n)))
        .collect();
    let mut m = Measure {
        ids,
        done: HashMap::new(),
        open: Vec::new(),
    };
    let (count, _) = m.element(doc.root_element(), 0)?;
    if count > EXPANDED_LIMIT {
        return Err(ImportError::TooManyObjects);
    }
    Ok(doc)
}

/// The deepest element nesting, measured without recursion: in document
/// order a parent comes before its children.
fn xml_depth(doc: &Document<'_>) -> usize {
    let mut depth: HashMap<NodeId, usize> = HashMap::new();
    let mut max = 0;
    for n in doc.descendants().filter(Node::is_element) {
        let d = n
            .parent()
            .and_then(|p| depth.get(&p.id()))
            .copied()
            .unwrap_or(0)
            + 1;
        max = max.max(d);
        depth.insert(n.id(), d);
    }
    max
}

/// True when an attribute (or a `<style>` sheet) has a number past
/// [`MAX_MAGNITUDE`], or one that is not finite.
fn has_huge_number(doc: &Document<'_>) -> bool {
    doc.descendants().any(|n| {
        let sheet = (n.tag_name().name() == "style")
            .then(|| n.text())
            .flatten()
            .is_some_and(huge);
        sheet
            || n.attributes()
                .filter(|a| !NOT_NUMBERS.contains(&a.name()) && !a.name().starts_with("xmlns"))
                .any(|a| huge(a.value()))
    })
}

/// True when `text` holds a number (SVG / CSS syntax) past the limit.
fn huge(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // `url(…)` is a reference or a data URI (base64 has digit runs).
        if b[i..].starts_with(b"url(") {
            i += b[i..]
                .iter()
                .position(|&c| c == b')')
                .unwrap_or(b.len() - i);
            continue;
        }
        // `#1e90ff` is a colour.
        if b[i] == b'#' {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'-' || b[i] == b'_') {
                i += 1;
            }
            continue;
        }
        if !(b[i].is_ascii_digit() || b[i] == b'.') {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
            i += 1;
        }
        if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
            let mut j = i + 1;
            if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                j += 1;
            }
            if j < b.len() && b[j].is_ascii_digit() {
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                i = j;
            }
        }
        // A run like `1.5.5` is two numbers to SVG; take the longest that
        // parses, which is never smaller than either.
        let token = &text[start..i];
        let value = token
            .parse::<f64>()
            .or_else(|_| token.split('.').next().unwrap_or("0").parse::<f64>())
            .unwrap_or(0.0);
        if !value.is_finite() || value > MAX_MAGNITUDE {
            return true;
        }
    }
    false
}

struct Measure<'a, 'input> {
    ids: HashMap<&'a str, Node<'a, 'input>>,
    /// Expanded (element count, height) of elements measured so far.
    done: HashMap<NodeId, (u64, usize)>,
    /// Elements being measured: a reference back to one is a loop, which
    /// `usvg` skips, so it counts nothing.
    open: Vec<NodeId>,
}

impl<'a, 'input> Measure<'a, 'input> {
    /// `node`'s expanded element count and height, reached at `depth`.
    fn element(
        &mut self,
        node: Node<'a, 'input>,
        depth: usize,
    ) -> Result<(u64, usize), ImportError> {
        if let Some(&(count, height)) = self.done.get(&node.id()) {
            return check(depth, count, height);
        }
        if self.open.contains(&node.id()) {
            return Ok((0, 0));
        }
        if depth > MAX_DEPTH {
            return Err(ImportError::TooDeep);
        }
        self.open.push(node.id());
        let (mut count, mut height) = (1u64, 1usize);
        let mut add = |m: &mut Self, child: Node<'a, 'input>| -> Result<(), ImportError> {
            let (c, h) = m.element(child, depth + 1)?;
            count = count.saturating_add(c);
            height = height.max(h + 1);
            if count > EXPANDED_LIMIT {
                return Err(ImportError::TooManyObjects);
            }
            Ok(())
        };
        for child in node.children().filter(Node::is_element) {
            add(self, child)?;
        }
        if node.tag_name().name() == "use"
            && let Some(target) = self.reference(node)
        {
            add(self, target)?;
        }
        self.open.pop();
        self.done.insert(node.id(), (count, height));
        check(depth, count, height)
    }

    /// The element a `<use>` points at (`href` or `xlink:href`, `#id`).
    fn reference(&self, node: Node<'a, 'input>) -> Option<Node<'a, 'input>> {
        let href = node
            .attribute((XLINK, "href"))
            .or_else(|| node.attribute("href"))?;
        self.ids.get(href.trim().strip_prefix('#')?).copied()
    }
}

fn check(depth: usize, count: u64, height: usize) -> Result<(u64, usize), ImportError> {
    if depth + height > MAX_DEPTH + 1 {
        Err(ImportError::TooDeep)
    } else {
        Ok((count, height))
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    fn nested_uses(levels: usize, fan_out: usize) -> String {
        let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg"><defs>"#);
        s.push_str(r#"<rect id="l0" width="1" height="1"/>"#);
        for l in 1..=levels {
            let _ = write!(s, r#"<g id="l{l}">"#);
            for _ in 0..fan_out {
                let _ = write!(s, r##"<use href="#l{}"/>"##, l - 1);
            }
            s.push_str("</g>");
        }
        let _ = write!(s, r##"</defs><use href="#l{levels}"/></svg>"##);
        s
    }

    #[test]
    fn numbers_past_a_billion_are_found_in_any_spelling() {
        for text in [
            "M0 0a1e48 1 0 0 1 2 2",
            "1099999999999",
            "2E10",
            "scale(1e+12)",
            "5e999",
        ] {
            assert!(huge(text), "{text}");
        }
        for text in [
            "M0 0h24v24",
            "1e9",
            "-3.5e-7",
            ".5.5",
            "rgb(1 2 3)",
            "1e",
            "x1e3y",
        ] {
            assert!(!huge(text), "{text}");
        }
    }

    #[test]
    fn a_billion_laughs_of_use_is_refused_quickly() {
        let svg = nested_uses(9, 10);
        assert_eq!(parse(&svg).err(), Some(ImportError::TooManyObjects));
    }

    #[test]
    fn a_small_use_tree_passes() {
        let svg = nested_uses(2, 3);
        assert!(parse(&svg).is_ok());
    }

    #[test]
    fn a_use_loop_counts_once_and_passes() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg">
            <g id="a"><use href="#b"/></g><g id="b"><use href="#a"/></g>
            <use href="#self" id="self"/></svg>"##;
        assert!(parse(svg).is_ok());
    }

    #[test]
    fn deep_nesting_is_refused() {
        let deep = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg">{}{}</svg>"#,
            "<g>".repeat(MAX_DEPTH + 5),
            "</g>".repeat(MAX_DEPTH + 5)
        );
        assert_eq!(parse(&deep).err(), Some(ImportError::TooDeep));
        // A chain of `<use>` nests just as deep once expanded.
        let mut chain = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg">"#);
        for i in 0..MAX_DEPTH + 5 {
            let _ = write!(chain, r##"<g id="c{i}"><use href="#c{}"/></g>"##, i + 1);
        }
        chain.push_str("</svg>");
        assert_eq!(parse(&chain).err(), Some(ImportError::TooDeep));
    }
}
