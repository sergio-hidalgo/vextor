//! Vextor desktop editor: window, tools, panels and input.

mod app;
mod artboard;
mod backups;
#[cfg(test)]
mod bundle_icons;
mod canvas;
mod code_panel;
mod convert_path;
mod corners;
mod documents;
mod edit_ops;
mod emergency;
mod export;
mod export_dialog;
mod files;
mod fonts;
mod icon;
mod icons;
mod image_trace;
mod import;
mod layers;
mod new_doc;
mod node_tool;
mod outline_text;
mod paint_edit;
mod panels;
mod path_dialog;
mod path_ops;
mod pen_tool;
mod prompts;
mod recovery;
mod restore_dialog;
mod safe_save;
mod saver;
mod select;
mod session;
mod shape_ops;
mod shape_tool;
mod snap_settings;
mod snapping;
mod swatch_library;
mod tabs;
mod text_edit;
mod text_tool;
mod theme;
mod thumbnails;
mod trace_dialog;
mod view;
mod wordmark;

use eframe::egui;

/// Window title, also used as the app name for eframe's persisted state.
const APP_NAME: &str = "Vextor";

fn main() -> eframe::Result {
    emergency::install();
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
