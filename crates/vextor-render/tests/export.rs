//! Render comparison (spec 12): `resvg` drawing our exported SVG looks like
//! our canvas drawing the document, for every fixture and artboard. Text
//! is checked live and as outlines, with the bundled OFL test fonts.

use std::sync::Arc;

use resvg::{tiny_skia, usvg};
use vextor_core::{
    Command, Container, CornerRadii, Document, FillRule, Geometry, Node, NodeKind, Paint, Path,
    PathNode, Rect, Rgba, Shape, Stroke, Style, SubPath, Text, TextAlign, TextFrame, Transform,
    Vec2, View,
};
use vextor_geom::{FontDb, Fonts};
use vextor_render::{Viewport, fixtures, render};
use vextor_svg::{Format, Scope, export};

const REGULAR: &[u8] =
    include_bytes!("../../vextor-geom/tests/fixtures/fonts/NotoSans-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../vextor-geom/tests/fixtures/fonts/NotoSans-Bold.ttf");

/// A pixel differs when any channel is off by more than this (of 255).
const CHANNEL_TOLERANCE: u8 = 16;
/// At most this share of pixels may differ (anti-aliasing at edges).
const MAX_DIFFERENT: f64 = 0.003;

const ZOOM: f64 = 2.0;

fn fonts() -> Fonts {
    Fonts::new(Arc::new(FontDb::from_data([
        REGULAR.to_vec(),
        BOLD.to_vec(),
    ])))
}

/// Exports every artboard of `doc` and compares resvg's render of it with
/// ours of the same area.
fn assert_exports_like_canvas(doc: &Document) {
    let fonts = fonts();
    for artboard in &doc.artboards {
        let svg = export(
            doc,
            &fonts,
            Scope::Artboard(artboard.id),
            Format::Svg,
            &doc.export,
        )
        .expect("the artboard exists")
        .code;
        let r = artboard.rect;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small sizes
        let (w, h) = (
            (r.width * ZOOM).ceil() as u32,
            (r.height * ZOOM).ceil() as u32,
        );
        let viewport = Viewport {
            view: View {
                zoom: ZOOM,
                pan: Vec2::new(-r.x * ZOOM, -r.y * ZOOM),
            },
            width: w,
            height: h,
            pixels_per_point: 1.0,
        };
        let ours = render(doc, &fonts, &viewport).expect("renders");

        let mut opt = usvg::Options::default();
        opt.fontdb_mut().load_font_data(REGULAR.to_vec());
        opt.fontdb_mut().load_font_data(BOLD.to_vec());
        let tree = usvg::Tree::from_str(&svg, &opt).expect("the export parses");
        let mut reference = tiny_skia::Pixmap::new(w, h).expect("size");
        #[allow(clippy::cast_possible_truncation)] // constant 2.0
        let scale = ZOOM as f32;
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut reference.as_mut(),
        );

        let share = differing(ours.data(), reference.data());
        assert!(
            share <= MAX_DIFFERENT,
            "{}: {:.3} % of pixels differ\n{svg}",
            artboard.name,
            share * 100.0
        );
    }
}

fn differing(a: &[u8], b: &[u8]) -> f64 {
    let (a, b) = (a.as_chunks::<4>().0, b.as_chunks::<4>().0);
    let n = a
        .iter()
        .zip(b)
        .filter(|(a, b)| {
            a.iter()
                .zip(*b)
                .any(|(x, y)| x.abs_diff(*y) > CHANNEL_TOLERANCE)
        })
        .count();
    #[allow(clippy::cast_precision_loss)] // pixel counts are small
    let share = n as f64 / a.len() as f64;
    share
}

#[test]
fn basic_shapes_export_like_the_canvas() {
    assert_exports_like_canvas(&fixtures::basic_shapes());
}

#[test]
fn gradients_and_patterns_export_like_the_canvas() {
    assert_exports_like_canvas(&fixtures::paints());
}

#[test]
fn many_nodes_export_like_the_canvas() {
    assert_exports_like_canvas(&fixtures::many_nodes(200));
}

#[test]
fn every_option_exports_like_the_canvas() {
    let mut doc = fixtures::paints();
    for (styling, ids, precision) in [
        (
            vextor_core::Styling::StyleElement,
            vextor_core::IdMode::UniquePrefixed,
            3,
        ),
        (
            vextor_core::Styling::Attributes,
            vextor_core::IdMode::None,
            1,
        ),
    ] {
        doc.export.styling = styling;
        doc.export.ids = ids;
        doc.export.precision = precision;
        doc.export.minify = true;
        doc.export.responsive = false;
        assert_exports_like_canvas(&doc);
    }
}

/// Clips, nested transforms, group and layer opacity, and an object on a
/// second artboard.
fn clips_and_groups() -> Document {
    let mut doc = fixtures::basic_shapes();
    let layer = doc.layers[0];
    let (g, clip, a, b, inner) = (
        doc.alloc_node_id(),
        doc.alloc_node_id(),
        doc.alloc_node_id(),
        doc.alloc_node_id(),
        doc.alloc_node_id(),
    );
    let shape = |id, geometry, fill: Rgba| {
        Node::new(
            id,
            NodeKind::Shape(Shape {
                geometry,
                style: Style {
                    fill: Paint::Solid(fill),
                    stroke: Some(Stroke {
                        width: 2.0,
                        ..Stroke::default()
                    }),
                },
            }),
        )
    };
    let square = |x, y, s| Geometry::Rect {
        rect: Rect::new(x, y, s, s),
        radius: CornerRadii::default(),
    };
    let triangle = Geometry::Path(Path {
        subpaths: vec![SubPath {
            nodes: vec![
                PathNode::corner(0.0, 0.0),
                PathNode::corner(60.0, 0.0),
                PathNode::corner(0.0, 60.0),
            ],
            closed: true,
        }],
        fill_rule: FillRule::NonZero,
    });
    let mut group = Node::new(
        g,
        NodeKind::Group(Container {
            children: vec![a, inner, clip],
            clip: Some(clip),
        }),
    );
    group.transform = Transform::translate(120.0, 50.0);
    group.opacity = 0.7;
    let mut inner_node = Node::new(
        inner,
        NodeKind::Group(Container {
            children: vec![b],
            clip: None,
        }),
    );
    // Rotated and scaled: kept as one transform on the inner group.
    inner_node.transform = Transform([1.2, 0.5, -0.5, 1.2, 20.0, 5.0]);
    let mut clip_node = shape(clip, triangle, Rgba::BLACK);
    clip_node.transform = Transform::translate(-5.0, -5.0);
    doc.apply(Command::Insert {
        parent: Some(layer),
        index: 0,
        nodes: vec![
            group,
            shape(a, square(0.0, 0.0, 50.0), Rgba::rgb(0xff, 0xc1, 0x07)),
            inner_node,
            shape(b, square(0.0, 0.0, 20.0), Rgba::rgb(0x6a, 0x1b, 0x9a)),
            clip_node,
        ],
    })
    .expect("inserts");
    doc
}

#[test]
fn clips_transforms_and_opacity_export_like_the_canvas() {
    assert_exports_like_canvas(&clips_and_groups());
}

/// A framed, centred two-line label and a plain right-aligned line.
fn texts() -> Document {
    let mut doc = fixtures::basic_shapes();
    let layer = doc.layers[0];
    let (label, plain) = (doc.alloc_node_id(), doc.alloc_node_id());
    let framed = Text {
        size: 20.0,
        align: TextAlign::Centre,
        style: Style {
            fill: Paint::Solid(Rgba::rgb(255, 255, 255)),
            stroke: None,
        },
        frame: Some(TextFrame {
            padding: [6.0, 12.0, 6.0, 12.0],
            radius: CornerRadii {
                tl: 6.0,
                tr: 6.0,
                br: 6.0,
                bl: 6.0,
            },
            fill: Paint::Solid(Rgba::rgb(30, 144, 255)),
            stroke: Some(Stroke {
                paint: Paint::Solid(Rgba::rgb(11, 83, 148)),
                width: 2.0,
                ..Stroke::default()
            }),
        }),
        font: vextor_core::FontSpec {
            family: "Noto Sans".to_owned(),
            weight: 700,
            italic: false,
        },
        ..Text::new("Buy\nnow", "Noto Sans")
    };
    let right = Text {
        size: 14.0,
        align: TextAlign::Right,
        letter_spacing: 1.0,
        ..Text::new("Total 42", "Noto Sans")
    };
    let mut a = Node::new(label, NodeKind::Text(framed));
    a.transform = Transform::translate(130.0, 60.0);
    let mut b = Node::new(plain, NodeKind::Text(right));
    b.transform = Transform::translate(20.0, 90.0);
    doc.apply(Command::Insert {
        parent: Some(layer),
        index: 0,
        nodes: vec![a, b],
    })
    .expect("inserts");
    doc
}

#[test]
fn live_text_exports_like_the_canvas() {
    assert_exports_like_canvas(&texts());
}

#[test]
fn outlined_text_exports_like_the_canvas() {
    let mut doc = texts();
    doc.export.text_as_outlines = true;
    assert_exports_like_canvas(&doc);
}
