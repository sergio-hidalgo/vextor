//! Render comparison: our canvas rasteriser against `resvg` rendering a
//! hand-written SVG twin of the same fixture.

use resvg::{tiny_skia, usvg};
use vextor_core::{Vec2, View};
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

fn ours(width: u32, height: u32) -> Pixmap {
    let viewport = Viewport {
        view: View {
            zoom: ZOOM,
            pan: Vec2::new(0.0, 0.0),
        },
        width,
        height,
        pixels_per_point: 1.0,
    };
    render(&fixtures::basic_shapes(), &viewport).expect("valid viewport")
}

/// RGBA of the pixel at `(x, y)`.
fn pixel(data: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [data[i], data[i + 1], data[i + 2], data[i + 3]]
}

/// Writes both images and a diff (red = differs) next to the test binary.
fn save_failure(actual: &Pixmap, expected: &tiny_skia::Pixmap, different: &[bool]) {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("basic_shapes");
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

#[test]
fn basic_shapes_matches_reference() {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 400×240
    let (width, height) = (
        (fixtures::BASIC_SHAPES_SIZE.0 * ZOOM) as u32,
        (fixtures::BASIC_SHAPES_SIZE.1 * ZOOM) as u32,
    );
    let svg = include_str!("fixtures/basic_shapes.svg");
    let expected = reference(svg, width, height);
    let actual = ours(width, height);

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
        save_failure(&actual, &expected, &different);
    }
    assert!(
        share <= MAX_DIFFERENT,
        "{count} of {} pixels differ ({:.3} %)",
        different.len(),
        share * 100.0
    );
}

#[test]
fn basic_shapes_is_not_blank() {
    // Guards the comparison above against two equally empty images.
    let (width, height) = (400, 240);
    let actual = ours(width, height);
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
