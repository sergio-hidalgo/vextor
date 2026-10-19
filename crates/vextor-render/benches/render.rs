//! Canvas render cost. Budget (`performance` skill): ≤ 8 ms per frame for
//! a 500-node document. The case measured is the worst one for pan and
//! zoom: a full re-render of a 1280×800 pt canvas on a 2× (Retina) screen.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::sync::Arc;
use vextor_core::{Command, Document, Node, NodeKind, Text, Transform, Vec2, View};
use vextor_geom::{FontDb, Fonts};
use vextor_render::{Pixmap, Viewport, fixtures, render_into};

const REGULAR: &[u8] =
    include_bytes!("../../vextor-geom/tests/fixtures/fonts/NotoSans-Regular.ttf");

/// One 18 px text of 1 000 characters over 55 lines.
fn long_text() -> Document {
    let mut doc = Document::new("text");
    let layer = doc.layers[0];
    let id = doc.alloc_node_id();
    let text = Text {
        size: 18.0,
        ..Text::new("Buy now, save 20%\n".repeat(55), "Noto Sans")
    };
    let mut node = Node::new(id, NodeKind::Text(text));
    node.transform = Transform([1.0, 0.0, 0.0, 1.0, 20.0, 20.0]);
    doc.apply(Command::Insert {
        parent: Some(layer),
        index: 0,
        nodes: vec![node],
    })
    .expect("inserts");
    doc
}

fn viewport(zoom: f64, pan: Vec2) -> Viewport {
    Viewport {
        view: View { zoom, pan },
        width: 2560,
        height: 1600,
        pixels_per_point: 2.0,
    }
}

fn bench_render(c: &mut Criterion) {
    let doc = fixtures::many_nodes(500);
    let mut pixmap = Pixmap::new(2560, 1600).expect("non-empty size");

    // The whole 1000×800 artboard in view.
    let fit = viewport(1.0, Vec2::new(140.0, 0.0));
    c.bench_function("render 500 nodes, fit, 2560×1600", |b| {
        b.iter(|| {
            render_into(
                black_box(&doc),
                &Fonts::default(),
                black_box(&fit),
                &mut pixmap,
            )
        });
    });

    // Same canvas, no shapes: the fixed cost of clearing and the artboard.
    let empty = fixtures::many_nodes(0);
    c.bench_function("render 0 nodes, fit, 2560×1600", |b| {
        b.iter(|| {
            render_into(
                black_box(&empty),
                &Fonts::default(),
                black_box(&fit),
                &mut pixmap,
            )
        });
    });

    // Zoomed in 4×: most shapes outside the view.
    let zoomed = viewport(4.0, Vec2::new(-800.0, -600.0));
    c.bench_function("render 500 nodes, 4× zoom, 2560×1600", |b| {
        b.iter(|| {
            render_into(
                black_box(&doc),
                &Fonts::default(),
                black_box(&zoomed),
                &mut pixmap,
            )
        });
    });
}

/// Typing: every keystroke lays the text out again and re-renders.
fn bench_text(c: &mut Criterion) {
    let db = Arc::new(FontDb::from_data([REGULAR.to_vec()]));
    let doc = long_text();
    let mut pixmap = Pixmap::new(2560, 1600).expect("non-empty size");
    let fit = viewport(1.0, Vec2::new(140.0, 0.0));
    c.bench_function("layout + render 1 000-char text, 2560×1600", |b| {
        b.iter(|| {
            let fonts = Fonts::new(Arc::clone(&db));
            render_into(black_box(&doc), &fonts, black_box(&fit), &mut pixmap)
        });
    });
}

criterion_group!(benches, bench_render, bench_text);
criterion_main!(benches);
