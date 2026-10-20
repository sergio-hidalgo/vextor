//! How path overlays look on the canvas (spec 07): node squares and
//! circles, handles, rings and the "add node" plus. Constant on-screen
//! size at any zoom.

use eframe::egui::{self, Painter, Pos2, Stroke, StrokeKind};

use crate::theme;

/// Side of a drawn node, in points.
const NODE_SIZE: f32 = 7.0;
/// Radius of a drawn handle tip, in points.
const TIP_RADIUS: f32 = 3.0;
/// Radius of the ring around a node a click would close or join.
const RING_RADIUS: f32 = 7.0;

/// A node: a square for a corner, a circle for a smooth or symmetric
/// node; filled when selected.
pub(super) fn node(painter: &Painter, at: Pos2, selected: bool, round: bool) {
    let fill = if selected {
        theme::YELLOW
    } else {
        theme::WHITE
    };
    let stroke = Stroke::new(1.0, theme::LOGO_AMBER);
    if round {
        painter.circle_filled(at, NODE_SIZE / 2.0, fill);
        painter.circle_stroke(at, NODE_SIZE / 2.0, stroke);
    } else {
        let r = egui::Rect::from_center_size(at, egui::vec2(NODE_SIZE, NODE_SIZE));
        painter.rect_filled(r, 0.0, fill);
        painter.rect_stroke(r, 0.0, stroke, StrokeKind::Inside);
    }
}

/// A handle: a thin line from its anchor to a small round tip.
pub(super) fn handle(painter: &Painter, anchor: Pos2, tip: Pos2) {
    painter.line_segment([anchor, tip], Stroke::new(1.0, theme::LOGO_AMBER));
    painter.circle_filled(tip, TIP_RADIUS, theme::WHITE);
    painter.circle_stroke(tip, TIP_RADIUS, Stroke::new(1.0, theme::LOGO_AMBER));
}

/// A ring around a node a click would close the path on, or join.
pub(super) fn ring(painter: &Painter, at: Pos2) {
    painter.circle_stroke(at, RING_RADIUS, Stroke::new(1.5, theme::YELLOW));
}

/// A small plus next to the pointer: a click adds a node here.
pub(super) fn plus(painter: &Painter, at: Pos2) {
    let c = at + egui::vec2(9.0, -9.0);
    let stroke = Stroke::new(1.5, theme::YELLOW);
    painter.line_segment([c - egui::vec2(4.0, 0.0), c + egui::vec2(4.0, 0.0)], stroke);
    painter.line_segment([c - egui::vec2(0.0, 4.0), c + egui::vec2(0.0, 4.0)], stroke);
}

/// A cubic from `p0` to `p3` through controls `p1`, `p2` (screen points).
pub(super) fn curve(painter: &Painter, [p0, p1, p2, p3]: [Pos2; 4], stroke: Stroke) {
    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
        [p0, p1, p2, p3],
        false,
        egui::Color32::TRANSPARENT,
        stroke,
    ));
}
