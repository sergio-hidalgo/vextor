//! proptest (spec 14): transforms compose and invert within `EPS` for the
//! sizes a web component uses (scales 0.1–10×, moves up to 10 000 px).

use proptest::prelude::*;
use vextor_core::{EPS, Point, Transform};

/// Rotation, non-uniform scale (maybe mirrored), then a move.
fn transform() -> impl Strategy<Value = Transform> {
    (
        -180.0..180.0f64,
        0.1..10.0f64,
        0.1..10.0f64,
        any::<bool>(),
        -10_000.0..10_000.0f64,
        -10_000.0..10_000.0f64,
    )
        .prop_map(|(deg, sx, sy, flip, tx, ty)| {
            let (sin, cos) = deg.to_radians().sin_cos();
            let sx = if flip { -sx } else { sx };
            Transform([cos * sx, sin * sx, -sin * sy, cos * sy, tx, ty])
        })
}

fn point() -> impl Strategy<Value = Point> {
    (-2_000.0..2_000.0f64, -2_000.0..2_000.0f64).prop_map(|(x, y)| Point::new(x, y))
}

fn close(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn a_transform_then_its_inverse_maps_every_point_back(t in transform(), p in point()) {
        let inverse = t.invert().expect("scales ≥ 0.1 never collapse");
        let back = inverse.apply(t.apply(p));
        prop_assert!(close(back, p), "{p:?} → {back:?}");
        prop_assert!(close(t.then(inverse).apply(p), p));
        prop_assert!(close(inverse.then(t).apply(p), p));
    }

    #[test]
    fn composing_is_applying_one_after_the_other(a in transform(), b in transform(), p in point()) {
        let together = a.then(b).apply(p);
        let one_by_one = b.apply(a.apply(p));
        // Coordinates reach ~10⁶ px here; compare relative to their size.
        let scale = together.x.abs().max(together.y.abs()).max(1.0);
        prop_assert!((together.x - one_by_one.x).abs() < EPS * scale);
        prop_assert!((together.y - one_by_one.y).abs() < EPS * scale);
    }

    #[test]
    fn the_inverse_of_a_composition_is_the_reversed_inverses(a in transform(), b in transform(), p in point()) {
        let whole = a.then(b).invert().expect("invertible");
        let parts = b.invert().expect("invertible").then(a.invert().expect("invertible"));
        let (x, y) = (whole.apply(p), parts.apply(p));
        let scale = x.x.abs().max(x.y.abs()).max(1.0);
        prop_assert!((x.x - y.x).abs() < EPS * scale && (x.y - y.y).abs() < EPS * scale);
    }
}
