//! Debug menu: sample documents and canvas timing, for development only.
//!
//! Shown in debug builds, or in a release build started with the
//! `VEXTOR_DEBUG_MENU` environment variable set (to measure performance on
//! the sample documents).

use eframe::egui;
use vextor_core::Document;
use vextor_render::fixtures;

/// Whether the Debug menu is shown.
pub fn enabled() -> bool {
    cfg!(debug_assertions) || std::env::var_os("VEXTOR_DEBUG_MENU").is_some()
}

/// What the user picked in the Debug menu.
pub enum Action {
    /// Open this sample document.
    Load(Box<Document>),
}

/// The Debug menu's contents. `show_stats` is the frame-time toggle.
pub fn menu(ui: &mut egui::Ui, show_stats: &mut bool) -> Option<Action> {
    let mut action = None;
    ui.menu_button("Debug", |ui| {
        if ui.button("Load: basic shapes").clicked() {
            action = Some(Action::Load(Box::new(fixtures::basic_shapes())));
        }
        if ui.button("Load: 500 shapes").clicked() {
            action = Some(Action::Load(Box::new(fixtures::many_nodes(500))));
        }
        if ui.button("Load: 5 000 shapes").clicked() {
            action = Some(Action::Load(Box::new(fixtures::many_nodes(5_000))));
        }
        ui.separator();
        ui.checkbox(show_stats, "Show frame time");
    });
    action
}
