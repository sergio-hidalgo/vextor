//! Vextor desktop editor: window, tools, panels and input.

mod app;
mod canvas;
mod debug;
mod edit_ops;
mod files;
mod fonts;
mod icon;
mod new_doc;
mod outline_text;
mod panels;
mod prompts;
mod recovery;
mod select;
mod session;
mod text_edit;
mod text_tool;
mod theme;
mod view;
mod wordmark;

use eframe::egui;

/// Window title, also used as the app name for eframe's persisted state.
const APP_NAME: &str = "Vextor";

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([800.0, 500.0]);
    if let Some(icon) = icon::app_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(app::VextorApp::new(cc)))),
    )
}
