//! Snapping (spec 08): where a dragged point or box lands near the grid,
//! other objects' nodes, edges and centres, and the artboards.
//!
//! The targets are collected once, when a drag begins, from the document
//! as it was then ([`SnapTargets::collect`]); every frame of the drag then
//! only resolves against them. Among the targets within the tolerance the
//! highest-priority kind wins (node > grid > edge > centre > artboard),
//! then the nearest.

use kurbo::{BezPath, ParamCurve, ParamCurveNearest, PathEl, PathSeg, Point, Rect, Shape, Vec2};
use vextor_core::{Document, GridSettings, NodeId, SnapSettings};

use crate::hit::outlines;
use crate::text::Fonts;

mod moving;

/// Accuracy of the nearest-point search on curves, in document px.
const NEAREST_ACCURACY: f64 = 1e-3;

/// What a point snapped to, highest priority first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SnapKind {
    Node,
    Grid,
    Edge,
    Centre,
    Artboard,
}

impl SnapKind {
    /// The word the canvas shows next to the snapped point.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Node => "Node",
            Self::Grid => "Grid",
            Self::Edge => "Edge",
            Self::Centre => "Centre",
            Self::Artboard => "Artboard",
        }
    }
}

/// A point that snapped: where it lands and to what.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snap {
    pub point: Point,
    pub kind: SnapKind,
}

/// A line from a moved box to what it lines up with (a smart guide), in
/// document space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    pub from: Point,
    pub to: Point,
}

/// How a moved box snapped: the offset to add to the move, and per axis
/// what it lined up with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoxSnap {
    pub offset: Vec2,
    pub x: Option<SnapKind>,
    pub y: Option<SnapKind>,
    /// Smart guides: one line per axis that lined up with an object or
    /// artboard (none for the grid).
    pub guides: Vec<Guide>,
}

impl BoxSnap {
    /// The highest-priority kind either axis snapped to.
    #[must_use]
    pub fn kind(&self) -> Option<SnapKind> {
        match (self.x, self.y) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

/// Everything a drag can snap to, in document space.
#[derive(Clone, Debug, Default)]
pub struct SnapTargets {
    /// Minor grid spacing, when snapping to the grid.
    grid: Option<f64>,
    nodes: Vec<Point>,
    /// Outline segments and object box sides, with their bounds.
    edges: Vec<(Rect, PathSeg)>,
    /// Object boxes (for centres and box-to-box alignment).
    boxes: Vec<Rect>,
    artboards: Vec<Rect>,
    to_edges: bool,
    to_centres: bool,
}

impl SnapTargets {
    /// The targets of `doc` its snap settings ask for, leaving out the
    /// objects in `exclude` (the ones being dragged).
    #[must_use]
    pub fn collect(doc: &Document, fonts: &Fonts, exclude: &[NodeId]) -> Self {
        let snap = &doc.grid.snap;
        let mut t = Self {
            grid: snap.to_grid.then(|| minor_spacing(&doc.grid)).flatten(),
            to_edges: snap.to_edges,
            to_centres: snap.to_centers,
            ..Self::default()
        };
        if snap.to_artboard {
            t.artboards = doc
                .artboards
                .iter()
                .map(|a| {
                    Rect::new(
                        a.rect.x,
                        a.rect.y,
                        a.rect.x + a.rect.width,
                        a.rect.y + a.rect.height,
                    )
                })
                .collect();
        }
        let mut objects: Vec<(NodeId, Rect)> = Vec::new();
        for (object, outline) in outlines(doc, fonts) {
            if exclude.contains(&object) {
                continue;
            }
            if snap.to_nodes {
                t.nodes.extend(end_points(&outline));
            }
            if snap.to_edges {
                t.edges
                    .extend(outline.segments().map(|s| (s.bounding_box(), s)));
            }
            let b = outline.bounding_box();
            match objects.iter_mut().find(|(id, _)| *id == object) {
                Some((_, r)) => *r = r.union(b),
                None => objects.push((object, b)),
            }
        }
        t.boxes = objects.into_iter().map(|(_, r)| r).collect();
        if snap.to_edges {
            let sides: Vec<_> = t.boxes.iter().flat_map(|b| box_sides(*b)).collect();
            t.edges.extend(sides);
        }
        t
    }

    /// Adds points that count as nodes (the edited path's other nodes).
    pub fn add_nodes(&mut self, points: impl IntoIterator<Item = Point>) {
        self.nodes.extend(points);
    }

    /// Where `p` snaps within `tolerance` document px, if anywhere.
    #[must_use]
    pub fn point(&self, p: Point, tolerance: f64) -> Option<Snap> {
        let within = |q: Point| ((q - p).hypot() <= tolerance).then_some(q);
        let nearest = |qs: &mut dyn Iterator<Item = Point>| {
            qs.filter_map(within)
                .min_by(|a, b| (*a - p).hypot().total_cmp(&(*b - p).hypot()))
        };
        let candidates: [(SnapKind, Option<Point>); 5] = [
            (SnapKind::Node, nearest(&mut self.nodes.iter().copied())),
            (SnapKind::Grid, self.grid_point(p, tolerance)),
            (SnapKind::Edge, self.edge_point(p, tolerance)),
            (
                SnapKind::Centre,
                if self.to_centres {
                    nearest(&mut self.boxes.iter().map(Rect::center))
                } else {
                    None
                },
            ),
            (
                SnapKind::Artboard,
                nearest(
                    &mut self
                        .artboards
                        .iter()
                        .flat_map(|&a| [a.center(), nearest_on_box(a, p)]),
                ),
            ),
        ];
        candidates
            .into_iter()
            .find_map(|(kind, q)| q.map(|point| Snap { point, kind }))
    }

    /// The nearest grid crossing, snapping each axis that is within
    /// `tolerance` of a grid line.
    fn grid_point(&self, p: Point, tolerance: f64) -> Option<Point> {
        let g = self.grid?;
        let (x, y) = ((p.x / g).round() * g, (p.y / g).round() * g);
        let (sx, sy) = ((x - p.x).abs() <= tolerance, (y - p.y).abs() <= tolerance);
        (sx || sy).then(|| Point::new(if sx { x } else { p.x }, if sy { y } else { p.y }))
    }

    /// The nearest point on an outline or object box side.
    fn edge_point(&self, p: Point, tolerance: f64) -> Option<Point> {
        self.edges
            .iter()
            .filter(|(b, _)| b.inflate(tolerance, tolerance).contains(p))
            .map(|(_, s)| s.eval(s.nearest(p, NEAREST_ACCURACY).t))
            .filter(|q| (*q - p).hypot() <= tolerance)
            .min_by(|a, b| (*a - p).hypot().total_cmp(&(*b - p).hypot()))
    }
}

/// The spacing between minor grid lines, when the grid is valid.
#[must_use]
pub fn minor_spacing(grid: &GridSettings) -> Option<f64> {
    let g = grid.size / f64::from(grid.subdivisions.max(1));
    (g.is_finite() && g > 0.0).then_some(g)
}

/// The snap tolerance in document px at `zoom` (screen points per
/// document px): the setting is in screen px.
#[must_use]
pub fn tolerance(snap: &SnapSettings, zoom: f64) -> f64 {
    if zoom > 0.0 && zoom.is_finite() {
        snap.tolerance_px / zoom
    } else {
        0.0
    }
}

/// `p` on whole px (pixel snap).
#[must_use]
pub fn pixel_snap(p: Point) -> Point {
    Point::new(p.x.round(), p.y.round())
}

/// Every anchor of `path`: the start and end of each segment.
fn end_points(path: &BezPath) -> impl Iterator<Item = Point> + '_ {
    path.elements().iter().filter_map(|el| match *el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) | PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => {
            Some(p)
        }
        PathEl::ClosePath => None,
    })
}

/// The nearest point to `p` on the outline of `r`.
fn nearest_on_box(r: Rect, p: Point) -> Point {
    box_sides(r)
        .map(|(_, s)| s.eval(s.nearest(p, NEAREST_ACCURACY).t))
        .min_by(|a, b| (*a - p).hypot().total_cmp(&(*b - p).hypot()))
        .unwrap_or(p)
}

/// The four sides of `r` as segments, with their bounds.
fn box_sides(r: Rect) -> impl Iterator<Item = (Rect, PathSeg)> {
    let c = [
        Point::new(r.x0, r.y0),
        Point::new(r.x1, r.y0),
        Point::new(r.x1, r.y1),
        Point::new(r.x0, r.y1),
    ];
    (0..4).map(move |i| {
        let line = kurbo::Line::new(c[i], c[(i + 1) % 4]);
        (line.bounding_box(), PathSeg::Line(line))
    })
}
