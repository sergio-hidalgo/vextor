//! Path editing geometry (spec 07): hit-tests under transforms, splitting
//! without changing the outline, bending, smooth handles, angle steps,
//! joins and node corners.

use kurbo::{Affine, Line, ParamCurve, ParamCurveNearest, Point, Vec2};
use proptest::prelude::*;
use vextor_core::{EPS, FillRule, NodeType, Path, PathNode, SubPath};
use vextor_geom::{
    bend, constrain_angle, hit_node, hit_segment, is_straight, join_open, node_corner,
    open_endpoints, segment, smooth_handles, split_segment, transform_subpath,
};

fn v(x: f64, y: f64) -> vextor_core::Vec2 {
    vextor_core::Vec2::new(x, y)
}

fn curved(x: f64, y: f64, h_in: Option<(f64, f64)>, h_out: Option<(f64, f64)>) -> PathNode {
    PathNode {
        handle_in: h_in.map(|(a, b)| v(a, b)),
        handle_out: h_out.map(|(a, b)| v(a, b)),
        ..PathNode::corner(x, y)
    }
}

/// An open polyline (0,0) → (100,0) → (100,100).
fn elbow() -> SubPath {
    SubPath {
        nodes: vec![
            PathNode::corner(0.0, 0.0),
            PathNode::corner(100.0, 0.0),
            PathNode::corner(100.0, 100.0),
        ],
        closed: false,
    }
}

fn path(sp: SubPath) -> Path {
    Path {
        subpaths: vec![sp],
        fill_rule: FillRule::NonZero,
    }
}

/// Points sampled along every segment of `sp`.
fn samples(sp: &SubPath) -> Vec<Point> {
    (0..sp.segment_count())
        .filter_map(|i| segment(sp, i))
        .flat_map(|c| (0..=16).map(move |k| c.eval(f64::from(k) / 16.0)))
        .collect()
}

/// Distance from `p` to the nearest point of `sp`, found precisely (a
/// straight segment is its line).
fn distance_to(sp: &SubPath, p: Point) -> f64 {
    (0..sp.segment_count())
        .filter_map(|i| {
            let c = segment(sp, i)?;
            let d2 = if is_straight(sp, i) {
                Line::new(c.p0, c.p3).nearest(p, 1e-12).distance_sq
            } else {
                c.nearest(p, 1e-12).distance_sq
            };
            Some(d2.sqrt())
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn segment_hit_reports_the_parameter_under_a_rotated_transform() {
    let world = Affine::translate((50.0, 0.0)) * Affine::rotate(std::f64::consts::FRAC_PI_2);
    // The first segment (0,0)→(100,0) lands on (50,0)→(50,100).
    let hit = hit_segment(&path(elbow()), world, Point::new(52.0, 25.0), 4.0).expect("hit");
    assert_eq!((hit.subpath, hit.index), (0, 0));
    assert!((hit.t - 0.25).abs() < 1e-3, "t = {}", hit.t);
    assert!((hit.point - Point::new(50.0, 25.0)).hypot() < 1e-3);
    assert!(hit_segment(&path(elbow()), world, Point::new(60.0, 25.0), 4.0).is_none());
}

#[test]
fn node_hit_picks_the_nearest_anchor() {
    let p = path(elbow());
    let scaled = Affine::scale(2.0);
    assert_eq!(
        hit_node(&p, scaled, Point::new(198.0, 3.0), 5.0),
        Some((0, 1))
    );
    assert_eq!(hit_node(&p, scaled, Point::new(100.0, 0.0), 5.0), None);
}

#[test]
fn open_endpoints_skip_closed_subpaths() {
    let mut closed = elbow();
    closed.closed = true;
    let single = SubPath {
        nodes: vec![PathNode::corner(5.0, 5.0)],
        closed: false,
    };
    let p = Path {
        subpaths: vec![elbow(), closed, single],
        fill_rule: FillRule::NonZero,
    };
    assert_eq!(open_endpoints(&p), vec![(0, 0), (0, 2), (2, 0)]);
}

#[test]
fn splitting_a_straight_segment_adds_a_corner() {
    let mut sp = elbow();
    assert_eq!(split_segment(&mut sp, 0, 0.25), Some(1));
    assert_eq!(sp.nodes[1], PathNode::corner(25.0, 0.0));
    assert_eq!(sp.nodes.len(), 4);
    assert_eq!(split_segment(&mut sp, 0, 0.0), None, "never at an end");
    assert_eq!(split_segment(&mut sp, 9, 0.5), None);
}

#[test]
fn splitting_a_curve_adds_a_smooth_node_on_it() {
    let mut sp = SubPath {
        nodes: vec![
            curved(0.0, 0.0, None, Some((30.0, -40.0))),
            curved(100.0, 0.0, Some((-30.0, -40.0)), None),
        ],
        closed: false,
    };
    let before = segment(&sp, 0).expect("segment");
    let i = split_segment(&mut sp, 0, 0.5).expect("split");
    let mid = sp.nodes[i];
    assert_eq!(mid.kind, NodeType::Smooth);
    assert!(mid.satisfies_type());
    assert!((Point::new(mid.anchor.x, mid.anchor.y) - before.eval(0.5)).hypot() < EPS);
}

#[test]
fn splitting_the_closing_segment_appends_the_node() {
    let mut sp = elbow();
    sp.closed = true;
    assert_eq!(split_segment(&mut sp, 2, 0.5), Some(3));
    assert_eq!(sp.nodes[3], PathNode::corner(50.0, 50.0));
}

#[test]
fn bending_a_straight_segment_moves_its_grabbed_point_with_the_pointer() {
    let sp = elbow();
    let (out, inn) = bend(&sp, 0, 0.5, Vec2::new(0.0, 30.0)).expect("bend");
    let mut bent = sp.clone();
    bent.nodes[0].handle_out = Some(out);
    bent.nodes[1].handle_in = Some(inn);
    let c = segment(&bent, 0).expect("segment");
    assert!((c.eval(0.5) - Point::new(50.0, 30.0)).hypot() < 1e-9);
    let (out, inn) = bend(&sp, 0, 0.5, Vec2::ZERO).expect("bend");
    assert!((out.x - 100.0 / 3.0).abs() < 1e-9 && out.y.abs() < 1e-9);
    assert!((inn.x + 100.0 / 3.0).abs() < 1e-9 && inn.y.abs() < 1e-9);
}

#[test]
fn smooth_handles_follow_the_neighbours() {
    let sp = elbow();
    let (h_in, h_out) = smooth_handles(&sp, 1);
    let (h_in, h_out) = (h_in.expect("in"), h_out.expect("out"));
    // Along (100,100) − (0,0), a third of each neighbour's distance.
    let third = 100.0 / 3.0;
    let k = std::f64::consts::FRAC_1_SQRT_2 * third;
    assert!((h_in.x + k).abs() < 1e-9 && (h_in.y + k).abs() < 1e-9);
    assert!((h_out.x - k).abs() < 1e-9 && (h_out.y - k).abs() < 1e-9);
    let (start_in, start_out) = smooth_handles(&sp, 0);
    assert_eq!(start_in, None, "an open end has one neighbour");
    assert_eq!(start_out, Some(v(third, 0.0)));
}

#[test]
fn angle_steps_snap_and_keep_the_projected_length() {
    let p = constrain_angle(Point::ZERO, Point::new(100.0, 10.0), 45.0);
    assert!((p - Point::new(100.0, 0.0)).hypot() < 1e-9);
    let p = constrain_angle(Point::new(10.0, 10.0), Point::new(60.0, 55.0), 45.0);
    assert!(((p.x - 10.0) - (p.y - 10.0)).abs() < 1e-9, "45°: {p:?}");
    let free = constrain_angle(Point::ZERO, Point::new(3.0, 1.0), 0.0);
    assert_eq!(free, Point::new(3.0, 1.0));
}

#[test]
fn joining_merges_coincident_ends_or_adds_a_segment() {
    let a = SubPath {
        nodes: vec![PathNode::corner(0.0, 0.0), PathNode::corner(10.0, 0.0)],
        closed: false,
    };
    let b = SubPath {
        nodes: vec![PathNode::corner(10.0, 0.0), PathNode::corner(10.0, 10.0)],
        closed: false,
    };
    assert_eq!(join_open(a.clone(), b.clone()).nodes.len(), 3, "merged");
    let far = transform_subpath(&b, Affine::translate((5.0, 0.0)));
    assert_eq!(far.nodes[0].anchor, vextor_core::Point::new(15.0, 0.0));
    assert_eq!(join_open(a, far).nodes.len(), 4, "a segment between");
}

#[test]
fn transforming_a_subpath_turns_its_handles() {
    let sp = SubPath {
        nodes: vec![curved(10.0, 0.0, None, Some((5.0, 0.0)))],
        closed: false,
    };
    let turned = transform_subpath(&sp, Affine::rotate(std::f64::consts::FRAC_PI_2));
    let h = turned.nodes[0].handle_out.expect("handle");
    assert!(h.x.abs() < 1e-9 && (h.y - 5.0).abs() < 1e-9);
    assert!((turned.nodes[0].anchor.y - 10.0).abs() < 1e-9);
}

#[test]
fn a_right_angle_corner_rounds_from_its_widget() {
    let sp = elbow();
    let c = node_corner(&sp, 1).expect("roundable");
    assert!((c.max_radius - 50.0).abs() < 1e-9, "half the shorter side");
    let mid = c.arc_middle(20.0);
    assert!((c.radius_at(mid) - 20.0).abs() < 1e-9, "round trip");
    assert!(
        c.radius_at(Point::new(150.0, -50.0)) <= 0.0 + 1e-9,
        "outside: none"
    );
    assert!(node_corner(&sp, 0).is_none(), "an open end");
    // spec 11: a corner next to a curve rounds too.
    let mut curvy = elbow();
    curvy.nodes[0].handle_out = Some(v(10.0, 10.0));
    assert!(node_corner(&curvy, 1).is_some(), "next to a curve");
}

fn arb_node() -> impl Strategy<Value = PathNode> {
    let coord = -200.0..200.0f64;
    let handle = prop::option::of((-80.0..80.0f64, -80.0..80.0f64));
    (coord.clone(), coord, handle.clone(), handle).prop_map(|(x, y, a, b)| curved(x, y, a, b))
}

fn arb_subpath() -> impl Strategy<Value = SubPath> {
    (prop::collection::vec(arb_node(), 2..6), any::<bool>())
        .prop_map(|(nodes, closed)| SubPath { nodes, closed })
}

proptest! {
    #[test]
    fn inserting_a_node_keeps_the_outline(
        sp in arb_subpath(),
        pick in 0usize..8,
        t in 0.05..0.95f64,
    ) {
        let index = pick % sp.segment_count();
        let mut split = sp.clone();
        let i = split_segment(&mut split, index, t).expect("split inside a segment");
        prop_assert_eq!(split.nodes.len(), sp.nodes.len() + 1);
        prop_assert_eq!(i, index + 1);
        // Every point of each outline lies on the other.
        for p in samples(&split) {
            prop_assert!(distance_to(&sp, p) < EPS, "{p:?} off the original");
        }
        for p in samples(&sp) {
            prop_assert!(distance_to(&split, p) < EPS, "{p:?} off the split");
        }
    }
}
