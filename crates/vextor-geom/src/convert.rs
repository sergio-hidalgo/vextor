//! Model geometry ↔ `kurbo::BezPath`.
//!
//! Parametric shapes become cubic Béziers the way SVG renderers draw them;
//! live corner radii are baked in as circular-arc approximations.

use std::f64::consts::{FRAC_PI_2, PI};

use kurbo::{BezPath, PathEl, Point, Vec2};
use vextor_core::{CornerRadii, EPS, FillRule, Geometry, Path, PathNode, SubPath};

/// Handle length of a quarter-circle cubic, per unit radius.
const KAPPA: f64 = 0.552_284_749_830_793_6;

/// A model point as a `kurbo` point.
#[must_use]
pub fn to_kurbo(p: vextor_core::Point) -> Point {
    Point::new(p.x, p.y)
}

fn handle(v: Vec2) -> Option<vextor_core::Vec2> {
    (v.hypot() > EPS).then(|| vextor_core::Vec2::new(v.x, v.y))
}

/// The outline of a validated geometry (`Document::validate`) as a Bézier
/// path, in the shape's own coordinates (the node transform is not applied).
#[must_use]
pub fn to_bezpath(geometry: &Geometry) -> BezPath {
    match geometry {
        Geometry::Rect { rect, radius } => rounded_rect(*rect, *radius),
        Geometry::Ellipse { center, rx, ry } => ellipse(to_kurbo(*center), *rx, *ry),
        Geometry::Polygon {
            center,
            radius,
            sides,
            corner_radius,
        } => {
            let verts = regular_vertices(to_kurbo(*center), *sides, |_| *radius);
            rounded_polygon(&verts, *corner_radius)
        }
        Geometry::Star {
            center,
            outer,
            inner,
            points,
            corner_radius,
        } => {
            let verts = regular_vertices(to_kurbo(*center), points.saturating_mul(2), |i| {
                if i % 2 == 0 { *outer } else { *inner }
            });
            rounded_polygon(&verts, *corner_radius)
        }
        Geometry::Line { from, to } => {
            let mut path = BezPath::new();
            path.move_to(to_kurbo(*from));
            path.line_to(to_kurbo(*to));
            path
        }
        Geometry::Path(p) => path_to_bezpath(p),
    }
}

/// Any geometry as an editable path ("Convert to path"). Corner radii are
/// baked into curves; every node becomes a `Corner` node.
#[must_use]
pub fn geometry_to_path(geometry: &Geometry) -> Path {
    let fill_rule = match geometry {
        Geometry::Path(p) => p.fill_rule,
        _ => FillRule::NonZero,
    };
    bezpath_to_path(&to_bezpath(geometry), fill_rule)
}

/// The radii a rect is drawn with: all scaled down by the same factor
/// until adjacent ones fit along each side (the CSS `border-radius` rule).
#[must_use]
pub fn fitted_radii(r: vextor_core::Rect, radius: CornerRadii) -> CornerRadii {
    let (w, h) = (r.width, r.height);
    let fit = |len: f64, a: f64, b: f64| if a + b > len { len / (a + b) } else { 1.0 };
    let f = fit(w, radius.tl, radius.tr)
        .min(fit(w, radius.bl, radius.br))
        .min(fit(h, radius.tl, radius.bl))
        .min(fit(h, radius.tr, radius.br));
    CornerRadii {
        tl: radius.tl * f,
        tr: radius.tr * f,
        br: radius.br * f,
        bl: radius.bl * f,
    }
}

fn rounded_rect(r: vextor_core::Rect, radius: CornerRadii) -> BezPath {
    let (x0, y0, w, h) = (r.x, r.y, r.width, r.height);
    let (x1, y1) = (x0 + w, y0 + h);
    let CornerRadii { tl, tr, br, bl } = fitted_radii(r, radius);

    let mut path = BezPath::new();
    path.move_to((x0 + tl, y0));
    path.line_to((x1 - tr, y0));
    if tr > 0.0 {
        path.curve_to(
            (x1 - tr + KAPPA * tr, y0),
            (x1, y0 + tr - KAPPA * tr),
            (x1, y0 + tr),
        );
    }
    path.line_to((x1, y1 - br));
    if br > 0.0 {
        path.curve_to(
            (x1, y1 - br + KAPPA * br),
            (x1 - br + KAPPA * br, y1),
            (x1 - br, y1),
        );
    }
    path.line_to((x0 + bl, y1));
    if bl > 0.0 {
        path.curve_to(
            (x0 + bl - KAPPA * bl, y1),
            (x0, y1 - bl + KAPPA * bl),
            (x0, y1 - bl),
        );
    }
    path.line_to((x0, y0 + tl));
    if tl > 0.0 {
        path.curve_to(
            (x0, y0 + tl - KAPPA * tl),
            (x0 + tl - KAPPA * tl, y0),
            (x0 + tl, y0),
        );
    }
    path.close_path();
    path
}

fn ellipse(c: Point, rx: f64, ry: f64) -> BezPath {
    let (kx, ky) = (KAPPA * rx, KAPPA * ry);
    let mut path = BezPath::new();
    path.move_to((c.x + rx, c.y));
    path.curve_to((c.x + rx, c.y + ky), (c.x + kx, c.y + ry), (c.x, c.y + ry));
    path.curve_to((c.x - kx, c.y + ry), (c.x - rx, c.y + ky), (c.x - rx, c.y));
    path.curve_to((c.x - rx, c.y - ky), (c.x - kx, c.y - ry), (c.x, c.y - ry));
    path.curve_to((c.x + kx, c.y - ry), (c.x + rx, c.y - ky), (c.x + rx, c.y));
    path.close_path();
    path
}

/// `count` vertices evenly spaced around `center`, the first pointing up.
fn regular_vertices(center: Point, count: u32, radius: impl Fn(u32) -> f64) -> Vec<Point> {
    (0..count)
        .map(|i| {
            let angle = -FRAC_PI_2 + f64::from(i) * 2.0 * PI / f64::from(count);
            center + Vec2::new(angle.cos(), angle.sin()) * radius(i)
        })
        .collect()
}

/// A rounded corner at a vertex between straight edges to `prev` and `next`:
/// the curve runs from `a` (on the edge to `prev`) to `b` (on the edge to
/// `next`).
#[derive(Clone, Copy)]
struct Fillet {
    a: Point,
    c1: Point,
    c2: Point,
    b: Point,
}

/// The fillet of radius `radius` at `v`, shrunk so it uses at most half of
/// each edge; `None` when there is nothing to round.
fn fillet(prev: Point, vertex: Point, next: Point, radius: f64) -> Option<Fillet> {
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
    })
}

fn rounded_polygon(verts: &[Point], radius: f64) -> BezPath {
    let n = verts.len();
    let fillets: Vec<Option<Fillet>> = (0..n)
        .map(|i| fillet(verts[(i + n - 1) % n], verts[i], verts[(i + 1) % n], radius))
        .collect();
    let mut path = BezPath::new();
    if n == 0 {
        return path;
    }
    path.move_to(fillets[0].map_or(verts[0], |f| f.b));
    for i in 1..=n {
        let j = i % n;
        match fillets[j] {
            Some(f) => {
                path.line_to(f.a);
                path.curve_to(f.c1, f.c2, f.b);
            }
            None if j != 0 => path.line_to(verts[j]),
            None => {} // closing edge drawn by close_path
        }
    }
    path.close_path();
    path
}

/// A model path as a Bézier path; node corner radii are rounded where both
/// adjacent segments are straight (ignored next to curves).
#[must_use]
pub fn path_to_bezpath(path: &Path) -> BezPath {
    let mut out = BezPath::new();
    for sp in &path.subpaths {
        subpath_to_bezpath(sp, &mut out);
    }
    out
}

fn subpath_to_bezpath(sp: &SubPath, out: &mut BezPath) {
    let nodes = &sp.nodes;
    let n = nodes.len();
    if n == 0 {
        return;
    }
    let anchor = |i: usize| to_kurbo(nodes[i].anchor);
    let straight =
        |i: usize, j: usize| nodes[i].handle_out.is_none() && nodes[j].handle_in.is_none();
    let fillets: Vec<Option<Fillet>> = (0..n)
        .map(|i| {
            let has_neighbours = sp.closed || (i > 0 && i + 1 < n);
            if !has_neighbours || n < 3 {
                return None;
            }
            let (p, q) = ((i + n - 1) % n, (i + 1) % n);
            if !(straight(p, i) && straight(i, q)) {
                return None;
            }
            fillet(anchor(p), anchor(i), anchor(q), nodes[i].corner_radius)
        })
        .collect();

    out.move_to(fillets[0].map_or(anchor(0), |f| f.b));
    for s in 0..sp.segment_count() {
        let (i, j) = (s, (s + 1) % n);
        if straight(i, j) {
            let closing_plain = sp.closed && j == 0 && fillets[0].is_none();
            if !closing_plain {
                out.line_to(fillets[j].map_or(anchor(j), |f| f.a));
            }
        } else {
            let h_out = nodes[i]
                .handle_out
                .map_or(Vec2::ZERO, |v| Vec2::new(v.x, v.y));
            let h_in = nodes[j]
                .handle_in
                .map_or(Vec2::ZERO, |v| Vec2::new(v.x, v.y));
            out.curve_to(anchor(i) + h_out, anchor(j) + h_in, anchor(j));
        }
        if let Some(f) = fillets[j] {
            out.curve_to(f.c1, f.c2, f.b);
        }
    }
    if sp.closed {
        out.close_path();
    }
}

/// A Bézier path as an editable model path. Quadratics are raised to
/// cubics; a closing node that repeats the start is merged into it. Nodes
/// are `Corner` nodes with no corner radius.
#[must_use]
pub fn bezpath_to_path(bez: &BezPath, fill_rule: FillRule) -> Path {
    let mut subpaths: Vec<SubPath> = Vec::new();
    let mut current: Option<SubPath> = None;
    let corner = |p: Point| PathNode::corner(p.x, p.y);
    for el in bez.elements() {
        if let PathEl::MoveTo(p) = *el {
            subpaths.extend(current.take());
            current = Some(SubPath {
                nodes: vec![corner(p)],
                closed: false,
            });
            continue;
        }
        // drawing without a MoveTo (or after a close) starts at the last start
        let start = subpaths
            .last()
            .map_or(Point::ZERO, |s| to_kurbo(s.nodes[0].anchor));
        let sp = current.get_or_insert_with(|| SubPath {
            nodes: vec![corner(start)],
            closed: false,
        });
        match *el {
            PathEl::MoveTo(_) => {} // handled above
            PathEl::LineTo(p) => sp.nodes.push(corner(p)),
            PathEl::QuadTo(q, p3) => {
                let p0 = sp.nodes.last().map_or(start, |n| to_kurbo(n.anchor));
                let c1 = p0 + (q - p0) * (2.0 / 3.0);
                let c2 = p3 + (q - p3) * (2.0 / 3.0);
                push_cubic(sp, c1, c2, p3);
            }
            PathEl::CurveTo(c1, c2, p3) => push_cubic(sp, c1, c2, p3),
            PathEl::ClosePath => {
                close(sp);
                subpaths.extend(current.take());
            }
        }
    }
    subpaths.extend(current);
    Path {
        subpaths,
        fill_rule,
    }
}

fn push_cubic(sp: &mut SubPath, c1: Point, c2: Point, p3: Point) {
    let last = sp.nodes.last_mut().expect("subpath starts with a node");
    last.handle_out = handle(c1 - to_kurbo(last.anchor));
    sp.nodes.push(PathNode {
        handle_in: handle(c2 - p3),
        ..PathNode::corner(p3.x, p3.y)
    });
}

fn close(sp: &mut SubPath) {
    let n = sp.nodes.len();
    if n > 1 && (to_kurbo(sp.nodes[n - 1].anchor) - to_kurbo(sp.nodes[0].anchor)).hypot() <= EPS {
        let end = sp.nodes.pop().expect("n > 1");
        sp.nodes[0].handle_in = end.handle_in;
    }
    sp.closed = sp.nodes.len() >= 2;
}
