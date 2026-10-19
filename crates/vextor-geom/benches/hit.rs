//! Hit-testing cost. Budget (`performance` skill): the canvas has ≤ 8 ms per
//! frame on a 500-node document; a click or a marquee frame must take a
//! small part of it.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use kurbo::{Point, Rect};
use vextor_core::{Command, CornerRadii, Document, Geometry, Node, NodeKind, Shape, Stroke, Style};
use vextor_geom::{pick, retarget, touching};

/// `n` stroked rounded rects on a grid, 40 px apart.
fn grid(n: u32) -> Document {
    let mut doc = Document::new("bench");
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
                        radius: CornerRadii {
                            tl: 6.0,
                            tr: 6.0,
                            br: 6.0,
                            bl: 6.0,
                        },
                    },
                    style: Style {
                        stroke: Some(Stroke::default()),
                        ..Style::default()
                    },
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

fn bench_hit(c: &mut Criterion) {
    let doc = grid(500);
    c.bench_function("pick, 500 nodes, miss", |b| {
        b.iter(|| pick(black_box(&doc), black_box(Point::new(-5.0, -5.0)), 1.0));
    });
    c.bench_function("pick, 500 nodes, bottom-most hit", |b| {
        b.iter(|| pick(black_box(&doc), black_box(Point::new(15.0, 15.0)), 1.0));
    });
    c.bench_function("marquee, 500 nodes, half the grid", |b| {
        b.iter(|| {
            touching(
                black_box(&doc),
                black_box(Rect::new(0.0, 0.0, 500.0, 400.0)),
            )
        });
    });
}

/// One frame of dragging: new transforms from the press-time document,
/// applied as one validated command (the canvas then renders).
fn bench_drag_frame(c: &mut Criterion) {
    let before = grid(500);
    let layer = before.layers[0];
    let all = before.nodes[&layer]
        .container()
        .expect("layer")
        .children
        .clone();
    for (name, ids) in [("1 of 500", &all[..1]), ("500 of 500", &all[..])] {
        c.bench_function(&format!("drag frame, move {name} nodes"), |b| {
            b.iter(|| {
                let mut doc = before.clone();
                let items = retarget(&before, ids, kurbo::Affine::translate((3.0, 4.0)));
                doc.apply(Command::SetTransforms { items })
                    .expect("applies");
                black_box(doc)
            });
        });
    }
    c.bench_function("clone 500-node document (baseline of the above)", |b| {
        b.iter(|| black_box(before.clone()));
    });
}

criterion_group!(benches, bench_hit, bench_drag_frame);
criterion_main!(benches);
