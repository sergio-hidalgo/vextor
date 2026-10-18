//! Canvas render cost. Budget (`performance` skill): ≤ 8 ms per frame for
//! a 500-node document. The case measured is the worst one for pan and
//! zoom: a full re-render of a 1280×800 pt canvas on a 2× (Retina) screen.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use vextor_core::{Vec2, View};
use vextor_render::{Pixmap, Viewport, fixtures, render_into};

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
        b.iter(|| render_into(black_box(&doc), black_box(&fit), &mut pixmap));
    });

    // Same canvas, no shapes: the fixed cost of clearing and the artboard.
    let empty = fixtures::many_nodes(0);
    c.bench_function("render 0 nodes, fit, 2560×1600", |b| {
        b.iter(|| render_into(black_box(&empty), black_box(&fit), &mut pixmap));
    });

    // Zoomed in 4×: most shapes outside the view.
    let zoomed = viewport(4.0, Vec2::new(-800.0, -600.0));
    c.bench_function("render 500 nodes, 4× zoom, 2560×1600", |b| {
        b.iter(|| render_into(black_box(&doc), black_box(&zoomed), &mut pixmap));
    });
}

criterion_group!(benches, bench_render);
criterion_main!(benches);
