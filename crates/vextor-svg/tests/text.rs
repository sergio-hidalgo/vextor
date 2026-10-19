//! Text in the clipboard SVG (spec 05c): live `<text>`, escaped. Laid out
//! with the bundled OFL test font, never the machine's fonts.

use std::sync::Arc;

use vextor_core::{
    Command, CornerRadii, Document, FontSpec, Node, NodeKind, Paint, Rgba, Stroke, Style, Text,
    TextAlign, TextFrame, Transform,
};
use vextor_geom::{FontDb, Fonts};
use vextor_svg::clipboard_svg;

const BOLD: &[u8] = include_bytes!("../../vextor-geom/tests/fixtures/fonts/NotoSans-Bold.ttf");

fn fonts() -> Fonts {
    Fonts::new(Arc::new(FontDb::from_data([BOLD.to_vec()])))
}

/// The SVG of one text at (20, 10).
fn svg_of(text: Text) -> String {
    let mut doc = Document::new("t");
    let layer = doc.layers[0];
    let id = doc.alloc_node_id();
    let mut node = Node::new(id, NodeKind::Text(text));
    node.transform = Transform([1.0, 0.0, 0.0, 1.0, 20.0, 10.0]);
    doc.apply(Command::Insert {
        parent: Some(layer),
        index: 0,
        nodes: vec![node],
    })
    .expect("inserts");
    let payload = doc.copy(&[id]).expect("copies");
    clipboard_svg(&payload, &fonts()).expect("something visible")
}

/// "Buy / now", bold, centred, white on a blue frame with a border.
fn label() -> Text {
    Text {
        size: 18.0,
        align: TextAlign::Centre,
        font: FontSpec {
            family: "Noto Sans".to_owned(),
            weight: 700,
            italic: false,
        },
        style: Style {
            fill: Paint::Solid(Rgba::rgb(255, 255, 255)),
            stroke: None,
        },
        frame: Some(TextFrame {
            padding: [8.0, 16.0, 8.0, 16.0],
            radius: CornerRadii {
                tl: 6.0,
                tr: 6.0,
                br: 6.0,
                bl: 6.0,
            },
            fill: Paint::Solid(Rgba::rgb(30, 144, 255)),
            stroke: Some(Stroke {
                paint: Paint::Solid(Rgba::rgb(11, 83, 148)),
                ..Stroke::default()
            }),
        }),
        ..Text::new("Buy\nnow", "Noto Sans")
    }
}

#[test]
fn a_framed_two_line_centred_label() {
    assert_eq!(
        svg_of(label()),
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="4 2 70 59.2">
  <g transform="translate(20 10)">
    <rect x="-16" y="-8" width="70" height="59.2" rx="6" fill="#1e90ff" stroke="#0b5394"/>
    <text font-family="'Noto Sans', sans-serif" font-size="18" font-weight="700" text-anchor="middle" fill="#fff"><tspan x="19" y="17.78">Buy</tspan><tspan x="19" y="39.38">now</tspan></text>
  </g>
</svg>
"##
    );
}

#[test]
fn hostile_content_and_family_are_escaped() {
    let text = Text {
        size: 18.0,
        ..Text::new(
            "<script>alert(\"x\")</script> & 'q'\u{0}",
            r#"Evil"' onload="x"; }<b>"#,
        )
    };
    let svg = svg_of(text);
    assert!(!svg.contains("<script"), "{svg}");
    assert!(!svg.contains("onload=\""), "{svg}");
    assert!(
        svg.contains(
            "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt; &amp; &apos;q&apos;</tspan>"
        ),
        "{svg}"
    );
    assert!(
        svg.contains(r#"font-family="'Evil onload=x b', sans-serif""#),
        "{svg}"
    );
    assert!(!svg.contains('\u{0}'));
}

#[test]
fn plain_text_left_aligned_with_style_options() {
    let text = Text {
        size: 20.0,
        letter_spacing: 1.5,
        font: FontSpec {
            family: "Noto Sans".to_owned(),
            weight: 400,
            italic: true,
        },
        style: Style {
            fill: Paint::CurrentColor,
            stroke: None,
        },
        ..Text::new("A  B\n\nC", "Noto Sans")
    };
    let svg = svg_of(text);
    assert!(svg.starts_with("<svg"), "{svg}");
    let text_el = svg.lines().find(|l| l.contains("<text")).expect("a text");
    assert!(
        text_el.contains(r#"<text transform="translate(20 10)""#),
        "{text_el}"
    );
    assert!(text_el.contains(r#"font-style="italic""#));
    assert!(text_el.contains(r#"letter-spacing="1.5""#));
    assert!(text_el.contains(r#"fill="currentColor""#));
    assert!(
        text_el.contains(r#"xml:space="preserve""#),
        "double space kept"
    );
    assert!(!text_el.contains("font-weight"), "400 is the default");
    assert!(!text_el.contains("text-anchor"), "start is the default");
    assert_eq!(
        text_el.matches("<tspan").count(),
        2,
        "the empty line is skipped"
    );
    assert!(!svg.contains("<g"), "no frame, no group");
}
