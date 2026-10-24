//! Outlines as polygons for the overlay engine (`i_overlay`): flattened
//! within [`FLATTEN`] of the curves, open subpaths closed implicitly, and
//! normalised by their fill rule into outer contours with holes.

use i_overlay::core::fill_rule::FillRule as Rule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::simplify::SimplifyShape;
use i_overlay::float::single::SingleFloatOverlay;
use kurbo::{BezPath, PathEl};
use vextor_core::FillRule;

/// One closed ring of points.
pub(super) type Contour = Vec<[f64; 2]>;
/// Shapes as `i_overlay` returns them: each an outer contour, then holes.
pub(super) type Shapes = Vec<Vec<Contour>>;

/// How far a polygon may stray from the curve it stands for, in document
/// px. Edges the result shares with its inputs get their curves back
/// (`recover`), so this only limits where new intersections land.
pub(super) const FLATTEN: f64 = 0.005;

/// `path` with every open subpath closed (booleans close open paths
/// implicitly, as SVG fills them).
#[must_use]
pub(super) fn closed(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    let mut open = false;
    for &el in path.elements() {
        match el {
            PathEl::MoveTo(_) if open => out.close_path(),
            PathEl::ClosePath => open = false,
            _ => {}
        }
        if let PathEl::MoveTo(_) = el {
            open = true;
        }
        out.push(el);
    }
    if open {
        out.close_path();
    }
    out
}

/// The closed subpaths of `path` as rings of points; rings with a
/// non-finite point or fewer than three points are dropped.
fn flatten(path: &BezPath) -> Vec<Contour> {
    let mut rings: Vec<Contour> = Vec::new();
    kurbo::flatten(closed(path), FLATTEN, |el| match el {
        PathEl::MoveTo(p) => rings.push(vec![[p.x, p.y]]),
        PathEl::LineTo(p) => {
            if let Some(ring) = rings.last_mut() {
                ring.push([p.x, p.y]);
            }
        }
        _ => {}
    });
    rings.retain(|r| r.len() >= 3 && r.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
    rings
}

/// The area `path` fills under `fill_rule`, as non-overlapping shapes.
pub(super) fn normalise(path: &BezPath, fill_rule: FillRule) -> Shapes {
    let rings = flatten(path);
    if rings.is_empty() {
        return Shapes::new();
    }
    let rule = match fill_rule {
        FillRule::NonZero => Rule::NonZero,
        FillRule::EvenOdd => Rule::EvenOdd,
    };
    rings.simplify_shape(rule)
}

/// Normalised shapes merged into one area: they wind the same way, so
/// overlaps count once.
pub(super) fn merged(shapes: &Shapes) -> Shapes {
    if shapes.is_empty() {
        return Shapes::new();
    }
    shapes.simplify_shape(Rule::NonZero)
}

/// `rule` applied to two normalised areas.
pub(super) fn overlay(subject: &Shapes, clip: &Shapes, rule: OverlayRule) -> Shapes {
    match (subject.is_empty(), clip.is_empty()) {
        (true, true) => Shapes::new(),
        (true, false) => match rule {
            OverlayRule::Union | OverlayRule::Xor | OverlayRule::Clip => clip.clone(),
            _ => Shapes::new(),
        },
        (false, true) => match rule {
            OverlayRule::Intersect | OverlayRule::Clip => Shapes::new(),
            _ => subject.clone(),
        },
        (false, false) => subject.overlay(clip, rule, Rule::NonZero),
    }
}

/// Signed area of a ring: positive for the engine's outer contours,
/// negative for its holes.
pub(super) fn ring_area(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    let twice: f64 = (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    twice / 2.0
}
