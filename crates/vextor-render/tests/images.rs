//! Embedded images on the canvas (spec 13): drawn stretched over their
//! rect, each pixel where SVG puts it; broken data draws nothing.

use vextor_core::{Document, ImageSource, NodeKind, Vec2, View};
use vextor_geom::Fonts;
use vextor_render::{Pixmap, Viewport, render};

/// 2 × 2: red, blue / green, half-transparent yellow.
const PNG: &[u8] = include_bytes!("../../vextor-svg/tests/fixtures/import/pixel_2x2.png");

fn document(bytes: &[u8]) -> Document {
    let imported = vextor_svg::import_image(PNG).expect("png");
    let mut doc = vextor_svg::into_document(&imported, "image").expect("places");
    for node in doc.nodes.values_mut() {
        if let NodeKind::Image(image) = &mut node.kind {
            let ImageSource::Embedded { bytes: b, .. } = &mut image.source;
            *b = bytes.to_vec();
        }
    }
    doc
}

/// The document drawn 10 × (one image pixel = 10 × 10 canvas pixels).
fn draw(doc: &Document) -> Pixmap {
    let viewport = Viewport {
        view: View {
            zoom: 10.0,
            pan: Vec2::new(0.0, 0.0),
        },
        width: 20,
        height: 20,
        pixels_per_point: 1.0,
    };
    render(doc, &Fonts::default(), &viewport).expect("valid viewport")
}

fn rgba(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
    let c = p.pixel(x, y).expect("inside").demultiply();
    [c.red(), c.green(), c.blue(), c.alpha()]
}

fn near(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 24)
}

#[test]
fn an_embedded_png_is_drawn_over_its_rect() {
    let p = draw(&document(PNG));
    // Pixel centres, away from the bicubic blend between them.
    for (x, y, want) in [
        (3, 3, [255, 0, 0, 255]),
        (16, 3, [0, 0, 255, 255]),
        (3, 16, [0, 255, 0, 255]),
    ] {
        let got = rgba(&p, x, y);
        assert!(near(got, want), "({x}, {y}): {got:?}, want {want:?}");
    }
}

#[test]
fn a_broken_image_draws_nothing_and_does_not_panic() {
    let blank = draw(&document(b"\x89PNG\r\n\x1a\nbroken"));
    let red = rgba(&blank, 3, 3);
    assert!(!near(red, [255, 0, 0, 255]), "{red:?}");
}
