//! Path operations (spec 11): the case table for every operation, checked
//! against the truth sampled from the inputs, plus curve preservation and
//! valid model paths.

use kurbo::{BezPath, Circle, PathSeg, Point, Rect, Shape};
use vextor_core::{FillRule, LineCap, LineJoin, Path, Stroke};
use vextor_geom::{Area, BooleanOp, divide, offset, outline_stroke, result_path, simplify};

/// An input outline with its fill rule.
#[derive(Clone)]
struct Input {
    path: BezPath,
    rule: FillRule,
}

fn input(path: BezPath) -> Input {
    Input {
        path,
        rule: FillRule::NonZero,
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Input {
    input(Rect::new(x0, y0, x1, y1).to_path(0.1))
}

fn circle(x: f64, y: f64, r: f64) -> Input {
    input(Circle::new((x, y), r).to_path(0.1))
}

/// A ring: a disc of radius 50 with a hole of 25, even-odd.
fn donut() -> Input {
    let mut p = Circle::new((50.0, 50.0), 50.0).to_path(0.1);
    p.extend(Circle::new((50.0, 50.0), 25.0).to_path(0.1));
    Input {
        path: p,
        rule: FillRule::EvenOdd,
    }
}

/// A bow tie: one subpath crossing itself.
fn bow_tie() -> Input {
    let mut p = BezPath::new();
    p.move_to((0.0, 0.0));
    p.line_to((100.0, 100.0));
    p.line_to((100.0, 0.0));
    p.line_to((0.0, 100.0));
    p.close_path();
    input(p)
}

/// An open arch (no close): filled as if closed.
fn arch() -> Input {
    let mut p = BezPath::new();
    p.move_to((0.0, 50.0));
    p.curve_to((0.0, -16.0), (100.0, -16.0), (100.0, 50.0));
    input(p)
}

/// The case table of the spec: name, bottom, top.
fn cases() -> Vec<(&'static str, Input, Input)> {
    vec![
        (
            "disjoint",
            rect(0., 0., 40., 40.),
            rect(60., 60., 100., 100.),
        ),
        (
            "touching edge",
            rect(0., 0., 50., 50.),
            rect(50., 0., 100., 50.),
        ),
        (
            "touching point",
            rect(0., 0., 50., 50.),
            rect(50., 50., 100., 100.),
        ),
        ("nested", rect(0., 0., 100., 100.), rect(25., 25., 75., 75.)),
        ("identical", circle(50., 50., 40.), circle(50., 50., 40.)),
        ("with holes", donut(), rect(40., -10., 60., 110.)),
        ("self-intersecting", bow_tie(), rect(20., 20., 80., 80.)),
        ("open path", arch(), rect(30., 0., 70., 100.)),
        (
            "overlapping circles",
            circle(35., 50., 30.),
            circle(65., 50., 30.),
        ),
    ]
}

const OPS: [BooleanOp; 4] = [
    BooleanOp::Union,
    BooleanOp::Subtract,
    BooleanOp::Intersect,
    BooleanOp::Exclude,
];

/// The rings of `path` (open subpaths closed), flattened far finer than
/// anything the operations promise: the truth the tests compare with.
/// (`kurbo`'s own `winding` miscounts some cubics that turn back in y.)
struct Region {
    rings: Vec<(Rect, Vec<Point>)>,
    rule: FillRule,
}

impl Region {
    fn new(path: &BezPath, rule: FillRule) -> Self {
        let mut rings: Vec<Vec<Point>> = Vec::new();
        kurbo::flatten(vextor_closed(path), 1e-3, |el| match el {
            kurbo::PathEl::MoveTo(p) => rings.push(vec![p]),
            kurbo::PathEl::LineTo(p) => rings.last_mut().expect("moved").push(p),
            _ => {}
        });
        let rings = rings
            .into_iter()
            .map(|r| {
                (
                    r.iter()
                        .fold(Rect::from_points(r[0], r[0]), |b, p| b.union_pt(*p)),
                    r,
                )
            })
            .collect();
        Self { rings, rule }
    }

    fn contains(&self, point: Point) -> bool {
        let mut turns = 0;
        for (bounds, ring) in &self.rings {
            if point.y < bounds.y0 || point.y > bounds.y1 || point.x > bounds.x1 {
                continue; // the ray to the right cannot cross it
            }
            for (i, a) in ring.iter().enumerate() {
                let b = ring[(i + 1) % ring.len()];
                let side = (b.x - a.x) * (point.y - a.y) - (point.x - a.x) * (b.y - a.y);
                if a.y <= point.y {
                    if b.y > point.y && side > 0.0 {
                        turns += 1;
                    }
                } else if b.y <= point.y && side < 0.0 {
                    turns -= 1;
                }
            }
        }
        match self.rule {
            FillRule::NonZero => turns != 0,
            FillRule::EvenOdd => turns % 2 != 0,
        }
    }
}

/// `path` with open subpaths closed, as fills treat them.
fn vextor_closed(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    let mut open = false;
    for &el in path.elements() {
        if let kurbo::PathEl::MoveTo(_) = el {
            if open {
                out.close_path();
            }
            open = true;
        }
        if el == kurbo::PathEl::ClosePath {
            open = false;
        }
        out.push(el);
    }
    if open {
        out.close_path();
    }
    out
}

/// `n`×`n` sample points over the case area (-15..115), off any grid
/// line the inputs use.
fn samples(n: u32) -> impl Iterator<Item = Point> {
    let step = 130.0 / f64::from(n);
    (0..n).flat_map(move |i| {
        (0..n).map(move |j| {
            Point::new(
                -15.0 + (f64::from(i) + 0.37) * step,
                -15.0 + (f64::from(j) + 0.61) * step,
            )
        })
    })
}

/// Percentage of `n`×`n` samples where `result` (non-zero) disagrees
/// with `truth`.
fn error(result: &BezPath, truth: impl Fn(Point) -> bool, n: u32) -> f64 {
    let result = Region::new(result, FillRule::NonZero);
    let (mut bad, mut count) = (0_u32, 0_u32);
    for p in samples(n) {
        count += 1;
        if result.contains(p) != truth(p) {
            bad += 1;
        }
    }
    f64::from(bad) * 100.0 / f64::from(count)
}

fn truth(op: BooleanOp, a: &Input, b: &Input) -> impl Fn(Point) -> bool {
    let (ra, rb) = (Region::new(&a.path, a.rule), Region::new(&b.path, b.rule));
    move |p| {
        let (ia, ib) = (ra.contains(p), rb.contains(p));
        match op {
            BooleanOp::Union => ia || ib,
            BooleanOp::Subtract => ia && !ib,
            BooleanOp::Intersect => ia && ib,
            BooleanOp::Exclude => ia != ib,
        }
    }
}

fn area(i: &Input) -> Area {
    Area::new(&i.path, i.rule)
}

/// A result is a valid model path: closed subpaths of ≥ 2 nodes, finite.
fn assert_valid(path: &Path, what: &str) {
    for sp in &path.subpaths {
        assert!(sp.closed && sp.nodes.len() >= 2, "{what}: {sp:?}");
        for n in &sp.nodes {
            assert!(n.anchor.is_finite(), "{what}: {n:?}");
            assert!(
                n.handle_in.is_none_or(vextor_core::Vec2::is_finite),
                "{what}: {n:?}"
            );
            assert!(
                n.handle_out.is_none_or(vextor_core::Vec2::is_finite),
                "{what}: {n:?}"
            );
        }
    }
}

#[test]
fn every_boolean_matches_the_truth_on_the_case_table() {
    for (name, a, b) in cases() {
        for op in OPS {
            let out = Area::combine(op, &[area(&a), area(&b)]).to_bezpath();
            let e = error(&out, truth(op, &a, &b), 200);
            // Only where new intersections land is approximate.
            assert!(e < 0.05, "{name} {op:?}: {e:.3}% of samples wrong");
            if let Some(p) = result_path(&out, FillRule::NonZero) {
                assert_valid(&p, name);
            }
        }
    }
}

#[test]
fn empty_results_are_empty() {
    let (a, b) = (rect(0., 0., 40., 40.), rect(60., 60., 100., 100.));
    let disjoint = Area::combine(BooleanOp::Intersect, &[area(&a), area(&b)]);
    assert!(disjoint.is_empty());
    assert!(result_path(&disjoint.to_bezpath(), FillRule::NonZero).is_none());
    let c = circle(50., 50., 40.);
    let gone = Area::combine(BooleanOp::Subtract, &[area(&c), area(&c)]);
    assert!(gone.is_empty(), "A − A = ∅");
    let same = Area::combine(BooleanOp::Exclude, &[area(&c), area(&c)]);
    assert!(same.is_empty(), "A ⊕ A = ∅");
}

#[test]
fn touching_squares_unite_into_one_rectangle_of_four_corners() {
    let (a, b) = (rect(0., 0., 50., 50.), rect(50., 0., 100., 50.));
    let out = Area::combine(BooleanOp::Union, &[area(&a), area(&b)]);
    let path = result_path(&out.to_bezpath(), FillRule::NonZero).expect("a path");
    assert_eq!(path.subpaths.len(), 1);
    assert_eq!(path.subpaths[0].nodes.len(), 4, "the shared edge is gone");
    assert!((out.size() - 5000.0).abs() < 1e-6);
}

#[test]
fn nested_subtract_leaves_a_hole() {
    let (a, b) = (rect(0., 0., 100., 100.), rect(25., 25., 75., 75.));
    let out = Area::combine(BooleanOp::Subtract, &[area(&a), area(&b)]);
    assert!((out.size() - 7500.0).abs() < 1e-6);
    let path = result_path(&out.to_bezpath(), FillRule::NonZero).expect("a path");
    assert_eq!(path.subpaths.len(), 2, "outline and hole");
    assert!(!out.contains(Point::new(50.0, 50.0)));
}

#[test]
fn circles_keep_their_curves() {
    let (a, b) = (circle(35., 50., 30.), circle(65., 50., 30.));
    for op in OPS {
        let bez = Area::combine(op, &[area(&a), area(&b)]).to_bezpath();
        let segs: Vec<PathSeg> = bez.segments().collect();
        let lines = segs
            .iter()
            .filter(|s| matches!(s, PathSeg::Line(_)))
            .count();
        // Two circles of 4 cubics each, cut at two points: a handful of
        // arcs, never a polygon.
        assert!(segs.len() <= 16, "{op:?}: {} segments", segs.len());
        assert_eq!(lines, 0, "{op:?}: {segs:?}");
    }
    let union = Area::combine(BooleanOp::Union, &[area(&a), area(&b)]).to_bezpath();
    let path = result_path(&union, FillRule::NonZero).expect("a path");
    let smooth = path.subpaths[0]
        .nodes
        .iter()
        .filter(|n| n.kind != vextor_core::NodeType::Corner)
        .count();
    assert!(smooth >= 4, "nodes inside an arc are smooth: {path:?}");
}

#[test]
fn identical_circles_unite_into_the_same_circle() {
    let c = circle(50., 50., 40.);
    let bez = Area::combine(BooleanOp::Union, &[area(&c), area(&c)]).to_bezpath();
    assert!(bez.segments().count() <= 5, "{bez:?}");
    assert!((bez.area().abs() - c.path.area().abs()).abs() < 0.5);
}

#[test]
fn many_shapes_subtract_all_from_the_bottom_one() {
    let base = rect(0., 0., 100., 100.);
    let holes = [
        rect(10., 10., 30., 30.),
        rect(70., 70., 90., 90.),
        circle(50., 50., 10.),
    ];
    let mut areas = vec![area(&base)];
    areas.extend(holes.iter().map(area));
    let out = Area::combine(BooleanOp::Subtract, &areas);
    assert!(out.contains(Point::new(5.0, 5.0)));
    for p in [(20.0, 20.0), (80.0, 80.0), (50.0, 50.0)] {
        assert!(!out.contains(Point::from(p)), "{p:?} is cut out");
    }
}

#[test]
fn divide_splits_every_overlap_with_its_topmost_owner() {
    let (a, b) = (rect(0., 0., 60., 60.), rect(40., 40., 100., 100.));
    let pieces = divide(&[area(&a), area(&b)]);
    assert_eq!(pieces.len(), 3, "A only, overlap, B only");
    let owner = |p: (f64, f64)| {
        pieces
            .iter()
            .find(|(piece, _)| piece.contains(Point::from(p)))
            .map(|(_, top)| *top)
    };
    assert_eq!(owner((10.0, 10.0)), Some(0));
    assert_eq!(owner((50.0, 50.0)), Some(1), "the overlap goes to the top");
    assert_eq!(owner((90.0, 90.0)), Some(1));
    let total: f64 = pieces.iter().map(|(p, _)| p.size()).sum();
    assert!((total - (3600.0 * 2.0 - 400.0)).abs() < 1e-6);
}

#[test]
fn divide_separates_disconnected_pieces() {
    // A bar across a disc cuts it into left, middle and right.
    let (disc, bar) = (circle(50., 50., 40.), rect(40., 0., 60., 100.));
    let pieces = divide(&[area(&disc), area(&bar)]);
    assert_eq!(pieces.len(), 5, "disc left/right, bar top/middle/bottom");
}

#[test]
fn offset_grows_and_shrinks_a_square() {
    let sq = Rect::new(0.0, 0.0, 100.0, 100.0).to_path(0.1);
    let out = offset(&sq, FillRule::NonZero, 10.0, LineJoin::Miter, 4.0);
    assert!(
        (out.area().abs() - 120.0 * 120.0).abs() < 0.5,
        "{}",
        out.area()
    );
    let round = offset(&sq, FillRule::NonZero, 10.0, LineJoin::Round, 4.0);
    let want = 100.0 * 100.0 + 4.0 * 100.0 * 10.0 + std::f64::consts::PI * 100.0;
    assert!((round.area().abs() - want).abs() < 1.0, "{}", round.area());
    let inset = offset(&sq, FillRule::NonZero, -10.0, LineJoin::Miter, 4.0);
    assert!(
        (inset.area().abs() - 80.0 * 80.0).abs() < 0.5,
        "{}",
        inset.area()
    );
    let gone = offset(&sq, FillRule::NonZero, -60.0, LineJoin::Miter, 4.0);
    assert_eq!(gone.segments().count(), 0, "an inset past the middle");
}

#[test]
fn offset_of_a_circle_is_a_bigger_circle_of_curves() {
    let c = Circle::new((0.0, 0.0), 40.0).to_path(0.1);
    let out = offset(&c, FillRule::NonZero, 10.0, LineJoin::Round, 4.0);
    let want = std::f64::consts::PI * 50.0 * 50.0;
    assert!((out.area().abs() - want).abs() < want * 1e-3);
    assert!(out.segments().count() < 40, "{}", out.segments().count());
    for seg in out.segments() {
        for t in [0.0, 0.5, 1.0] {
            let p = kurbo::ParamCurve::eval(&seg, t);
            assert!((p.to_vec2().hypot() - 50.0).abs() < 0.05, "{p:?}");
        }
    }
}

#[test]
fn outline_stroke_respects_width_caps_and_dashes() {
    let mut line = BezPath::new();
    line.move_to((0.0, 0.0));
    line.line_to((100.0, 0.0));
    let stroke = |cap, dash: Vec<f64>| Stroke {
        width: 10.0,
        cap,
        dash,
        ..Stroke::default()
    };
    let butt = outline_stroke(&line, &stroke(LineCap::Butt, vec![]));
    assert!((butt.area().abs() - 1000.0).abs() < 1e-6);
    let square = outline_stroke(&line, &stroke(LineCap::Square, vec![]));
    assert!((square.area().abs() - 1100.0).abs() < 1e-6);
    let round = outline_stroke(&line, &stroke(LineCap::Round, vec![]));
    let want = 1000.0 + std::f64::consts::PI * 25.0;
    assert!((round.area().abs() - want).abs() < 0.1);
    let dashed = outline_stroke(&line, &stroke(LineCap::Butt, vec![10.0, 10.0]));
    let path = result_path(&dashed, FillRule::NonZero).expect("dashes");
    assert_eq!(path.subpaths.len(), 5, "five 10 px dashes");
    assert!((dashed.area().abs() - 500.0).abs() < 1e-6);
    let zero = outline_stroke(&line, &stroke(LineCap::Butt, vec![0.0, 0.0]));
    assert!(
        (zero.area().abs() - 1000.0).abs() < 1e-6,
        "all-zero dashes: solid"
    );
}

#[test]
fn outline_stroke_of_a_closed_square_is_a_frame() {
    let sq = Rect::new(0.0, 0.0, 100.0, 100.0).to_path(0.1);
    let s = Stroke {
        width: 10.0,
        join: LineJoin::Miter,
        ..Stroke::default()
    };
    let out = outline_stroke(&sq, &s);
    assert!((out.area().abs() - (110.0 * 110.0 - 90.0 * 90.0)).abs() < 1e-6);
    let path = result_path(&out, FillRule::NonZero).expect("a frame");
    assert_eq!(path.subpaths.len(), 2);
    assert_eq!(path.subpaths[0].nodes.len(), 4);
}

/// A circle drawn with `n` cubics: far more nodes than it needs.
fn over_noded_circle(n: u32) -> BezPath {
    let mut p = BezPath::new();
    let r = 40.0;
    let step = std::f64::consts::TAU / f64::from(n);
    let k = 4.0 / 3.0 * (step / 4.0).tan() * r;
    let at = |a: f64| Point::new(r * a.cos(), r * a.sin());
    let tangent = |a: f64| kurbo::Vec2::new(-a.sin(), a.cos());
    p.move_to(at(0.0));
    for i in 0..n {
        let (a0, a1) = (f64::from(i) * step, f64::from(i + 1) * step);
        p.curve_to(at(a0) + tangent(a0) * k, at(a1) - tangent(a1) * k, at(a1));
    }
    p.close_path();
    p
}

#[test]
fn simplify_drops_nodes_and_keeps_the_look() {
    let c = over_noded_circle(64);
    let out = simplify(&c, 0.1);
    assert!(out.segments().count() <= 8, "{}", out.segments().count());
    assert!(max_distance(&out, &c) <= 0.1 + 1e-6);
    // Straight runs: collinear and nearly collinear points go.
    let mut zigzag = BezPath::new();
    zigzag.move_to((0.0, 0.0));
    for i in 1..=20 {
        let y = if i % 2 == 0 { 0.0 } else { 0.05 };
        zigzag.line_to((f64::from(i) * 5.0, y));
    }
    zigzag.line_to((100.0, 50.0));
    zigzag.close_path();
    let thin = simplify(&zigzag, 0.2);
    assert_eq!(thin.segments().count(), 3, "{thin:?}");
    assert!(thin.segments().all(|s| matches!(s, PathSeg::Line(_))));
    // Nothing to save: the path itself.
    let sq = Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.1);
    assert_eq!(simplify(&sq, 1.0), sq);
}

/// The largest distance from a point of `a` to `b` (both ways), sampled.
fn max_distance(a: &BezPath, b: &BezPath) -> f64 {
    let one_way = |from: &BezPath, to: &BezPath| {
        from.segments()
            .flat_map(|s| (0..=16).map(move |i| kurbo::ParamCurve::eval(&s, f64::from(i) / 16.0)))
            .map(|p| {
                to.segments()
                    .map(|s| kurbo::ParamCurveNearest::nearest(&s, p, 1e-9).distance_sq)
                    .fold(f64::INFINITY, f64::min)
                    .sqrt()
            })
            .fold(0.0, f64::max)
    };
    one_way(a, b).max(one_way(b, a))
}

mod props {
    use proptest::prelude::*;

    use super::*;

    /// A random closed curvy blob around `(cx, cy)`.
    fn blob() -> impl Strategy<Value = BezPath> {
        (
            20.0..80.0_f64,
            20.0..80.0_f64,
            10.0..40.0_f64,
            3_u32..12,
            0.0..6.0_f64,
        )
            .prop_map(|(cx, cy, r, n, phase)| {
                let pt = |k: f64| {
                    let a = k / f64::from(n) * std::f64::consts::TAU;
                    let rr = r * (1.0 + 0.2 * (a * 3.0 + phase).sin());
                    Point::new(cx + rr * a.cos(), cy + rr * a.sin())
                };
                let mut p = BezPath::new();
                p.move_to(pt(0.0));
                for k in 0..n {
                    let k = f64::from(k);
                    let (a, b) = (pt(k), pt(k + 1.0));
                    let (before, after) = (pt(k - 1.0), pt(k + 2.0));
                    p.curve_to(a + (b - before) / 6.0, b - (after - a) / 6.0, b);
                }
                p.close_path();
                p
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn union_is_at_least_as_big_as_either(a in blob(), b in blob()) {
            let (aa, ab) = (Area::new(&a, FillRule::NonZero), Area::new(&b, FillRule::NonZero));
            let u = Area::combine(BooleanOp::Union, &[aa.clone(), ab.clone()]);
            prop_assert!(u.size() >= aa.size().max(ab.size()) - 1e-6);
        }

        #[test]
        fn a_minus_a_is_empty(a in blob()) {
            let aa = Area::new(&a, FillRule::NonZero);
            prop_assert!(Area::combine(BooleanOp::Subtract, &[aa.clone(), aa]).is_empty());
        }

        #[test]
        fn intersection_lies_inside_the_bottom_one(a in blob(), b in blob()) {
            let (aa, ab) = (Area::new(&a, FillRule::NonZero), Area::new(&b, FillRule::NonZero));
            let i = Area::combine(BooleanOp::Intersect, &[aa.clone(), ab]);
            let rest = Area::combine(BooleanOp::Subtract, &[i.clone(), aa.clone()]);
            prop_assert!(i.size() <= aa.size() + 1e-6);
            prop_assert!(rest.size() < 1e-3, "A ∩ B − A = {}", rest.size());
        }

        #[test]
        fn results_are_valid_and_follow_the_truth(a in blob(), b in blob(), k in 0_usize..4) {
            let op = OPS[k];
            let (ia, ib) = (input(a), input(b));
            let out = Area::combine(op, &[area(&ia), area(&ib)]).to_bezpath();
            prop_assert!(error(&out, truth(op, &ia, &ib), 60) < 0.3);
            if let Some(p) = result_path(&out, FillRule::NonZero) {
                assert_valid(&p, "random");
            }
        }

        #[test]
        fn simplify_stays_within_tolerance(a in blob(), tol in 0.05..2.0_f64) {
            let out = simplify(&a, tol);
            prop_assert!(out.segments().count() <= a.segments().count());
            let d = max_distance(&out, &a);
            prop_assert!(d <= tol * 1.01 + 1e-6, "{d} > {tol}");
        }
    }
}
