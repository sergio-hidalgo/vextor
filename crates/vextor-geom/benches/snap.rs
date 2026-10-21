//! Snapping cost (spec 08). Budget (`performance` skill): ≤ 8 ms of our
//! work per frame on a 500-node document; collecting runs once per drag,
//! resolving every frame.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use kurbo::{Point, Rect};
use vextor_core::{Command, Document, Geometry, Node, NodeKind, Shape, Style};
use vextor_geom::{Fonts, SnapTargets};

/// `n` rects on a grid, 40 px apart, snapping to everything.
fn grid(n: u32) -> Document {
    let mut doc = Document::new("bench");
    doc.grid.snap.to_grid = true;
    let layer = doc.layers[0];
    let nodes: Vec<Node> = (0..n)
        .map(|i| {
            let id = doc.alloc_node_id();
            let (x, y) = (f64::from(i % 25) * 40.0, f64::from(i / 25) * 40.0);
            Node::new(
                id,
                NodeKind::Shape(Shape {
                    geometry: Geometry::Rect {
                        rect: vextor_core::Rect::new(x, y, 30.0, 30.0),
                        radius: vextor_core::CornerRadii::default(),
                    },
                    style: Style::default(),
                }),
            )
        })
        .collect();
    doc.apply(Command::Insert {
        parent: Some(layer),
        index: 0,
        nodes,
    })
    .expect("inserts");
    doc
}

fn bench_snap(c: &mut Criterion) {
    let doc = grid(500);
    c.bench_function("snap collect, 500 nodes", |b| {
        b.iter(|| SnapTargets::collect(black_box(&doc), &Fonts::default(), &[]));
    });
    let t = SnapTargets::collect(&doc, &Fonts::default(), &[]);
    c.bench_function("snap point, 500 nodes", |b| {
        b.iter(|| t.point(black_box(Point::new(415.3, 213.7)), 1.5));
    });
    c.bench_function("snap moving box, 500 nodes", |b| {
        b.iter(|| t.moving_box(black_box(Rect::new(415.3, 213.7, 445.3, 243.7)), 1.5));
    });
}

criterion_group!(benches, bench_snap);
criterion_main!(benches);
