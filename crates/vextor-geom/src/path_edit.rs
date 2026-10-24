//! Path editing geometry (spec 07): which node or segment is under the
//! pointer, splitting a segment without changing its outline, bending a
//! segment through the pointer, handles for a smooth node, angle steps,
//! joining open subpaths and the round widget of a corner node.
//!
//! Paths are in their shape's local space; hit-tests take the shape's
//! world affine and a point in document space.

use kurbo::{Affine, CubicBez, Line, ParamCurve, ParamCurveNearest, Point, Vec2};
use vextor_core::{EPS, NodeType, Path, PathNode, SubPath};

use crate::convert::to_kurbo;
use crate::fillet::node_angle;

/// Accuracy of the nearest-point search on a segment, in document px.
const NEAREST_ACCURACY: f64 = 1e-3;
/// How far from its ends a bend may grab a segment (as `t`): grabbing at
/// an end would need endless handles.
const BEND_T: std::ops::RangeInclusive<f64> = 0.15..=0.85;

fn to_vec(v: vextor_core::Vec2) -> Vec2 {
    Vec2::new(v.x, v.y)
}

/// A `kurbo` vector as a model vector.
#[must_use]
pub fn from_vec(v: Vec2) -> vextor_core::Vec2 {
    vextor_core::Vec2::new(v.x, v.y)
}

/// A `kurbo` point as a model point.
#[must_use]
pub fn from_point(p: Point) -> vextor_core::Point {
    vextor_core::Point::new(p.x, p.y)
}

/// `v` as a handle: `None` when it is too short to be one.
#[must_use]
pub fn handle_or_none(v: Vec2) -> Option<vextor_core::Vec2> {
    (v.hypot() > EPS).then(|| from_vec(v))
}

/// The node after `index`, wrapping round a closed subpath.
fn next(sp: &SubPath, index: usize) -> usize {
    (index + 1) % sp.nodes.len()
}

/// True when segment `index` (from node `index` to the next) has no
/// handles on either end.
#[must_use]
pub fn is_straight(sp: &SubPath, index: usize) -> bool {
    index < sp.segment_count()
        && sp.nodes[index].handle_out.is_none()
        && sp.nodes[next(sp, index)].handle_in.is_none()
}

/// Segment `index` as a cubic in local space; a missing handle puts its
/// control point on the anchor. `None` past the last segment. A straight
/// segment's `t` runs evenly along the line from `p0` to `p3`, not along
/// this cubic (see [`is_straight`]).
#[must_use]
pub fn segment(sp: &SubPath, index: usize) -> Option<CubicBez> {
    if index >= sp.segment_count() {
        return None;
    }
    let (a, b) = (&sp.nodes[index], &sp.nodes[next(sp, index)]);
    let (p0, p3) = (to_kurbo(a.anchor), to_kurbo(b.anchor));
    Some(CubicBez::new(
        p0,
        p0 + a.handle_out.map_or(Vec2::ZERO, to_vec),
        p3 + b.handle_in.map_or(Vec2::ZERO, to_vec),
        p3,
    ))
}

/// A point on a segment found by [`hit_segment`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentHit {
    pub subpath: usize,
    /// The segment runs from node `index` to the next one.
    pub index: usize,
    /// Where on the segment, 0 at its start and 1 at its end.
    pub t: f64,
    /// The point on the segment, in document space.
    pub point: Point,
    /// Distance from the probe, in document px.
    pub distance: f64,
}

/// The segment of `path` (drawn through `world`) nearest `p` (document
/// space), when within `tolerance` document px.
#[must_use]
pub fn hit_segment(path: &Path, world: Affine, p: Point, tolerance: f64) -> Option<SegmentHit> {
    let mut best: Option<SegmentHit> = None;
    for (s, sp) in path.subpaths.iter().enumerate() {
        for i in 0..sp.segment_count() {
            let Some(c) = segment(sp, i) else {
                continue;
            };
            let c = world * c;
            // A straight segment is drawn as a line: `t` runs evenly on it.
            let (t, point) = if is_straight(sp, i) {
                let line = Line::new(c.p0, c.p3);
                let t = line.nearest(p, NEAREST_ACCURACY).t;
                (t, line.eval(t))
            } else {
                let t = c.nearest(p, NEAREST_ACCURACY).t;
                (t, c.eval(t))
            };
            let distance = (point - p).hypot();
            if distance <= tolerance && best.is_none_or(|b| distance < b.distance) {
                best = Some(SegmentHit {
                    subpath: s,
                    index: i,
                    t,
                    point,
                    distance,
                });
            }
        }
    }
    best
}

/// The node of `path` (drawn through `world`) whose anchor is nearest `p`
/// (document space) within `tolerance`, as `(subpath, index)`.
#[must_use]
pub fn hit_node(path: &Path, world: Affine, p: Point, tolerance: f64) -> Option<(usize, usize)> {
    let mut best: Option<((usize, usize), f64)> = None;
    for (s, sp) in path.subpaths.iter().enumerate() {
        for (i, n) in sp.nodes.iter().enumerate() {
            let d = (world * to_kurbo(n.anchor) - p).hypot();
            if d <= tolerance && best.is_none_or(|(_, b)| d < b) {
                best = Some(((s, i), d));
            }
        }
    }
    best.map(|(at, _)| at)
}

/// The open ends of `path`'s subpaths, as `(subpath, index)`: both ends
/// of each open subpath (once for a single node).
#[must_use]
pub fn open_endpoints(path: &Path) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (s, sp) in path.subpaths.iter().enumerate() {
        if sp.closed {
            continue;
        }
        out.push((s, 0));
        if sp.nodes.len() > 1 {
            out.push((s, sp.nodes.len() - 1));
        }
    }
    out
}

/// Inserts a node at `t` on segment `index` without changing the outline
/// (de Casteljau). A straight segment gets a corner node; a curve a smooth
/// one, and its neighbours' handles shrink to match. Returns the new
/// node's index, or `None` when `t` is at an end or there is no segment.
pub fn split_segment(sp: &mut SubPath, index: usize, t: f64) -> Option<usize> {
    let c = segment(sp, index)?;
    if !(t > EPS && t < 1.0 - EPS) {
        return None;
    }
    let node = if is_straight(sp, index) {
        let p = c.p0.lerp(c.p3, t);
        PathNode::corner(p.x, p.y)
    } else {
        let p = c.eval(t);
        let (left, right) = (c.subsegment(0.0..t), c.subsegment(t..1.0));
        let j = next(sp, index);
        sp.nodes[index].handle_out = handle_or_none(left.p1 - left.p0);
        sp.nodes[j].handle_in = handle_or_none(right.p2 - right.p3);
        let handle_in = handle_or_none(left.p2 - left.p3);
        let handle_out = handle_or_none(right.p1 - right.p0);
        let kind = if handle_in.is_some() && handle_out.is_some() {
            NodeType::Smooth
        } else {
            NodeType::Corner
        };
        PathNode {
            handle_in,
            handle_out,
            kind,
            ..PathNode::corner(p.x, p.y)
        }
    };
    sp.nodes.insert(index + 1, node);
    Some(index + 1)
}

/// The handles that bend segment `index` so that its point at `t` moves by
/// `delta` (local space): the start node's new `handle_out` and the end
/// node's new `handle_in`. A straight segment bends from handles a third
/// of the way along it, so it starts from its own shape.
#[must_use]
pub fn bend(
    sp: &SubPath,
    index: usize,
    t: f64,
    delta: Vec2,
) -> Option<(vextor_core::Vec2, vextor_core::Vec2)> {
    let c = segment(sp, index)?;
    let (p1, p2) = if is_straight(sp, index) {
        (c.p0 + (c.p3 - c.p0) / 3.0, c.p3 + (c.p0 - c.p3) / 3.0)
    } else {
        (c.p1, c.p2)
    };
    let t = t.clamp(*BEND_T.start(), *BEND_T.end());
    // Moving both control points by `d` moves B(t) by 3t(1−t)·d.
    let d = delta / (3.0 * t * (1.0 - t));
    Some((from_vec(p1 + d - c.p0), from_vec(p2 + d - c.p3)))
}

/// Handles for making node `index` smooth: along the line through its
/// neighbours, each a third of the way to its neighbour. An open end gets
/// only the handle towards its one neighbour.
#[must_use]
pub fn smooth_handles(
    sp: &SubPath,
    index: usize,
) -> (Option<vextor_core::Vec2>, Option<vextor_core::Vec2>) {
    let n = sp.nodes.len();
    if n < 2 || index >= n {
        return (None, None);
    }
    let at = to_kurbo(sp.nodes[index].anchor);
    let prev = (sp.closed || index > 0).then(|| to_kurbo(sp.nodes[(index + n - 1) % n].anchor));
    let after = (sp.closed || index + 1 < n).then(|| to_kurbo(sp.nodes[(index + 1) % n].anchor));
    let dir = match (prev, after) {
        (Some(p), Some(q)) => q - p,
        (Some(p), None) => at - p,
        (None, Some(q)) => q - at,
        (None, None) => return (None, None),
    };
    let Some(u) = (dir.hypot() > EPS).then(|| dir / dir.hypot()) else {
        return (None, None);
    };
    let handle_in = prev.and_then(|p| handle_or_none(-u * ((at - p).hypot() / 3.0)));
    let handle_out = after.and_then(|q| handle_or_none(u * ((q - at).hypot() / 3.0)));
    (handle_in, handle_out)
}

/// `v` turned to the nearest multiple of `step_deg` degrees, keeping its
/// length along that direction. A step of zero or less leaves it as is.
#[must_use]
pub fn constrain_vector(v: Vec2, step_deg: f64) -> Vec2 {
    if step_deg <= EPS || v.hypot() <= EPS {
        return v;
    }
    let step = step_deg.to_radians();
    let angle = (v.y.atan2(v.x) / step).round() * step;
    let dir = Vec2::new(angle.cos(), angle.sin());
    dir * v.dot(dir).max(0.0)
}

/// `to`, moved so the line from `from` runs at a multiple of `step_deg`.
#[must_use]
pub fn constrain_angle(from: Point, to: Point, step_deg: f64) -> Point {
    from + constrain_vector(to - from, step_deg)
}

/// `sp` moved through `a`: anchors mapped, handles turned and scaled with
/// them.
#[must_use]
pub fn transform_subpath(sp: &SubPath, a: Affine) -> SubPath {
    let linear = |v: vextor_core::Vec2| from_vec(a * (Point::ZERO + to_vec(v)) - a * Point::ZERO);
    SubPath {
        nodes: sp
            .nodes
            .iter()
            .map(|n| PathNode {
                anchor: from_point(a * to_kurbo(n.anchor)),
                handle_in: n.handle_in.map(linear),
                handle_out: n.handle_out.map(linear),
                ..*n
            })
            .collect(),
        closed: sp.closed,
    }
}

/// Two open subpaths as one: `first`'s last node joins `second`'s first.
/// When those two anchors coincide they become one node (with `first`'s
/// incoming and `second`'s outgoing handle); otherwise a straight segment
/// runs between them.
#[must_use]
pub fn join_open(mut first: SubPath, mut second: SubPath) -> SubPath {
    if let (Some(tail), Some(head)) = (first.nodes.last_mut(), second.nodes.first())
        && (to_kurbo(tail.anchor) - to_kurbo(head.anchor)).hypot() <= EPS
    {
        tail.handle_out = head.handle_out;
        tail.enforce_balanced();
        second.nodes.remove(0);
    }
    first.nodes.append(&mut second.nodes);
    first.closed = false;
    first
}

/// A node that can be rounded: an interior node with a real angle between
/// its segments, straight or curved (local space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeCorner {
    pub vertex: Point,
    /// Unit vector from the vertex into the corner, halving its angle
    /// (between the segments' tangents at the vertex).
    pub bisector: Vec2,
    /// Half the angle between the two segments, in radians.
    pub half_angle: f64,
    /// The largest radius that fits (the fillet uses at most half of
    /// each segment, measured along it).
    pub max_radius: f64,
}

/// The corner at node `index`, when it can be rounded.
#[must_use]
pub fn node_corner(sp: &SubPath, index: usize) -> Option<NodeCorner> {
    let c = node_angle(sp, index)?;
    let sum = c.u_in + c.u_out;
    let half_angle = c.angle / 2.0;
    Some(NodeCorner {
        vertex: to_kurbo(sp.nodes[index].anchor),
        bisector: sum / sum.hypot(),
        half_angle,
        max_radius: c.len_in.min(c.len_out) / 2.0 * half_angle.tan(),
    })
}

impl NodeCorner {
    /// The middle of the rounding arc of `radius` (capped to fit), on the
    /// bisector.
    #[must_use]
    pub fn arc_middle(&self, radius: f64) -> Point {
        let r = radius.clamp(0.0, self.max_radius);
        self.vertex + self.bisector * (r / self.half_angle.sin() - r)
    }

    /// The radius whose arc middle is nearest `p`, from 0 to the largest
    /// that fits.
    #[must_use]
    pub fn radius_at(&self, p: Point) -> f64 {
        let along = (p - self.vertex).dot(self.bisector).max(0.0);
        let k = 1.0 / self.half_angle.sin() - 1.0;
        if k <= EPS {
            return self.max_radius;
        }
        (along / k).min(self.max_radius)
    }
}
