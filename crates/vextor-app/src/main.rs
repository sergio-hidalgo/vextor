//! Vextor desktop editor: window, tools, panels and input.

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
    eframe::run_native(APP_NAME, options, Box::new(|_cc| Ok(Box::new(VextorApp))))
}

/// Editor root. Empty until spec 03 adds the panels and the canvas.
struct VextorApp;

impl eframe::App for VextorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Frame::central_panel(ui.style()).show(ui, |_ui| {});
    }
}
