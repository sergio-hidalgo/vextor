//! The app's part in painting (spec 09): saving the swatch library, and
//! the Esc presses that belong to the eyedropper or a bad hex value
//! before the shortcuts see them.

use eframe::egui;

use super::VextorApp;
use crate::panels;

impl VextorApp {
    /// Saves the swatch library when last frame changed it: it belongs to
    /// the app, so it is kept whether or not any document is saved.
    /// Without an app-data folder it lasts until Vextor quits.
    pub(super) fn save_swatches(&mut self) {
        if self.swatches.take_changed()
            && let Some(dir) = &self.data_dir
            && let Err(e) = self.swatches.save(dir)
        {
            self.message = Some(format!("Swatches could not be saved: {e}"));
        }
    }

    /// Esc first puts an armed eyedropper away, then puts back a colour
    /// whose typed hex does not read (even after its field lost focus), so
    /// neither press deselects. Runs before the shortcuts.
    pub(super) fn paint_escape(&mut self, ctx: &egui::Context) {
        if let Some(tab) = self.docs.active_mut()
            && tab.session.eyedropper.is_some()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            tab.session.eyedropper = None;
        }
        panels::escape_bad_hex(ctx);
    }
}
