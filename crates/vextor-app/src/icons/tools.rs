//! The tools strip icons (spec 07), on the same 16 × 16 grid: our own
//! line drawings of a pointer arrow, a node arrow, a pen nib, a T and the
//! shapes.

use std::f32::consts::{FRAC_PI_2, TAU};

use eframe::egui::{self, Pos2, Stroke};

use super::{Icon, P, Pen};

/// The pointer arrow's outline: tip, left edge, notch, tail, notch, wing.
const ARROW: [P; 7] = [
    (4.0, 1.5),
    (4.0, 13.5),
    (7.0, 10.8),
    (9.2, 15.0),
    (11.2, 14.0),
    (9.1, 9.9),
    (13.0, 9.9),
];

impl Pen<'_> {
    /// Draws one of the tool icons; other icons draw nothing here.
    pub(super) fn tool_icon(&self, icon: Icon) {
        match icon {
            Icon::ToolSelect => {
                // Solid: the head and the tail, each convex, then the edge.
                self.filled(&[ARROW[0], ARROW[1], ARROW[2], ARROW[5], ARROW[6]]);
                self.filled(&[ARROW[2], ARROW[3], ARROW[4], ARROW[5]]);
                self.closed(&ARROW);
            }
            Icon::ToolNode => {
                self.closed(&ARROW.map(|(x, y)| (x - 1.5, y - 0.5)));
                self.fill((11.0, 11.5), (15.0, 15.5));
            }
            Icon::ToolPen => {
                // A nib pointing down: its outline, the slit, the hole and
                // the collar.
                self.closed(&[
                    (8.0, 15.0),
                    (3.5, 8.5),
                    (5.8, 5.5),
                    (10.2, 5.5),
                    (12.5, 8.5),
                ]);
                self.line((8.0, 15.0), (8.0, 10.6));
                self.painter
                    .circle_stroke(self.at((8.0, 9.3)), 1.3 * self.k, self.thin());
                self.closed(&[(5.8, 5.5), (5.8, 1.5), (10.2, 1.5), (10.2, 5.5)]);
            }
            Icon::ToolText => {
                self.line((2.5, 2.5), (13.5, 2.5));
                self.line((2.5, 2.5), (2.5, 4.5));
                self.line((13.5, 2.5), (13.5, 4.5));
                self.line((8.0, 2.5), (8.0, 13.5));
                self.line((5.5, 13.5), (10.5, 13.5));
            }
            Icon::ToolRect => {
                self.closed(&[(1.5, 3.5), (14.5, 3.5), (14.5, 12.5), (1.5, 12.5)]);
            }
            Icon::ToolEllipse => {
                self.closed(&around((8.0, 8.0), (7.0, 5.0), 32, 0.0, |_| 1.0));
            }
            Icon::ToolPolygon => {
                self.closed(&around((8.0, 8.0), (7.0, 6.5), 6, 0.0, |_| 1.0));
            }
            Icon::ToolStar => {
                let points = around((8.0, 8.5), (7.0, 7.0), 10, -FRAC_PI_2, |i| {
                    if i % 2 == 0 { 1.0 } else { 0.45 }
                });
                self.closed(&points);
            }
            Icon::ToolLine => {
                self.line((2.5, 13.5), (13.5, 2.5));
                self.fill((1.0, 12.0), (4.0, 15.0));
                self.fill((12.0, 1.0), (15.0, 4.0));
            }
            _ => {}
        }
    }

    /// A thinner stroke, for small details.
    fn thin(&self) -> Stroke {
        Stroke::new(self.stroke.width * 0.8, self.color)
    }

    /// The closed outline through `pts`.
    fn closed(&self, pts: &[P]) {
        let pts: Vec<Pos2> = pts.iter().map(|&p| self.at(p)).collect();
        self.painter.add(egui::Shape::closed_line(pts, self.stroke));
    }

    /// A filled convex polygon through `pts`.
    fn filled(&self, pts: &[P]) {
        self.painter.add(egui::Shape::convex_polygon(
            pts.iter().map(|&p| self.at(p)).collect(),
            self.color,
            Stroke::NONE,
        ));
    }
}

/// `n` points round `c` on an ellipse of radii `(rx, ry)`, starting at
/// angle `start`; point `i` is pulled in to `scale(i)` of the radius.
fn around(c: P, (rx, ry): P, n: u8, start: f32, scale: impl Fn(u8) -> f32) -> Vec<P> {
    (0..n)
        .map(|i| {
            let a = start + TAU * f32::from(i) / f32::from(n);
            let k = scale(i);
            (c.0 + rx * k * a.cos(), c.1 + ry * k * a.sin())
        })
        .collect()
}
