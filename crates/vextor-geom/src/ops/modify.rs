//! Operations on one outline: offset (inset/outset), outline stroke and
//! simplify. All work in the outline's own space.

use kurbo::simplify::{SimplifyOptions, simplify_bezpath};
use kurbo::{BezPath, Cap, Join, Line, ParamCurve, PathEl, PathSeg, Point, StrokeOpts};
use vextor_core::{EPS, FillRule, LineCap, LineJoin, Stroke};

use super::{Area, BooleanOp};
use crate::ops::polygon::closed;

/// How far stroke and offset curves may stray from the exact ones, in
/// document px.
const STROKE_TOLERANCE: f64 = 0.01;
/// Share of the simplify tolerance spent on thinning straight runs; the
/// rest goes to refitting curves, so together they stay within it.
const THIN_SHARE: f64 = 0.5;

fn join(j: LineJoin) -> Join {
    match j {
        LineJoin::Miter => Join::Miter,
        LineJoin::Round => Join::Round,
        LineJoin::Bevel => Join::Bevel,
    }
}

/// The area `outline` fills under `fill_rule`, grown by `distance` (shrunk
/// when negative), its corners joined by `join` (miters longer than
/// `miter_limit` × the distance become bevels, as in SVG). Open subpaths
/// count as closed. Empty when an inset swallows everything.
#[must_use]
pub fn offset(
    outline: &BezPath,
    fill_rule: FillRule,
    distance: f64,
    join_kind: LineJoin,
    miter_limit: f64,
) -> BezPath {
    let area = Area::new(outline, fill_rule);
    if distance.abs() <= EPS || !distance.is_finite() {
        return area.to_bezpath();
    }
    let style = kurbo::Stroke::new(distance.abs() * 2.0)
        .with_join(join(join_kind))
        .with_miter_limit(miter_limit.max(1.0));
    let band = kurbo::stroke(
        closed(outline),
        &style,
        &StrokeOpts::default(),
        STROKE_TOLERANCE,
    );
    let band = Area::new(&band, FillRule::NonZero);
    let op = if distance > 0.0 {
        BooleanOp::Union
    } else {
        BooleanOp::Subtract
    };
    Area::combine(op, &[area, band]).to_bezpath()
}

/// The area `stroke` paints along `outline`, as a filled outline: caps,
/// joins, miter limit and dashes as SVG draws them.
#[must_use]
pub fn outline_stroke(outline: &BezPath, stroke: &Stroke) -> BezPath {
    let cap = match stroke.cap {
        LineCap::Butt => Cap::Butt,
        LineCap::Round => Cap::Round,
        LineCap::Square => Cap::Square,
    };
    let mut style = kurbo::Stroke::new(stroke.width)
        .with_join(join(stroke.join))
        .with_miter_limit(stroke.miter_limit.max(1.0))
        .with_caps(cap);
    // SVG draws a dash list that sums to nothing as a solid line.
    if stroke.dash.iter().sum::<f64>() > EPS {
        style = style.with_dashes(stroke.dash_offset, stroke.dash.iter().copied());
    }
    let painted = kurbo::stroke(
        outline.iter(),
        &style,
        &StrokeOpts::default(),
        STROKE_TOLERANCE,
    );
    Area::new(&painted, FillRule::NonZero).to_bezpath()
}

/// `path` with fewer segments, never further than `tolerance` document px
/// from it: runs of straight segments are thinned, smooth runs refitted;
/// corners stay. `path` itself when nothing could be saved.
#[must_use]
pub fn simplify(path: &BezPath, tolerance: f64) -> BezPath {
    if tolerance <= EPS || !tolerance.is_finite() {
        return path.clone();
    }
    let thinned = thin_lines(path, tolerance * THIN_SHARE);
    let fitted = simplify_bezpath(
        thinned,
        tolerance * (1.0 - THIN_SHARE),
        &SimplifyOptions::default(),
    );
    let out = straighten(&fitted);
    if out.segments().count() < path.segments().count() {
        out
    } else {
        path.clone()
    }
}

/// `path`'s subpaths: each as its own path, and whether it is closed.
fn subpaths(path: &BezPath) -> Vec<(BezPath, bool)> {
    let mut out: Vec<(BezPath, bool)> = Vec::new();
    for &el in path.elements() {
        match el {
            PathEl::MoveTo(_) => out.push((BezPath::new(), false)),
            PathEl::ClosePath => {
                if let Some(last) = out.last_mut() {
                    last.1 = true;
                }
            }
            _ => {}
        }
        if let Some(last) = out.last_mut() {
            last.0.push(el);
        }
    }
    out
}

/// `path` with each run of straight segments reduced to the corners that
/// stray more than `tolerance` from a straight line (Ramer–Douglas–Peucker).
fn thin_lines(path: &BezPath, tolerance: f64) -> BezPath {
    let mut out = BezPath::new();
    for (sub, closed) in subpaths(path) {
        let segs: Vec<PathSeg> = sub.segments().collect();
        let Some(first) = segs.first() else {
            out.extend(sub.elements().iter().copied());
            continue;
        };
        out.move_to(first.start());
        let mut run: Vec<Point> = Vec::new();
        for seg in segs {
            match seg {
                PathSeg::Line(l) => {
                    if run.is_empty() {
                        run.push(l.p0);
                    }
                    run.push(l.p1);
                }
                other => {
                    flush_run(&mut run, tolerance, &mut out);
                    match other {
                        PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
                        PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
                        PathSeg::Line(_) => {}
                    }
                }
            }
        }
        flush_run(&mut run, tolerance, &mut out);
        if closed {
            out.close_path();
        }
    }
    out
}

fn flush_run(run: &mut Vec<Point>, tolerance: f64, out: &mut BezPath) {
    if run.len() >= 2 {
        let mut keep = vec![false; run.len()];
        keep[0] = true;
        keep[run.len() - 1] = true;
        mark_corners(run, 0, run.len() - 1, tolerance, &mut keep);
        for (p, _) in run.iter().zip(&keep).skip(1).filter(|(_, k)| **k) {
            out.line_to(*p);
        }
    }
    run.clear();
}

/// Marks the points between `a` and `b` that must stay so the polyline
/// stays within `tolerance` of `pts`.
fn mark_corners(pts: &[Point], a: usize, b: usize, tolerance: f64, keep: &mut [bool]) {
    if b <= a + 1 {
        return;
    }
    let chord = Line::new(pts[a], pts[b]);
    let (far, dist) = (a + 1..b)
        .map(|i| (i, distance_to_line(chord, pts[i])))
        .fold((a, -1.0), |best, c| if c.1 > best.1 { c } else { best });
    if dist > tolerance {
        keep[far] = true;
        mark_corners(pts, a, far, tolerance, keep);
        mark_corners(pts, far, b, tolerance, keep);
    }
}

fn distance_to_line(l: Line, p: Point) -> f64 {
    let d = l.p1 - l.p0;
    let len2 = d.hypot2();
    if len2 <= EPS * EPS {
        return (p - l.p0).hypot();
    }
    let t = ((p - l.p0).dot(d) / len2).clamp(0.0, 1.0);
    (p - l.eval(t)).hypot()
}

/// `path` with cubics that are straight lines (refitted lines) as lines.
fn straighten(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    let mut last = Point::ZERO;
    let mut start = Point::ZERO;
    for &el in path.elements() {
        match el {
            PathEl::CurveTo(c1, c2, p) => {
                let chord = Line::new(last, p);
                let slack = 1e-9 * chord.length().max(1.0);
                if distance_to_line(chord, c1) <= slack && distance_to_line(chord, c2) <= slack {
                    out.line_to(p);
                } else {
                    out.push(el);
                }
                last = p;
            }
            PathEl::MoveTo(p) => {
                out.push(el);
                (last, start) = (p, p);
            }
            PathEl::LineTo(p) | PathEl::QuadTo(_, p) => {
                out.push(el);
                last = p;
            }
            PathEl::ClosePath => {
                out.push(el);
                last = start;
            }
        }
    }
    out
}
