//! Text on the canvas (spec 05c): drawn from its layout outlines with the
//! shape paint model, frame behind. Fonts are the bundled OFL test fonts.

use std::sync::Arc;

use resvg::{tiny_skia, usvg};
use vextor_core::{
    Command, CornerRadii, Document, Node, NodeId, NodeKind, Paint, Rgba, Stroke, Style, Text,
    TextAlign, TextFrame, Transform, Vec2, View,
};
use vextor_geom::{FontDb, Fonts, text_outlines};
use vextor_render::{Pixmap, Viewport, render};

const REGULAR: &[u8] =
    include_bytes!("../../vextor-geom/tests/fixtures/fonts/NotoSans-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../vextor-geom/tests/fixtures/fonts/NotoSans-Bold.ttf");

const ZOOM: f64 = 2.0;
const SIZE: (u32, u32) = (400, 240);

fn fonts() -> Fonts {
    Fonts::new(Arc::new(FontDb::from_data([
        REGULAR.to_vec(),
        BOLD.to_vec(),
    ])))
}

fn viewport() -> Viewport {
    Viewport {
        view: View {
            zoom: ZOOM,
            pan: Vec2::new(0.0, 0.0),
        },
        width: SIZE.0,
        height: SIZE.1,
        pixels_per_point: 1.0,
    }
}

/// A document holding `nodes` in its layer.
fn doc(nodes: Vec<Node>) -> Document {
    let mut doc = Document::new("t");
    let layer = doc.layers[0];
    doc.apply(Command::Insert {
        parent: Some(layer),
        index: 0,
        nodes,
    })
    .expect("inserts");
    doc
}

fn at(id: NodeId, kind: NodeKind, x: f64, y: f64) -> Node {
    let mut n = Node::new(id, kind);
    n.transform = Transform([1.0, 0.0, 0.0, 1.0, x, y]);
    n
}

/// "Buy / now", bold, centred, white on a rounded blue frame with a border.
fn label() -> Text {
    Text {
        size: 32.0,
        align: TextAlign::Centre,
        style: Style {
            fill: Paint::Solid(Rgba::rgb(255, 255, 255)),
            stroke: None,
        },
        frame: Some(TextFrame {
            padding: [8.0, 16.0, 8.0, 16.0],
            radius: CornerRadii {
                tl: 10.0,
                tr: 10.0,
                br: 10.0,
                bl: 10.0,
            },
            fill: Paint::Solid(Rgba::rgb(30, 144, 255)),
            stroke: Some(Stroke {
                paint: Paint::Solid(Rgba::rgb(11, 83, 148)),
                width: 3.0,
                ..Stroke::default()
            }),
        }),
        font: vextor_core::FontSpec {
            family: "Noto Sans".to_owned(),
            weight: 700,
            italic: false,
        },
        ..Text::new("Buy\nnow", "Noto Sans")
    }
}

/// Share of pixels where any channel differs by more than `tolerance`.
fn differing(a: &[u8], b: &[u8], tolerance: u8) -> f64 {
    let (a, b) = (a.as_chunks::<4>().0, b.as_chunks::<4>().0);
    let n = a
        .iter()
        .zip(b)
        .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > tolerance))
        .count();
    #[allow(clippy::cast_precision_loss)] // pixel counts are small
    let share = n as f64 / a.len() as f64;
    share
}

/// RGBA of the pixel at `(x, y)`.
fn pixel(pixmap: &Pixmap, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * pixmap.width() + x) * 4) as usize;
    let data = pixmap.data();
    [data[at], data[at + 1], data[at + 2], data[at + 3]]
}

fn ink(p: &Pixmap) -> usize {
    p.data()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[3] > 0)
        .count()
}

#[test]
fn text_draws_like_the_shapes_of_its_outlines_and_frame() {
    let fonts = fonts();
    let text = label();
    let d = doc(vec![at(
        NodeId(100),
        NodeKind::Text(text.clone()),
        20.0,
        20.0,
    )]);
    let live = render(&d, &fonts, &viewport()).expect("renders");

    // What Convert to Outlines makes: a rounded rect for the frame, then
    // one compound path holding every glyph.
    let layout = fonts.layout(NodeId(100), &text);
    let (rect, glyphs) = text_outlines(&text, &layout);
    let (rect, glyphs) = (rect.expect("framed"), glyphs.expect("inked"));
    let twin = doc(vec![
        at(NodeId(101), NodeKind::Shape(rect), 20.0, 20.0),
        at(NodeId(102), NodeKind::Shape(glyphs), 20.0, 20.0),
    ]);
    let shapes = render(&twin, &fonts, &viewport()).expect("renders");

    let share = differing(live.data(), shapes.data(), 0);
    assert!(share <= 0.0, "{:.4} % of pixels differ", share * 100.0);
    assert!(ink(&live) > 1_000, "something was drawn");
}

#[test]
fn frame_is_behind_the_glyphs() {
    let fonts = fonts();
    let text = label();
    let d = doc(vec![at(
        NodeId(100),
        NodeKind::Text(text.clone()),
        20.0,
        20.0,
    )]);
    let p = render(&d, &fonts, &viewport()).expect("renders");
    let layout = fonts.layout(NodeId(100), &text);
    // Inside the padding, left of the first line: frame blue.
    let pad = |x: f64, y: f64| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (x, y) = (((20.0 + x) * ZOOM) as u32, ((20.0 + y) * ZOOM) as u32);
        pixel(&p, x, y)
    };
    let b = layout.bounds;
    assert_eq!(pad(b.x0 - 8.0, b.center().y), [30, 144, 255, 255]);
    // The middle of the "B" stem (bold, ~0.1 em wide from x ≈ 0.09 em):
    // white glyph over the frame.
    let first = &layout.lines[0];
    let stem_x = first.rect.x0 + 0.12 * 32.0;
    assert_eq!(pad(stem_x, first.baseline - 10.0), [255, 255, 255, 255]);
}

#[test]
fn glyph_stroke_and_hidden_text() {
    let fonts = fonts();
    let mut text = Text {
        size: 40.0,
        style: Style {
            fill: Paint::None,
            stroke: Some(Stroke {
                width: 2.0,
                ..Stroke::default()
            }),
        },
        ..Text::new("O", "Noto Sans")
    };
    let mut node = at(NodeId(100), NodeKind::Text(text.clone()), 10.0, 10.0);
    let outlined = render(&doc(vec![node.clone()]), &fonts, &viewport()).expect("renders");
    text.style.fill = Paint::Solid(Rgba::BLACK);
    let filled = render(
        &doc(vec![at(NodeId(100), NodeKind::Text(text), 10.0, 10.0)]),
        &fonts,
        &viewport(),
    )
    .expect("renders");
    assert!(ink(&outlined) > 0);
    assert!(
        ink(&filled) > ink(&outlined),
        "an outline inks less than a fill"
    );

    node.visible = false;
    let hidden = render(&doc(vec![node]), &fonts, &viewport()).expect("renders");
    assert_eq!(ink(&hidden), 0);
}

#[test]
fn without_fonts_only_the_frame_is_drawn() {
    let d = doc(vec![at(NodeId(100), NodeKind::Text(label()), 20.0, 20.0)]);
    let p = render(&d, &Fonts::default(), &viewport()).expect("renders");
    assert!(ink(&p) > 0, "the frame is drawn");
    let white = p
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| **px == [255, 255, 255, 255])
        .count();
    assert_eq!(white, 0, "no glyphs without fonts");
}

/// Our layout and outlines against resvg's own text engine, same font.
#[test]
fn a_line_of_text_matches_resvg() {
    let fonts = fonts();
    let text = Text {
        size: 28.0,
        ..Text::new("Hamburgefonstiv 0123", "Noto Sans")
    };
    let d = doc(vec![at(
        NodeId(100),
        NodeKind::Text(text.clone()),
        10.0,
        20.0,
    )]);
    let ours = render(&d, &fonts, &viewport()).expect("renders");

    let baseline = 20.0 + fonts.layout(NodeId(100), &text).lines[0].baseline;
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="120"><text x="10" y="{baseline}" font-family="Noto Sans" font-size="28">Hamburgefonstiv 0123</text></svg>"#
    );
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_font_data(REGULAR.to_vec());
    let tree = usvg::Tree::from_str(&svg, &opt).expect("parses");
    let mut reference = tiny_skia::Pixmap::new(SIZE.0, SIZE.1).expect("size");
    #[allow(clippy::cast_possible_truncation)] // constant 2.0
    let scale = ZOOM as f32;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut reference.as_mut(),
    );

    assert!(ink(&ours) > 1_000);
    let share = differing(ours.data(), reference.data(), 16);
    assert!(share <= 0.002, "{:.3} % of pixels differ", share * 100.0);
}

/// The clipboard SVG of a framed, centred two-line label, rendered by
/// resvg with the same font, looks like the canvas: `<text>`, `<tspan>`
/// baselines and `text-anchor` land where Vextor draws the glyphs.
#[test]
fn clipboard_svg_text_renders_like_the_canvas() {
    let fonts = fonts();
    let d = doc(vec![at(NodeId(100), NodeKind::Text(label()), 20.0, 20.0)]);
    let payload = d.copy(&[NodeId(100)]).expect("copies");
    let svg = vextor_svg::clipboard_svg(&payload, &fonts).expect("visible");

    let b = vextor_geom::bounds(&d, &fonts, NodeId(100)).expect("visible");
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (w, h) = (
        (b.width() * ZOOM).ceil() as u32,
        (b.height() * ZOOM).ceil() as u32,
    );
    let vp = Viewport {
        view: View {
            zoom: ZOOM,
            pan: Vec2::new(-b.x0 * ZOOM, -b.y0 * ZOOM),
        },
        width: w,
        height: h,
        pixels_per_point: 1.0,
    };
    let ours = render(&d, &fonts, &vp).expect("renders");

    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_font_data(REGULAR.to_vec());
    opt.fontdb_mut().load_font_data(BOLD.to_vec());
    let tree = usvg::Tree::from_str(&svg, &opt).expect("parses");
    let mut reference = tiny_skia::Pixmap::new(w, h).expect("size");
    // The SVG's size is its viewBox (the label's bounds): same zoom.
    #[allow(clippy::cast_possible_truncation)] // constant 2.0
    let scale = ZOOM as f32;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut reference.as_mut(),
    );

    let share = differing(ours.data(), reference.data(), 16);
    assert!(
        share <= 0.002,
        "{:.3} % of pixels differ\n{svg}",
        share * 100.0
    );
}
