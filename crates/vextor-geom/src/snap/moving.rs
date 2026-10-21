//! Snapping a moved box (spec 08): its sides and centre line up, per
//! axis, with nodes, grid lines, object sides and centres, and artboards;
//! the lines it lines up with become smart guides.

use kurbo::{Point, Rect};

use super::{BoxSnap, Guide, SnapKind, SnapTargets};

/// One coordinate a box edge or centre can line up with, and the extent
/// of its source along the other axis (for the guide).
#[derive(Clone, Copy, Debug)]
struct AxisTarget {
    value: f64,
    kind: SnapKind,
    span: (f64, f64),
}

impl SnapTargets {
    /// The offset that lines box `b` (being moved) up with the targets,
    /// per axis: its left, centre or right with a node, a grid line, an
    /// object's side or centre, or an artboard's side or centre.
    #[must_use]
    pub fn moving_box(&self, b: Rect, tolerance: f64) -> BoxSnap {
        let mut out = BoxSnap::default();
        let x = best(
            &[b.x0, b.center().x, b.x1],
            &self.axis_targets(true),
            self.grid,
            tolerance,
        );
        let y = best(
            &[b.y0, b.center().y, b.y1],
            &self.axis_targets(false),
            self.grid,
            tolerance,
        );
        if let Some((d, t)) = x {
            out.offset.x = d;
            out.x = Some(t.kind);
        }
        if let Some((d, t)) = y {
            out.offset.y = d;
            out.y = Some(t.kind);
        }
        // Guides run from the box where it lands to what it lines up with.
        let landed = b + out.offset;
        if let Some((_, t)) = x.filter(|(_, t)| t.kind != SnapKind::Grid) {
            out.guides.push(Guide {
                from: Point::new(t.value, landed.y0.min(t.span.0)),
                to: Point::new(t.value, landed.y1.max(t.span.1)),
            });
        }
        if let Some((_, t)) = y.filter(|(_, t)| t.kind != SnapKind::Grid) {
            out.guides.push(Guide {
                from: Point::new(landed.x0.min(t.span.0), t.value),
                to: Point::new(landed.x1.max(t.span.1), t.value),
            });
        }
        out
    }

    /// The coordinates on one axis (`x` when `horizontal`) a box side or
    /// centre can line up with, except grid lines.
    fn axis_targets(&self, horizontal: bool) -> Vec<AxisTarget> {
        let pick = |p: Point| if horizontal { p.x } else { p.y };
        let span = |r: Rect| {
            if horizontal {
                (r.y0, r.y1)
            } else {
                (r.x0, r.x1)
            }
        };
        let sides = |r: Rect| {
            if horizontal {
                [r.x0, r.x1]
            } else {
                [r.y0, r.y1]
            }
        };
        let mut out = Vec::new();
        for &n in &self.nodes {
            let other = if horizontal { n.y } else { n.x };
            out.push(AxisTarget {
                value: pick(n),
                kind: SnapKind::Node,
                span: (other, other),
            });
        }
        for &r in &self.boxes {
            if self.to_edges {
                for v in sides(r) {
                    out.push(AxisTarget {
                        value: v,
                        kind: SnapKind::Edge,
                        span: span(r),
                    });
                }
            }
            if self.to_centres {
                out.push(AxisTarget {
                    value: pick(r.center()),
                    kind: SnapKind::Centre,
                    span: span(r),
                });
            }
        }
        for &r in &self.artboards {
            for v in sides(r).into_iter().chain([pick(r.center())]) {
                out.push(AxisTarget {
                    value: v,
                    kind: SnapKind::Artboard,
                    span: span(r),
                });
            }
        }
        out
    }
}

/// The best offset for any of `moving` onto `targets` (or a grid line
/// every `grid`) within `tolerance`: highest-priority kind, then smallest.
fn best(
    moving: &[f64],
    targets: &[AxisTarget],
    grid: Option<f64>,
    tolerance: f64,
) -> Option<(f64, AxisTarget)> {
    let mut found: Option<(f64, AxisTarget)> = None;
    let mut consider = |d: f64, t: AxisTarget| {
        if d.abs() > tolerance {
            return;
        }
        let better = found
            .is_none_or(|(fd, ft)| t.kind < ft.kind || (t.kind == ft.kind && d.abs() < fd.abs()));
        if better {
            found = Some((d, t));
        }
    };
    for &m in moving {
        for &t in targets {
            consider(t.value - m, t);
        }
        if let Some(g) = grid {
            let value = (m / g).round() * g;
            consider(
                value - m,
                AxisTarget {
                    value,
                    kind: SnapKind::Grid,
                    span: (0.0, 0.0),
                },
            );
        }
    }
    found
}
