//! A pixel-grid edge (a loop, or an open line between colour areas) as a
//! smooth outline: stair steps become straight or curved lines (the midpoints of the pixel edges, as marching squares
//! gives), corners stay sharp where the outline turns sharply, and smooth
//! cubics are fitted in between (Potrace's corner / curve idea).

use kurbo::{BezPath, Line, ParamCurveNearest, Point, Shape, Vec2};
use vextor_core::EPS;

use super::outline::{Dir, Loop, step};
use crate::ops::simplify;

/// Straight runs at least this long (pixels) on both sides of a turn make
/// a sharp corner: a drawn corner, not a stair step.
const CORNER_RUN: usize = 3;
/// Rounds of [1 2 1] smoothing of the pixel edges' midpoints: takes out
/// what is left of the stair steps.
const SMOOTHING: usize = 3;
/// How far the corner polygon may stray from the smoothed midpoints.
const POLYGON_TOLERANCE: f64 = 0.5;
/// The outline turning by more than this (radians, 75°) makes a corner.
const CORNER_TURN: f64 = 1.309;
/// Below this share of its pixels' area, a fitted outline has lost a
/// thin shape: the pixel edges are kept instead.
const MIN_AREA_KEPT: f64 = 0.5;
/// `simplify`'s tolerance for the fitted curves: with the polygon's, about
/// one pixel in all.
const CURVE_TOLERANCE: f64 = 1.0;

/// `edge` as one subpath in pixel units, closed when the edge is. An open
/// edge's ends stay exactly where they are (where colour areas meet).
pub(super) fn fitted(edge: &Loop) -> BezPath {
    let runs = runs(edge);
    let (points, sharp) = midpoints(&runs);
    let points = smoothed(points, &sharp);
    let ring = polygon(&points, &sharp, runs.closed);
    if ring.len() < if runs.closed { 3 } else { 2 } {
        return straight(&run_corners(&runs), runs.closed);
    }
    let n = ring.len();
    let cos_turn = CORNER_TURN.cos();
    let corner: Vec<bool> = (0..n)
        .map(|i| {
            let (here, forced) = ring[i];
            let incoming = direction(here - ring[(i + n - 1) % n].0);
            let outgoing = direction(ring[(i + 1) % n].0 - here);
            forced || incoming.dot(outgoing) <= cos_turn
        })
        .collect();

    let at = |i: usize| ring[i % n].0;
    let mut bez = BezPath::new();
    bez.move_to(at(0));
    // an open edge's ends are corners, so its first and last segments never
    // look past them
    let segments = if runs.closed { n } else { n - 1 };
    for i in 0..segments {
        let (p0, p3) = (at(i), at(i + 1));
        let (c0, c3) = (corner[i], corner[(i + 1) % n]);
        if c0 && c3 {
            bez.line_to(p3);
            continue;
        }
        // Catmull-Rom directions through the neighbours (along the chord
        // at a corner), as long as this segment, so a short segment next
        // to a long one does not overshoot
        let chord = p3 - p0;
        let along = |d: Vec2| direction(d) * chord.hypot();
        let t0 = if c0 { chord } else { along(p3 - at(i + n - 1)) };
        let t3 = if c3 { chord } else { along(at(i + 2) - p0) };
        bez.curve_to(p0 + t0 / 3.0, p3 - t3 / 3.0, p3);
    }
    if !runs.closed {
        return simplify(&bez, CURVE_TOLERANCE);
    }
    bez.close_path();
    let out = simplify(&bez, CURVE_TOLERANCE);
    // thinner than the smoothing (a 1 px line): the pixel edges as they are
    #[allow(clippy::cast_precision_loss)] // pixel areas, far below 2^52
    let pixels = edge.area2().unsigned_abs() as f64 / 2.0;
    if out.area().abs() < pixels * MIN_AREA_KEPT {
        return straight(&run_corners(&runs), true);
    }
    out
}

/// The polygon through `points`, closed or not.
fn straight(points: &[Point], closed: bool) -> BezPath {
    let mut bez = BezPath::new();
    for (i, &p) in points.iter().enumerate() {
        if i == 0 {
            bez.move_to(p);
        } else {
            bez.line_to(p);
        }
    }
    if closed {
        bez.close_path();
    }
    bez
}

/// `v` as a unit vector; zero when it has no length (never NaN).
fn direction(v: Vec2) -> Vec2 {
    let len = v.hypot();
    if len > EPS { v / len } else { Vec2::ZERO }
}

/// `edge`'s steps as runs (direction, length), starting at a turn, and where
/// the first run starts.
struct Stairs {
    start: Point,
    runs: Vec<(Dir, usize)>,
    closed: bool,
}

fn runs(edge: &Loop) -> Stairs {
    let steps = &edge.steps;
    let n = steps.len();
    // a loop always turns: start where it does, so no run wraps round; an
    // open edge starts at its start
    let first = if edge.closed {
        (0..n)
            .find(|&i| steps[i] != steps[(i + n - 1) % n])
            .unwrap_or(0)
    } else {
        0
    };
    let (mut x, mut y) = edge.start;
    for &dir in &steps[..first] {
        let (dx, dy) = step(dir);
        (x, y) = (x + dx, y + dy);
    }
    let mut runs: Vec<(Dir, usize)> = Vec::new();
    for i in 0..n {
        let dir = steps[(first + i) % n];
        match runs.last_mut() {
            Some(run) if run.0 == dir => run.1 += 1,
            _ => runs.push((dir, 1)),
        }
    }
    #[allow(clippy::cast_precision_loss)] // pixel corners, far below 2^52
    let start = Point::new(x as f64, y as f64);
    Stairs {
        start,
        runs,
        closed: edge.closed,
    }
}

fn unit(d: Dir) -> Vec2 {
    let (dx, dy) = step(d);
    #[allow(clippy::cast_precision_loss)] // −1, 0 or 1
    Vec2::new(dx as f64, dy as f64)
}

/// The midpoint of every pixel edge, with the sharp corners (turns between
/// two long runs, and an open edge's ends) in their places, flagged.
fn midpoints(r: &Stairs) -> (Vec<Point>, Vec<bool>) {
    let (mut points, mut sharp) = (Vec::new(), Vec::new());
    let mut at = r.start;
    let n = r.runs.len();
    for (k, &(d, len)) in r.runs.iter().enumerate() {
        let turn = (r.closed || k > 0) && r.runs[(k + n - 1) % n].1 >= CORNER_RUN;
        if (turn && len >= CORNER_RUN) || (!r.closed && k == 0) {
            points.push(at);
            sharp.push(true);
        }
        let u = unit(d);
        for _ in 0..len {
            points.push(at + u * 0.5);
            sharp.push(false);
            at += u;
        }
    }
    if !r.closed {
        points.push(at);
        sharp.push(true);
    }
    (points, sharp)
}

/// `points` (a closed ring) averaged with their neighbours [`SMOOTHING`]
/// times; sharp points stay where they are (an open edge's ends are sharp,
/// so it never wraps round).
fn smoothed(mut points: Vec<Point>, sharp: &[bool]) -> Vec<Point> {
    let n = points.len();
    if n < 3 {
        return points;
    }
    for _ in 0..SMOOTHING {
        let before = points.clone();
        for (i, p) in points.iter_mut().enumerate() {
            if !sharp[i] {
                let (a, b) = (before[(i + n - 1) % n], before[(i + 1) % n]);
                *p = ((a.to_vec2() + p.to_vec2() * 2.0 + b.to_vec2()) / 4.0).to_point();
            }
        }
    }
    points
}

/// The pixel corners where the runs turn: the exact pixel outline (with
/// its end, when open).
fn run_corners(r: &Stairs) -> Vec<Point> {
    let mut at = r.start;
    let mut out = Vec::with_capacity(r.runs.len() + 1);
    for &(d, len) in &r.runs {
        out.push(at);
        #[allow(clippy::cast_precision_loss)] // a run is shorter than 2^52
        let len = len as f64;
        at += unit(d) * len;
    }
    if !r.closed {
        out.push(at);
    }
    out
}

/// The polygon through `points` (closed or not) with as few vertices as
/// stay within [`POLYGON_TOLERANCE`], keeping every sharp point; each
/// vertex flagged when sharp.
fn polygon(points: &[Point], sharp: &[bool], closed: bool) -> Vec<(Point, bool)> {
    let n = points.len();
    if !closed {
        // the ends are sharp: chains between sharp points, then the end
        let cuts: Vec<usize> = (0..n).filter(|&i| sharp[i]).collect();
        let mut out = Vec::new();
        for pair in cuts.windows(2) {
            let chain = &points[pair[0]..=pair[1]];
            let kept = douglas_peucker(chain);
            for &i in &kept[..kept.len() - 1] {
                out.push((chain[i], sharp[pair[0] + i]));
            }
        }
        if let Some(&last) = points.last() {
            out.push((last, true));
        }
        return out;
    }
    let mut cuts: Vec<usize> = (0..n).filter(|&i| sharp[i]).collect();
    if cuts.is_empty() {
        // no corner to start from: split at the point furthest from the first
        let far = (1..n)
            .max_by(|&a, &b| {
                let (da, db) = (points[a].distance(points[0]), points[b].distance(points[0]));
                da.total_cmp(&db)
            })
            .unwrap_or(0);
        cuts = vec![0, far];
    }
    let mut out = Vec::new();
    for (k, &from) in cuts.iter().enumerate() {
        let to = cuts.get(k + 1).copied().unwrap_or(cuts[0] + n);
        let chain: Vec<Point> = (from..=to).map(|i| points[i % n]).collect();
        let kept = douglas_peucker(&chain);
        // the chain's last point starts the next chain
        for &i in &kept[..kept.len() - 1] {
            out.push((chain[i], sharp[(from + i) % n]));
        }
    }
    out
}

/// Indices of `chain` kept by Ramer–Douglas–Peucker with
/// [`POLYGON_TOLERANCE`]; both ends always.
fn douglas_peucker(chain: &[Point]) -> Vec<usize> {
    let last = chain.len() - 1;
    let mut keep = vec![false; chain.len()];
    keep[0] = true;
    keep[last] = true;
    let mut todo = vec![(0, last)];
    while let Some((a, b)) = todo.pop() {
        if b <= a + 1 {
            continue;
        }
        let line = Line::new(chain[a], chain[b]);
        let mut far = (0.0, a);
        for (i, &p) in chain.iter().enumerate().take(b).skip(a + 1) {
            let d = line.nearest(p, 1e-9).distance_sq;
            if d > far.0 {
                far = (d, i);
            }
        }
        if far.0 > POLYGON_TOLERANCE * POLYGON_TOLERANCE {
            keep[far.1] = true;
            todo.push((a, far.1));
            todo.push((far.1, b));
        }
    }
    (0..chain.len()).filter(|&i| keep[i]).collect()
}

#[cfg(test)]
mod tests {
    use kurbo::PathEl;

    use super::*;
    use crate::trace::outline::loops;

    fn traced(mask: &[bool], w: usize, h: usize) -> Vec<BezPath> {
        loops(mask, w, h).iter().map(fitted).collect()
    }

    fn disc(size: usize, r: f64) -> Vec<bool> {
        #[allow(clippy::cast_precision_loss)]
        let c = size as f64 / 2.0;
        (0..size * size)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = ((i % size) as f64 + 0.5, (i / size) as f64 + 0.5);
                (x - c).hypot(y - c) <= r
            })
            .collect()
    }

    #[test]
    fn a_square_stays_four_straight_corners() {
        let (w, h) = (20, 20);
        let mask: Vec<bool> = (0..w * h)
            .map(|i| (4..16).contains(&(i % w)) && (4..16).contains(&(i / w)))
            .collect();
        let paths = traced(&mask, w, h);
        assert_eq!(paths.len(), 1);
        let els = paths[0].elements();
        assert!(els.iter().all(|e| !matches!(e, PathEl::CurveTo(..))));
        assert!((paths[0].area().abs() - 144.0).abs() < 1e-9);
        let nodes = els
            .iter()
            .filter(|e| !matches!(e, PathEl::ClosePath))
            .count();
        assert!(nodes <= 5, "{els:?}");
    }

    #[test]
    fn a_disc_becomes_a_few_curves_close_to_its_area() {
        let r = 30.0;
        let paths = traced(&disc(80, r), 80, 80);
        assert_eq!(paths.len(), 1);
        let els = paths[0].elements();
        assert!(els.iter().any(|e| matches!(e, PathEl::CurveTo(..))));
        assert!(els.len() < 20, "{} elements", els.len());
        let exact = std::f64::consts::PI * r * r;
        assert!((paths[0].area().abs() - exact).abs() / exact < 0.02);
    }

    #[test]
    fn an_open_edge_stays_open_and_keeps_its_ends() {
        // two colours side by side meet along x = 5, top to bottom
        let (w, h) = (10, 8);
        let labels: Vec<u8> = (0..w * h).map(|i| u8::from(i % w >= 5)).collect();
        let lines = crate::trace::edges::lines(&labels, w, h);
        assert_eq!(lines.len(), 1);
        let path = fitted(&lines[0]);
        let els = path.elements();
        assert!(!els.iter().any(|e| matches!(e, PathEl::ClosePath)));
        let PathEl::MoveTo(start) = els[0] else {
            panic!("starts with a move");
        };
        let end = kurbo::ParamCurve::end(&path.segments().last().expect("a segment"));
        let ends = [start, end];
        assert!(ends.contains(&Point::new(5.0, 0.0)), "{ends:?}");
        assert!(ends.contains(&Point::new(5.0, 8.0)), "{ends:?}");
        assert_eq!(path.segments().count(), 1, "one straight line");
    }

    #[test]
    fn a_one_pixel_line_keeps_its_pixels() {
        let (w, h) = (12, 3);
        let mask: Vec<bool> = (0..w * h)
            .map(|i| i / w == 1 && (1..11).contains(&(i % w)))
            .collect();
        let paths = traced(&mask, w, h);
        assert_eq!(paths.len(), 1);
        assert!(
            (paths[0].area().abs() - 10.0).abs() < 1e-9,
            "{}",
            paths[0].area()
        );
    }
}
