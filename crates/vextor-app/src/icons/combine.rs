//! The path operation icons (spec 11), on the same 16 × 16 grid: two
//! overlapping circles, back (upper left) and front (lower right), filled
//! where the operation keeps area. Circles the result cuts away show as
//! outlines (Subtract: the front one; Intersect: both).

use eframe::egui::{self, Mesh, Pos2, Rect, Stroke};

use super::{Icon, P, Pen};

/// The two circles: centre and radius on the 16-unit grid.
const BACK: (P, f32) = ((6.0, 6.0), 4.8);
const FRONT: (P, f32) = ((10.0, 10.0), 4.8);
/// Rows the fill is built from, over the 16 units.
const ROWS: u16 = 96;
/// Points along each circle for its outline.
const STEPS: u16 = 64;

fn inside(((cx, cy), r): (P, f32), (x, y): P) -> bool {
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

impl Pen<'_> {
    /// Draws one of the path operation icons; others draw nothing here.
    pub(super) fn combine_icon(&self, icon: Icon) {
        let keeps: fn(bool, bool) -> bool = match icon {
            Icon::Union => |a, b| a || b,
            Icon::Subtract => |a, b| a && !b,
            Icon::Intersect => |a, b| a && b,
            Icon::Exclude => |a, b| a != b,
            _ => return,
        };
        self.fill_where(keeps);
        // The result's edge, drawn smooth over the filled rows.
        for (circle, other, back) in [(BACK, FRONT, true), (FRONT, BACK, false)] {
            self.circle_edge(circle, |p| {
                let o = inside(other, p);
                let (with, without) = if back {
                    (keeps(true, o), keeps(false, o))
                } else {
                    (keeps(o, true), keeps(o, false))
                };
                with != without
            });
        }
        let thin = Stroke::new(self.stroke.width * 0.7, self.color);
        let outlined: &[(P, f32)] = match icon {
            Icon::Subtract => &[FRONT],
            Icon::Intersect => &[BACK, FRONT],
            _ => &[],
        };
        for &((cx, cy), r) in outlined {
            self.painter
                .circle_stroke(self.at((cx, cy)), r * self.k, thin);
        }
    }

    /// Fills the points where `keeps(in back, in front)`, row by row, as
    /// one mesh (no seams between rows).
    fn fill_where(&self, keeps: fn(bool, bool) -> bool) {
        let mut mesh = Mesh::default();
        let h = 16.0 / f32::from(ROWS);
        for row in 0..ROWS {
            let y = (f32::from(row) + 0.5) * h;
            let mut cuts = vec![0.0, 16.0];
            for ((cx, cy), r) in [BACK, FRONT] {
                let d = r * r - (y - cy).powi(2);
                if d > 0.0 {
                    cuts.extend([cx - d.sqrt(), cx + d.sqrt()]);
                }
            }
            cuts.sort_by(f32::total_cmp);
            for pair in cuts.windows(2) {
                let mid = (f32::midpoint(pair[0], pair[1]), y);
                if pair[1] > pair[0] && keeps(inside(BACK, mid), inside(FRONT, mid)) {
                    let rect = Rect::from_min_max(
                        self.at((pair[0], y - h / 2.0)),
                        self.at((pair[1], y + h / 2.0)),
                    );
                    mesh.add_colored_rect(rect, self.color);
                }
            }
        }
        self.painter.add(egui::Shape::mesh(mesh));
    }

    /// Strokes the parts of `circle` where `edge` holds.
    fn circle_edge(&self, ((cx, cy), r): (P, f32), edge: impl Fn(P) -> bool) {
        let at = |i: u16| {
            let a = f32::from(i) / f32::from(STEPS) * std::f32::consts::TAU;
            (cx + r * a.cos(), cy + r * a.sin())
        };
        let thin = Stroke::new(self.stroke.width * 0.5, self.color);
        let mut run: Vec<Pos2> = Vec::new();
        for i in 0..STEPS {
            let (a, b) = (at(i), at(i + 1));
            if edge((f32::midpoint(a.0, b.0), f32::midpoint(a.1, b.1))) {
                if run.is_empty() {
                    run.push(self.at(a));
                }
                run.push(self.at(b));
            } else if !run.is_empty() {
                self.painter
                    .add(egui::Shape::line(std::mem::take(&mut run), thin));
            }
        }
        if !run.is_empty() {
            self.painter.add(egui::Shape::line(run, thin));
        }
    }
}
