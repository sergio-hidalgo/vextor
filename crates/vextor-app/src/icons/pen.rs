//! The icons' pen: lines, arcs, arrows and boxes on the 16 × 16 design
//! grid, scaled to the size an icon is drawn at.

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Stroke, Vec2};

/// Draws on the 16 × 16 grid at `origin`, scaled by `k`.
pub(super) struct Pen<'a> {
    pub(super) painter: &'a Painter,
    pub(super) origin: Pos2,
    pub(super) k: f32,
    pub(super) stroke: Stroke,
    pub(super) color: Color32,
}

/// A point on the 16 × 16 grid.
pub(super) type P = (f32, f32);

impl Pen<'_> {
    pub(super) fn at(&self, (x, y): P) -> Pos2 {
        self.origin + Vec2::new(x, y) * self.k
    }

    pub(super) fn line(&self, a: P, b: P) {
        self.painter
            .line_segment([self.at(a), self.at(b)], self.stroke);
    }

    /// A line from `a` with an arrowhead at `b`.
    pub(super) fn arrow(&self, a: P, b: P) {
        self.line(a, b);
        let (pa, pb) = (self.at(a), self.at(b));
        let dir = (pb - pa).normalized();
        let side = Vec2::new(-dir.y, dir.x);
        let head = 3.0 * self.k;
        for s in [1.0, -1.0] {
            self.painter
                .line_segment([pb, pb - dir * head + side * head * s * 0.8], self.stroke);
        }
    }

    /// An arc around `c` of radius `r` from angle `a0` to `a1` (radians,
    /// y down).
    pub(super) fn arc(&self, c: P, r: f32, a0: f32, a1: f32) {
        let pts: Vec<Pos2> = (0..=8)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)] // eight steps
                let a = a0 + (a1 - a0) * i as f32 / 8.0;
                self.at((c.0 + r * a.cos(), c.1 + r * a.sin()))
            })
            .collect();
        self.painter.add(egui::Shape::line(pts, self.stroke));
    }

    pub(super) fn rect(&self, a: P, b: P) -> Rect {
        Rect::from_two_pos(self.at(a), self.at(b))
    }

    pub(super) fn fill(&self, a: P, b: P) {
        self.painter
            .rect_filled(self.rect(a, b), 1.0 * self.k, self.color);
    }

    pub(super) fn outline(&self, a: P, b: P) {
        self.painter.rect_stroke(
            self.rect(a, b),
            1.0 * self.k,
            self.stroke,
            egui::StrokeKind::Middle,
        );
    }

    pub(super) fn dashed_outline(&self, a: P, b: P) {
        let r = self.rect(a, b);
        let corners = [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
            r.left_top(),
        ];
        self.painter.extend(egui::Shape::dashed_line(
            &corners,
            Stroke::new(self.stroke.width * 0.8, self.color),
            2.0 * self.k,
            1.5 * self.k,
        ));
    }

    /// Four text lines, long and short in turn, lined up at `align`
    /// (0 left, 0.5 centre, 1 right).
    pub(super) fn text_lines(&self, align: f32) {
        for (i, long) in [true, false, true, false].into_iter().enumerate() {
            #[allow(clippy::cast_precision_loss)] // four lines
            let y = 3.0 + 3.5 * i as f32;
            let w = if long { 14.0 } else { 8.0 };
            let x0 = 1.0 + (14.0 - w) * align;
            self.line((x0, y), (x0 + w, y));
        }
    }

    /// A guide line and two bars lined up on it at `at` (0 start, 0.5
    /// centre, 1 end); `vertical` swaps the axes (top/middle/bottom).
    pub(super) fn align(&self, vertical: bool, at: f32) {
        let t = |(x, y): P| if vertical { (y, x) } else { (x, y) };
        let guide = 1.5 + 13.0 * at;
        self.line(t((guide, 1.0)), t((guide, 15.0)));
        for (y0, y1, len) in [(3.0, 7.0, 11.0), (9.0, 13.0, 7.0)] {
            let x0 = if at < 0.25 {
                3.0
            } else if at > 0.75 {
                13.0 - len
            } else {
                8.0 - len / 2.0
            };
            self.fill(t((x0, y0)), t((x0 + len, y1)));
        }
    }

    /// Three bars evenly spaced; `vertical` stacks them.
    pub(super) fn distribute(&self, vertical: bool) {
        let t = |(x, y): P| if vertical { (y, x) } else { (x, y) };
        for x in [1.0, 6.5, 12.0] {
            self.fill(t((x, 4.0)), t((x + 3.0, 12.0)));
        }
    }
}
