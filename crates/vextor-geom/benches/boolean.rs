//! Path operation cost (spec 11). Budget (`performance` skill): each
//! operation on two shapes of 200 segments ≤ 50 ms (release).

use std::f64::consts::TAU;
use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use kurbo::{BezPath, Point};
use vextor_core::{FillRule, LineJoin, Stroke};
use vextor_geom::{Area, BooleanOp, divide, offset, outline_stroke, simplify};

/// A closed wobbly ring of `segments` cubics around `(cx, cy)`.
fn wavy(cx: f64, cy: f64, radius: f64, segments: u32, phase: f64) -> BezPath {
    let at = |step: f64| {
        let angle = step / f64::from(segments) * TAU;
        let wobble = radius * (1.0 + 0.08 * (angle * 7.0 + phase).sin());
        Point::new(cx + wobble * angle.cos(), cy + wobble * angle.sin())
    };
    let mut path = BezPath::new();
    path.move_to(at(0.0));
    for step in 0..segments {
        let step = f64::from(step);
        let (from, to) = (at(step), at(step + 1.0));
        let (before, after) = (at(step - 1.0), at(step + 2.0));
        path.curve_to(from + (to - before) / 6.0, to - (after - from) / 6.0, to);
    }
    path.close_path();
    path
}

fn ops(c: &mut Criterion) {
    let a = wavy(50.0, 50.0, 40.0, 200, 0.0);
    let b = wavy(70.0, 55.0, 40.0, 200, 1.3);
    let areas = || {
        [
            Area::new(&a, FillRule::NonZero),
            Area::new(&b, FillRule::NonZero),
        ]
    };
    for (name, op) in [
        ("union", BooleanOp::Union),
        ("subtract", BooleanOp::Subtract),
        ("intersect", BooleanOp::Intersect),
        ("exclude", BooleanOp::Exclude),
    ] {
        c.bench_function(&format!("{name} 2×200 segments"), |bench| {
            bench.iter(|| Area::combine(op, &areas()).to_bezpath());
        });
    }
    c.bench_function("divide 2×200 segments", |bench| {
        bench.iter(|| {
            let pieces: Vec<BezPath> = divide(&areas())
                .iter()
                .map(|(p, _)| p.to_bezpath())
                .collect();
            pieces
        });
    });
    c.bench_function("offset 200 segments", |bench| {
        bench.iter(|| offset(black_box(&a), FillRule::NonZero, 4.0, LineJoin::Round, 4.0));
    });
    let stroke = Stroke {
        width: 3.0,
        dash: vec![6.0, 3.0],
        ..Stroke::default()
    };
    c.bench_function("outline stroke 200 segments", |bench| {
        bench.iter(|| outline_stroke(black_box(&a), &stroke));
    });
    c.bench_function("simplify 200 segments", |bench| {
        bench.iter(|| simplify(black_box(&a), 0.25));
    });
}

criterion_group!(benches, ops);
criterion_main!(benches);
