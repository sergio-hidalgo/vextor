//! Small line icons drawn with egui's painter (no icon font), for icon
//! buttons in the inspector. Every icon fits a 16 × 16 pt square.

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Response, Stroke, Vec2};

use crate::theme;

/// Side of the square an icon is drawn in, in points.
pub const SIZE: f32 = 16.0;
/// Size of an icon button, in points.
const BUTTON: Vec2 = Vec2::new(28.0, 24.0);
/// Line width of an icon, in points.
const LINE: f32 = 1.5;

/// The icons Vextor draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)] // only text icons until spec 05d adds more
pub enum Icon {
    /// Text alignment: lines flush left / centred / flush right.
    TextLeft,
    TextCentre,
    TextRight,
}

/// A button showing `icon`, highlighted when `selected`, explained by
/// `tooltip` on hover.
pub fn button(ui: &mut egui::Ui, icon: Icon, selected: bool, tooltip: &str) -> Response {
    let r = ui
        .add(egui::Button::selectable(selected, "").min_size(BUTTON))
        .on_hover_text(tooltip);
    let color = if selected {
        theme::YELLOW
    } else if r.hovered() {
        theme::WHITE
    } else {
        theme::GREY_100
    };
    let square = Rect::from_center_size(r.rect.center(), Vec2::splat(SIZE));
    paint(ui.painter(), square, icon, color);
    r
}

/// Draws `icon` into `rect` (a [`SIZE`] square) in `color`.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let stroke = Stroke::new(LINE, color);
    // Four lines, long and short in turn, 3.5 pt apart.
    let lines = |align: f32| {
        for (i, long) in [true, false, true, false].into_iter().enumerate() {
            #[allow(clippy::cast_precision_loss)] // four lines
            let y = rect.top() + 3.0 + 3.5 * i as f32;
            let w = if long { 14.0 } else { 8.0 };
            let x0 = rect.left() + 1.0 + (14.0 - w) * align;
            painter.line_segment([Pos2::new(x0, y), Pos2::new(x0 + w, y)], stroke);
        }
    };
    match icon {
        Icon::TextLeft => lines(0.0),
        Icon::TextCentre => lines(0.5),
        Icon::TextRight => lines(1.0),
    }
}
