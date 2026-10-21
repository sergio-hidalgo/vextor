//! The node type, handle and path command icons (spec 07), on the same
//! 16 × 16 grid as the others: a node is a small filled square, a handle
//! a thin line ending in a dot.

use eframe::egui::{self, Pos2, Stroke};

use super::{Icon, P, Pen};

impl Pen<'_> {
    /// Draws one of the path icons; other icons draw nothing here.
    pub(super) fn path_icon(&self, icon: Icon) {
        match icon {
            Icon::NodeCorner => {
                self.line((2.0, 13.0), (8.0, 4.5));
                self.line((8.0, 4.5), (14.0, 13.0));
                self.node((8.0, 4.5));
            }
            Icon::NodeSmooth => self.arch_with_handle(2.5, 7.0),
            Icon::NodeSymmetric => self.arch_with_handle(6.0, 6.0),
            Icon::HandleIn => {
                self.line((3.0, 8.0), (14.0, 8.0));
                self.dot((3.0, 8.0), 2.0);
            }
            Icon::HandleOut => {
                self.line((2.0, 8.0), (13.0, 8.0));
                self.dot((13.0, 8.0), 2.0);
            }
            Icon::PathClose => {
                self.ring((8.0, 9.0), 5.5, 0.0, std::f32::consts::TAU);
                self.node((8.0, 3.5));
                self.node((8.0, 14.5));
            }
            Icon::PathOpen => {
                // A loop with a gap at the top, a node at each side of it.
                let (a0, a1) = ((-60f32).to_radians(), 240f32.to_radians());
                self.ring((8.0, 9.0), 5.5, a0, a1);
                self.node((8.0 + 5.5 * a0.cos(), 9.0 + 5.5 * a0.sin()));
                self.node((8.0 + 5.5 * a1.cos(), 9.0 + 5.5 * a1.sin()));
            }
            Icon::PathJoin => {
                self.line((1.0, 13.0), (5.5, 8.0));
                self.line((10.5, 8.0), (15.0, 13.0));
                self.painter.extend(egui::Shape::dashed_line(
                    &[self.at((5.5, 8.0)), self.at((10.5, 8.0))],
                    Stroke::new(self.stroke.width * 0.8, self.color),
                    1.5 * self.k,
                    1.2 * self.k,
                ));
                self.node((5.5, 8.0));
                self.node((10.5, 8.0));
            }
            Icon::PathSplit => {
                // A path cut in two at a node, and scissors below it.
                self.line((1.0, 3.5), (6.0, 3.5));
                self.line((10.0, 3.5), (15.0, 3.5));
                self.node((6.0, 3.5));
                self.node((10.0, 3.5));
                self.line((6.5, 6.5), (10.0, 11.0));
                self.line((9.5, 6.5), (6.0, 11.0));
                self.ring((5.0, 12.5), 1.8, 0.0, std::f32::consts::TAU);
                self.ring((11.0, 12.5), 1.8, 0.0, std::f32::consts::TAU);
            }
            Icon::PathReverse => {
                self.arrow((2.0, 5.0), (14.0, 5.0));
                self.arrow((14.0, 11.0), (2.0, 11.0));
            }
            _ => {}
        }
    }

    /// A node: a small filled square centred on `c`.
    fn node(&self, (x, y): P) {
        self.fill((x - 2.2, y - 2.2), (x + 2.2, y + 2.2));
    }

    /// A filled dot of radius `r` (grid units) at `c`.
    fn dot(&self, c: P, r: f32) {
        self.painter
            .circle_filled(self.at(c), r * self.k, self.color);
    }

    /// An arc (or a whole circle) round `c`; angles in radians, y down.
    fn ring(&self, c: P, r: f32, a0: f32, a1: f32) {
        let steps: u8 = 24;
        let pts: Vec<Pos2> = (0..=steps)
            .map(|i| {
                let a = a0 + (a1 - a0) * f32::from(i) / f32::from(steps);
                self.at((c.0 + r * a.cos(), c.1 + r * a.sin()))
            })
            .collect();
        self.painter.add(egui::Shape::line(pts, self.stroke));
    }

    /// An arch over a node at its top, with the node's handle along the
    /// tangent: `left` and `right` arm lengths (equal: symmetric).
    fn arch_with_handle(&self, left: f32, right: f32) {
        let top = (8.0, 6.0);
        let curve = |pts: [P; 4]| {
            self.painter
                .add(egui::epaint::CubicBezierShape::from_points_stroke(
                    pts.map(|p| self.at(p)),
                    false,
                    egui::Color32::TRANSPARENT,
                    self.stroke,
                ));
        };
        curve([(1.5, 14.0), (1.5, 8.5), (4.0, 6.0), top]);
        curve([top, (12.0, 6.0), (14.5, 8.5), (14.5, 14.0)]);
        let thin = Stroke::new(self.stroke.width * 0.7, self.color);
        let (l, r) = ((top.0 - left, top.1), (top.0 + right, top.1));
        self.painter.line_segment([self.at(l), self.at(r)], thin);
        self.dot(l, 1.3);
        self.dot(r, 1.3);
        self.node(top);
    }
}
