//! The eyedropper on the canvas (spec 09): armed by a colour editor's
//! pipette button, it shows a crosshair and a hint; a click takes the
//! colour of the rendered pixel under the pointer (what you see, over
//! transparency where nothing is drawn) for that editor. `Esc` (handled
//! with the app's keys) or the button again puts it away.

use eframe::egui::{self, Painter};

use super::Canvas;
use crate::session::Session;

impl Canvas {
    /// While the eyedropper is armed, takes the canvas input: returns
    /// true so no tool sees the click.
    pub(super) fn eyedropper_input(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        session: &mut Session,
    ) -> bool {
        let Some(key) = session.eyedropper else {
            return false;
        };
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let ppp = ui.ctx().pixels_per_point();
            let local = (p - rect.min) * ppp;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // in the canvas
            let (x, y) = (local.x.max(0.0) as u32, local.y.max(0.0) as u32);
            if let Some(c) = self.cache.pixel(x, y) {
                session.picked = Some((key, c));
            }
            session.eyedropper = None;
        }
        true
    }

    /// The eyedropper's hint at the top of the canvas while it is armed.
    pub(super) fn draw_eyedropper_hint(
        ui: &egui::Ui,
        painter: &Painter,
        rect: egui::Rect,
        session: &Session,
    ) {
        if session.eyedropper.is_none() {
            return;
        }
        painter.text(
            rect.center_top() + egui::vec2(0.0, 10.0),
            egui::Align2::CENTER_TOP,
            "Click to pick a colour · Esc cancels",
            egui::FontId::proportional(13.0),
            ui.visuals().strong_text_color(),
        );
    }
}
