//! Render comparison: our canvas rasteriser against `resvg` rendering a
//! hand-written SVG twin of the same fixture.

use resvg::{tiny_skia, usvg};
use vextor_core::{Vec2, View};
use vextor_geom::Fonts;
use vextor_render::{Pixmap, Viewport, fixtures, render};

/// A pixel differs when any channel is off by more than this (of 255).
const CHANNEL_TOLERANCE: u8 = 8;
/// At most this share of pixels may differ (anti-aliasing at edges).
const MAX_DIFFERENT: f64 = 0.001;

const ZOOM: f64 = 2.0;

fn reference(svg: &str, width: u32, height: u32) -> tiny_skia::Pixmap {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).expect("fixture SVG parses");
    let mut pixmap = tiny_skia::Pixmap::new(width, height).expect("non-empty size");
    #[allow(clippy::cast_possible_truncation)] // constant 2.0
    let scale = ZOOM as f32;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
}

fn ours(doc: &vextor_core::Document, width: u32, height: u32) -> Pixmap {
    let viewport = Viewport {
        view: View {
            zoom: ZOOM,
            pan: Vec2::new(0.0, 0.0),
        },
        width,
        height,
        pixels_per_point: 1.0,
    };
    render(doc, &Fonts::default(), &viewport).expect("valid viewport")
}

/// RGBA of the pixel at `(x, y)`.
fn pixel(data: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [data[i], data[i + 1], data[i + 2], data[i + 3]]
}

/// Writes both images and a diff (red = differs) next to the test binary.
fn save_failure(name: &str, actual: &Pixmap, expected: &tiny_skia::Pixmap, different: &[bool]) {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&dir).expect("target tmp dir is writable");
    let size = tiny_skia::IntSize::from_wh(actual.width(), actual.height()).expect("non-empty");
    let actual = tiny_skia::Pixmap::from_vec(actual.data().to_vec(), size).expect("same size");
    let diff: Vec<u8> = different
        .iter()
        .flat_map(|&d| if d { [255, 0, 0, 255] } else { [0, 0, 0, 0] })
        .collect();
    let diff = tiny_skia::Pixmap::from_vec(diff, size).expect("same size");
    for (name, img) in [("actual", &actual), ("expected", expected), ("diff", &diff)] {
        img.save_png(dir.join(format!("{name}.png")))
            .expect("PNG is writable");
    }
    eprintln!("render comparison images written to {}", dir.display());
}

/// Renders `doc` (`size` in document px) and its SVG twin `svg` and
/// checks that they match within the tolerances above.
fn compare(name: &str, doc: &vextor_core::Document, svg: &str, size: (f64, f64)) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small fixtures
    let (width, height) = ((size.0 * ZOOM) as u32, (size.1 * ZOOM) as u32);
    let expected = reference(svg, width, height);
    let actual = ours(doc, width, height);

    let different: Vec<bool> = actual
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.data().as_chunks::<4>().0)
        .map(|(a, e)| {
            a.iter()
                .zip(e)
                .any(|(a, e)| a.abs_diff(*e) > CHANNEL_TOLERANCE)
        })
        .collect();
    let count = different.iter().filter(|&&d| d).count();
    #[allow(clippy::cast_precision_loss)] // pixel counts are small
    let share = count as f64 / different.len() as f64;
    if share > MAX_DIFFERENT {
        save_failure(name, &actual, &expected, &different);
    }
    assert!(
        share <= MAX_DIFFERENT,
        "{name}: {count} of {} pixels differ ({:.3} %)",
        different.len(),
        share * 100.0
    );
}

#[test]
fn basic_shapes_matches_reference() {
    compare(
        "basic_shapes",
        &fixtures::basic_shapes(),
        include_str!("fixtures/basic_shapes.svg"),
        fixtures::BASIC_SHAPES_SIZE,
    );
}

#[test]
fn paints_match_reference() {
    // Spec 09: gradients, patterns, a gradient stroke, dots, opacity.
    compare(
        "paints",
        &fixtures::paints(),
        include_str!("fixtures/paints.svg"),
        fixtures::PAINTS_SIZE,
    );
}

#[test]
fn basic_shapes_is_not_blank() {
    // Guards the comparison above against two equally empty images.
    let (width, height) = (400, 240);
    let actual = ours(&fixtures::basic_shapes(), width, height);
    // Centre of the rounded rect (40, 30) at zoom 2: #1e90ff, opaque.
    assert_eq!(
        pixel(actual.data(), width, 80, 60),
        [0x1e, 0x90, 0xff, 0xff]
    );
    // White artboard background in an empty corner.
    assert_eq!(
        pixel(actual.data(), width, 396, 4),
        [0xff, 0xff, 0xff, 0xff]
    );
}

#[test]
fn hidden_layers_are_not_drawn() {
    // Spec 10: hiding a layer from the Layers panel hides its contents.
    let (width, height) = (400, 240);
    let mut doc = fixtures::basic_shapes();
    for layer in doc.layers.clone() {
        let n = doc.nodes[&layer].clone();
        doc.apply(vextor_core::Command::SetProps {
            id: layer,
            name: n.name,
            visible: false,
            locked: n.locked,
            opacity: n.opacity,
        })
        .expect("hides");
    }
    let actual = ours(&doc, width, height);
    // Where the rounded rect was: only the white artboard is left.
    assert_eq!(
        pixel(actual.data(), width, 80, 60),
        [0xff, 0xff, 0xff, 0xff]
    );
}

#[test]
fn paints_are_not_blank() {
    // Guards the comparison above against two equally empty images.
    let (width, height) = (400, 320);
    let actual = ours(&fixtures::paints(), width, height);
    let at = |x: f64, y: f64| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // in the image
        pixel(actual.data(), width, (x * ZOOM) as u32, (y * ZOOM) as u32)
    };
    // Top-left of the diagonal gradient: its first stop, red.
    let [r, g, b, _] = at(11.0, 11.0);
    assert!(r > 240 && g < 20 && b < 20, "{:?}", [r, g, b]);
    // The dots pattern's dark square, then its orange circle.
    assert_eq!(at(72.0, 72.0), [0x22, 0x22, 0x22, 0xff]);
    let [r, g, b, _] = at(77.5, 77.5);
    assert!(
        r > 240 && (0x50..0x70).contains(&g) && b < 0x50,
        "{:?}",
        [r, g, b]
    );
}

#[test]
fn a_pattern_painting_itself_on_many_scaled_objects_still_renders() {
    // A hostile file (spec 09): nesting and the tile budget bound the work.
    use vextor_core::{
        Command, Document, Geometry, Node, NodeKind, Paint, Pattern, Rect, Resource, Shape, Style,
        Transform,
    };
    let mut doc = Document::new("hostile");
    let pid = doc.alloc_resource_id();
    let cell = doc.alloc_node_id();
    let square = |paint| {
        NodeKind::Shape(Shape {
            geometry: Geometry::Rect {
                rect: Rect::new(0.0, 0.0, 4.0, 4.0),
                radius: vextor_core::CornerRadii::default(),
            },
            style: Style {
                fill: paint,
                stroke: None,
            },
        })
    };
    doc.apply(Command::PutResource {
        id: pid,
        resource: Resource::Pattern(Pattern {
            name: None,
            tile: Rect::new(0.0, 0.0, 8.0, 8.0),
            transform: Transform::IDENTITY,
            content: vec![cell],
        }),
        nodes: vec![Node::new(cell, square(Paint::Pattern(pid)))],
    })
    .expect("a pattern may paint with itself");
    let nodes: Vec<Node> = (0..2_000)
        .map(|i| {
            let mut n = Node::new(doc.alloc_node_id(), square(Paint::Pattern(pid)));
            let k = 1.0 + f64::from(i) / 100.0;
            n.transform = Transform::scale(k, k);
            n
        })
        .collect();
    doc.apply(Command::Insert {
        parent: Some(doc.layers[0]),
        index: 0,
        nodes,
    })
    .expect("nodes");
    // Terminates: nesting stops at 4 tiles deep, and at most 256 tiles
    // are drawn whatever the number of scales.
    let _ = ours(&doc, 400, 400);
}
