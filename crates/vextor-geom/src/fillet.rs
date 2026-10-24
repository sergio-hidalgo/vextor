//! Rounded corners: the arc that replaces a corner of a polygon, a rect or
//! a path node (live `corner_radius`). Next to curves (spec 11) the arc is
//! fitted where the curves are cut back by arc length.

use std::f64::consts::PI;

use kurbo::{CubicBez, Line, ParamCurve, ParamCurveArclen, ParamCurveDeriv, PathSeg, Point, Vec2};
use vextor_core::{EPS, SubPath};

use crate::convert::to_kurbo;
use crate::path_edit::{is_straight, segment};

/// Accuracy of arc lengths along curves, in document px.
const ARCLEN_ACCURACY: f64 = 1e-6;

/// A rounded corner: the curve runs from `a` (on the segment coming in,
/// at parameter `t_in`) to `b` (on the segment going out, at `t_out`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Fillet {
    pub a: Point,
    pub c1: Point,
    pub c2: Point,
    pub b: Point,
    pub t_in: f64,
    pub t_out: f64,
}

/// The fillet of radius `radius` at `vertex` between straight edges to
/// `prev` and `next`, shrunk so it uses at most half of each edge; `None`
/// when there is nothing to round.
pub(crate) fn fillet(prev: Point, vertex: Point, next: Point, radius: f64) -> Option<Fillet> {
    if radius <= EPS {
        return None;
    }
    let (d1, d2) = (prev - vertex, next - vertex);
    let (l1, l2) = (d1.hypot(), d2.hypot());
    if l1 <= EPS || l2 <= EPS {
        return None;
    }
    let (u1, u2) = (d1 / l1, d2 / l2);
    let angle = u1.dot(u2).clamp(-1.0, 1.0).acos(); // interior angle at the vertex
    if angle <= EPS || PI - angle <= EPS {
        return None; // folded back or straight through
    }
    let half_tan = (angle / 2.0).tan();
    let tangent = (radius / half_tan).min(l1.min(l2) / 2.0);
    let fitted = tangent * half_tan;
    let k = 4.0 / 3.0 * ((PI - angle) / 4.0).tan() * fitted;
    let (a, b) = (vertex + u1 * tangent, vertex + u2 * tangent);
    Some(Fillet {
        a,
        c1: a - u1 * k,
        c2: b - u2 * k,
        b,
        t_in: 1.0 - tangent / l1,
        t_out: tangent / l2,
    })
}

/// Segment `index` of `sp` as drawn: a line when it has no handles.
pub(crate) fn drawn_segment(sp: &SubPath, index: usize) -> Option<PathSeg> {
    let c = segment(sp, index)?;
    Some(if is_straight(sp, index) {
        PathSeg::Line(Line::new(c.p0, c.p3))
    } else {
        PathSeg::Cubic(c)
    })
}

/// The corner at node `index`: the segments in and out, unit vectors from
/// the node along each (back along the one coming in), and their lengths.
/// `None` for an end node or a node without a real angle.
pub(crate) struct NodeAngle {
    pub seg_in: PathSeg,
    pub seg_out: PathSeg,
    pub u_in: Vec2,
    pub u_out: Vec2,
    /// Angle between `u_in` and `u_out`, in radians (π: straight through).
    pub angle: f64,
    pub len_in: f64,
    pub len_out: f64,
}

/// See [`NodeAngle`].
pub(crate) fn node_angle(sp: &SubPath, index: usize) -> Option<NodeAngle> {
    let n = sp.nodes.len();
    let interior = sp.closed || (index > 0 && index + 1 < n);
    if n < 2 || index >= n || !interior || sp.segment_count() < 2 {
        return None;
    }
    let seg_in = drawn_segment(sp, (index + n - 1) % n)?;
    let seg_out = drawn_segment(sp, index)?;
    let u_in = -end_tangents(seg_in).1.normalize();
    let u_out = end_tangents(seg_out).0.normalize();
    if !(u_in.is_finite() && u_out.is_finite()) {
        return None;
    }
    let angle = u_in.dot(u_out).clamp(-1.0, 1.0).acos();
    if angle <= EPS || PI - angle <= EPS {
        return None;
    }
    let (len_in, len_out) = (
        seg_in.arclen(ARCLEN_ACCURACY),
        seg_out.arclen(ARCLEN_ACCURACY),
    );
    (len_in > EPS && len_out > EPS).then_some(NodeAngle {
        seg_in,
        seg_out,
        u_in,
        u_out,
        angle,
        len_in,
        len_out,
    })
}

/// The fillet of node `index`'s `corner_radius`, between straight
/// segments or curves; `None` when there is nothing to round.
pub(crate) fn node_fillet(sp: &SubPath, index: usize) -> Option<Fillet> {
    let radius = sp.nodes.get(index)?.corner_radius;
    if radius <= EPS {
        return None;
    }
    let count = sp.nodes.len();
    let prev = (index + count - 1) % count;
    if count >= 3 && is_straight(sp, prev) && is_straight(sp, index) {
        let anchor = |i: usize| to_kurbo(sp.nodes[i].anchor);
        let interior = sp.closed || (index > 0 && index + 1 < count);
        return interior
            .then(|| {
                let next = (index + 1) % count;
                fillet(anchor(prev), anchor(index), anchor(next), radius)
            })
            .flatten();
    }
    let corner = node_angle(sp, index)?;
    let tangent = (radius / (corner.angle / 2.0).tan())
        .min(corner.len_in / 2.0)
        .min(corner.len_out / 2.0);
    if tangent <= EPS {
        return None;
    }
    let t_in = corner
        .seg_in
        .inv_arclen(corner.len_in - tangent, ARCLEN_ACCURACY);
    let t_out = corner.seg_out.inv_arclen(tangent, ARCLEN_ACCURACY);
    let (from, to) = (corner.seg_in.eval(t_in), corner.seg_out.eval(t_out));
    let leave = direction(corner.seg_in, t_in)?;
    let arrive = direction(corner.seg_out, t_out)?;
    // A circular arc leaving `from` along `leave`, arriving at `to` along
    // `arrive`.
    let turn = leave.dot(arrive).clamp(-1.0, 1.0).acos();
    let handle = if turn <= EPS {
        (to - from).hypot() / 3.0
    } else {
        let radius = (to - from).hypot() / (2.0 * (turn / 2.0).sin());
        4.0 / 3.0 * (turn / 4.0).tan() * radius
    };
    Some(Fillet {
        a: from,
        c1: from + leave * handle,
        c2: to - arrive * handle,
        b: to,
        t_in,
        t_out,
    })
}

/// The unit direction of travel along `seg` at `t`.
fn direction(seg: PathSeg, t: f64) -> Option<Vec2> {
    let d = match seg {
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => q.deriv().eval(t).to_vec2(),
        PathSeg::Cubic(c) => cubic_direction(c, t),
    };
    let d = d.normalize();
    d.is_finite().then_some(d)
}

/// A cubic's direction at `t`, falling back on its end tangents where
/// the derivative vanishes (a missing handle at an end).
fn cubic_direction(c: CubicBez, t: f64) -> Vec2 {
    let d = c.deriv().eval(t).to_vec2();
    if d.hypot() > EPS {
        return d;
    }
    let (start, end) = end_tangents(PathSeg::Cubic(c));
    if t < 0.5 { start } else { end }
}

/// The directions `seg` leaves its start and reaches its end, looking past
/// control points that sit on the anchor.
fn end_tangents(seg: PathSeg) -> (Vec2, Vec2) {
    let first = |vs: [Vec2; 3]| {
        vs.into_iter()
            .find(|v| v.hypot() > EPS)
            .unwrap_or(Vec2::ZERO)
    };
    match seg {
        PathSeg::Line(l) => (l.p1 - l.p0, l.p1 - l.p0),
        PathSeg::Quad(q) => (
            first([q.p1 - q.p0, q.p2 - q.p0, Vec2::ZERO]),
            first([q.p2 - q.p1, q.p2 - q.p0, Vec2::ZERO]),
        ),
        PathSeg::Cubic(c) => (
            first([c.p1 - c.p0, c.p2 - c.p0, c.p3 - c.p0]),
            first([c.p3 - c.p2, c.p3 - c.p1, c.p3 - c.p0]),
        ),
    }
}
