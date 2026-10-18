//! Vextor desktop editor: window, tools, panels and input.

mod app;
mod canvas;
mod debug;
mod new_doc;
mod panels;
mod session;
mod view;

use eframe::egui;

/// Window title, also used as the app name for eframe's persisted state.
const APP_NAME: &str = "Vextor";

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(app::VextorApp::new(cc)))),
    )
}
