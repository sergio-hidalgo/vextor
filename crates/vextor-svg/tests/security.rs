//! `security-check` §3: document strings never escape their place in the
//! output, in any format, and nothing executable is ever written.

mod common;

use common::*;
use vextor_core::{Node, NodeKind, Paint, Style, Text};
use vextor_svg::Format;

const HOSTILE: &str = "\"><script>alert(1)</script><g onload=\"x\" {alert(2)} ${y} `z` &amp;\u{0}";
/// The same within a name's limits (64 characters, no control characters).
const NAME: &str = "\"><script>alert(1)</script><g onload=\"x\" {alert(2)}";

/// Every place a document string reaches the output: node and gradient
/// names (ids), the artboard name (`aria-label`, JSX component), the text
/// content and its font family.
fn hostile() -> Doc {
    let mut d = Doc::new(0.0, 0.0, 200.0, 60.0);
    let mut artboards = d.doc.artboards.clone();
    HOSTILE.clone_into(&mut artboards[0].name);
    d.doc
        .apply(vextor_core::Command::SetArtboards(artboards))
        .expect("valid");
    let shape = d.shape(rect(0.0, 0.0, 10.0, 10.0, 0.0), Style::default());
    d.edit(shape, |n| n.name = Some(NAME.to_owned()));
    let id = d.id();
    let text = Text {
        style: Style {
            fill: Paint::Solid(RED),
            stroke: None,
        },
        ..Text::new(HOSTILE, NAME)
    };
    let mut node = Node::new(id, NodeKind::Text(text));
    node.transform = moved(10.0, 20.0);
    d.add(node, Vec::new());
    d
}

fn assert_inert(code: &str) {
    for bad in [
        "<script",
        "onload=\"",
        "<g onload",
        "javascript:",
        "<foreignObject",
        "\u{0}",
    ] {
        assert!(!code.contains(bad), "{bad:?} in {code}");
    }
}

#[test]
fn hostile_strings_are_escaped_in_every_format() {
    let d = hostile();
    for format in [Format::Svg, Format::Html] {
        let code = d.export(format).code;
        assert_inert(&code);
        assert!(
            code.contains("&quot;&gt;&lt;script&gt;alert(1)&lt;/script&gt;"),
            "{format:?}: {code}"
        );
    }
    let jsx = d.export(Format::Jsx).code;
    assert_inert(&jsx);
    // Braces in JSX text would run code.
    assert!(!jsx.contains("{alert(2)}"), "{jsx}");
    assert!(jsx.contains("&#123;alert(2)&#125;"), "{jsx}");
    assert!(
        jsx.starts_with("export function ScriptAlert1ScriptGOnloadXAlert2YZAmp(props) {"),
        "{jsx}"
    );
    let css = d.export(Format::CssDataUri).code;
    assert!(
        !css[5..css.len() - 2].contains('"'),
        "the url() string stays closed: {css}"
    );
    assert!(!css.contains('<'), "{css}");
}

#[test]
fn ids_and_classes_are_slugs() {
    let mut d = hostile();
    d.options(|o| o.styling = vextor_core::Styling::StyleElement);
    let svg = d.svg();
    assert!(
        svg.contains(r#"id="script-alert-1-script-g-onload-x-alert-2""#),
        "{svg}"
    );
    assert!(svg.contains("<style>"), "{svg}");
    let style = &svg[svg.find("<style>").expect("style")..svg.find("</style>").expect("end")];
    assert!(!style.contains("<script"), "{style}");
}

#[test]
fn font_families_cannot_leave_their_css_string() {
    let d = hostile();
    let svg = d.svg();
    let family = svg
        .split("font-family=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("a family");
    assert!(family.starts_with('\''), "{family}");
    assert!(family.ends_with("', sans-serif"), "{family}");
    assert_eq!(family.matches('\'').count(), 2, "{family}");
    assert!(!family.contains(';') && !family.contains('<'), "{family}");
}

#[test]
fn aria_labels_are_escaped() {
    let html = hostile().export(Format::Html).code;
    let label = html
        .split("aria-label=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("a label");
    assert!(label.starts_with("&quot;&gt;&lt;script&gt;"), "{label}");
}
