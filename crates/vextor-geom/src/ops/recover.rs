//! Curve recovery: the overlay engine works on polygons, so each edge of
//! its result is matched back to the input segment it lies on, and runs of
//! edges along one segment become that segment's own sub-curve. A circle
//! stays four cubics instead of hundreds of lines; only new intersection
//! points are approximate (within `polygon::FLATTEN`).

use kurbo::{BezPath, ParamCurve, ParamCurveNearest, PathSeg, Point, Rect, Shape};

use super::polygon::{Shapes, closed};

/// How far a result point may be from a segment to count as on it, in
/// document px: above the flattening tolerance, far below anything drawn.
const ON_SEGMENT: f64 = 0.02;
/// Accuracy of the nearest-point search on curves.
const NEAREST: f64 = 1e-9;
/// A result point this close to a segment is one of its own points.
const EXACT: f64 = 1e-7;
/// Two parameters closer than this are the same point of a segment.
const SAME_T: f64 = 1e-6;

/// The input segments results are matched against.
#[derive(Clone, Debug, Default)]
pub(super) struct Sources {
    segments: Vec<PathSeg>,
    /// Each segment's bounding box grown by [`ON_SEGMENT`].
    boxes: Vec<Rect>,
}

/// Where a result edge runs: along `segment` from `t0` to `t1`.
#[derive(Clone, Copy, Debug)]
struct Along {
    segment: usize,
    t0: f64,
    t1: f64,
}

impl Sources {
    /// The segments of `path`, its open subpaths closed.
    pub(super) fn of(path: &BezPath) -> Self {
        let mut s = Self::default();
        for seg in closed(path).segments() {
            s.boxes
                .push(seg.bounding_box().inflate(ON_SEGMENT, ON_SEGMENT));
            s.segments.push(seg);
        }
        s
    }

    /// Adds the segments of `other`.
    pub(super) fn extend(&mut self, other: &Self) {
        self.segments.extend_from_slice(&other.segments);
        self.boxes.extend_from_slice(&other.boxes);
    }

    /// The parameter of `p` on segment `i` and its distance from it, when
    /// `p` lies on it.
    fn t_on(&self, i: usize, p: Point) -> Option<(f64, f64)> {
        let near = self.segments[i].nearest(p, NEAREST);
        (near.distance_sq <= ON_SEGMENT * ON_SEGMENT).then_some((near.t, near.distance_sq.sqrt()))
    }

    /// How edge `a`–`b` runs along segment `i` when both ends and its
    /// middle lie on it, with the larger distance of its ends.
    fn along(&self, i: usize, a: Point, b: Point) -> Option<(Along, f64)> {
        let r = self.boxes[i];
        if !(r.contains(a) && r.contains(b)) {
            return None;
        }
        let (t0, da) = self.t_on(i, a)?;
        let (t1, db) = self.t_on(i, b)?;
        self.t_on(i, a.midpoint(b))?;
        Some((Along { segment: i, t0, t1 }, da.max(db)))
    }

    /// The segment the edge `a`–`b` lies on: the one its ends lie closest
    /// to (near an intersection a curve's points can also be close to the
    /// line it crosses). `hint` and its neighbours are tried first and
    /// win outright when the ends lie exactly on one.
    fn find(&self, a: Point, b: Point, hint: usize) -> Option<Along> {
        let n = self.segments.len();
        if n == 0 {
            return None;
        }
        let near = [hint % n, (hint + 1) % n, (hint + n - 1) % n];
        if let Some((along, _)) = near
            .into_iter()
            .filter_map(|i| self.along(i, a, b))
            .find(|(_, d)| *d <= EXACT)
        {
            return Some(along);
        }
        (0..n)
            .filter_map(|i| self.along(i, a, b))
            .min_by(|x, y| x.1.total_cmp(&y.1))
            .map(|(along, _)| along)
    }

    /// `shapes` as a Bézier path with the input curves recovered.
    pub(super) fn to_bezpath(&self, shapes: &Shapes) -> BezPath {
        let mut out = BezPath::new();
        for ring in shapes.iter().flatten() {
            self.ring(ring, &mut out);
        }
        out
    }

    fn ring(&self, ring: &[[f64; 2]], out: &mut BezPath) {
        let pts: Vec<Point> = ring.iter().map(|p| Point::new(p[0], p[1])).collect();
        let n = pts.len();
        if n < 3 {
            return;
        }
        let mut hint = 0;
        let edges: Vec<Option<Along>> = (0..n)
            .map(|k| {
                let along = self.find(pts[k], pts[(k + 1) % n], hint);
                if let Some(a) = along {
                    hint = a.segment;
                }
                along
            })
            .collect();
        let continues = |k: usize| match (edges[(k + n - 1) % n], edges[k]) {
            (Some(prev), Some(e)) => {
                prev.segment == e.segment
                    && (prev.t1 - e.t0).abs() < SAME_T
                    && (prev.t1 - prev.t0) * (e.t1 - e.t0) >= 0.0
            }
            _ => false,
        };
        // Start where a run starts, so no run is cut in two at the seam.
        let start = (0..n).find(|&k| !continues(k)).unwrap_or(0);
        out.move_to(pts[start]);
        let mut run: Option<Along> = None;
        for step in 0..n {
            let k = (start + step) % n;
            let end = pts[(k + 1) % n];
            run = match (run, edges[k]) {
                (Some(r), Some(e)) if continues(k) => Some(Along { t1: e.t1, ..r }),
                (open, e) => {
                    if let Some(r) = open {
                        self.emit(r, pts[k], out);
                    }
                    if e.is_none() {
                        out.line_to(end);
                    }
                    e
                }
            };
        }
        if let Some(r) = run {
            self.emit(r, pts[start], out);
        }
        out.close_path();
    }

    /// Draws `run` up to `end`: the matched segment's piece between its
    /// parameters, ending exactly on the result's point.
    fn emit(&self, run: Along, end: Point, out: &mut BezPath) {
        if (run.t1 - run.t0).abs() < SAME_T {
            out.line_to(end);
            return;
        }
        match self.segments[run.segment].subsegment(run.t0..run.t1) {
            PathSeg::Line(_) => out.line_to(end),
            PathSeg::Quad(q) => out.quad_to(q.p1, end),
            PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, end),
        }
    }
}
