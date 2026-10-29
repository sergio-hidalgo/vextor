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
mod launch;
mod launch_card;
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
#[cfg(test)]
mod quicklook_samples;
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
    let started = std::time::Instant::now();
    emergency::install();
    // The window opens as the launch card and becomes the editor (spec 14e).
    let mut viewport = launch_card::viewport(APP_NAME);
    if let Some(icon) = icon::app_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };
    let paths = launch::launch_paths(std::env::args_os().skip(1));
    let (to_open, ctx_for_hook) = open_queue(paths);
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| {
            let _ = ctx_for_hook.set(cc.egui_ctx.clone());
            Ok(Box::new(app::VextorApp::new(cc, to_open, started)))
        }),
    )
}

/// Where the UI context goes once the window is up, so the macOS hook can
/// wake the frame loop when files arrive.
type ContextSlot = std::sync::Arc<std::sync::OnceLock<egui::Context>>;

/// The files to open: `paths` from the command line, then what Finder sends
/// (spec 14b).
#[cfg(target_os = "macos")]
fn open_queue(paths: Vec<std::path::PathBuf>) -> (launch::OpenQueue, ContextSlot) {
    let slot = ContextSlot::default();
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = slot.clone();
    vextor_macos::install(tx, move || {
        if let Some(ctx) = ctx.get() {
            ctx.request_repaint();
        }
    });
    (launch::OpenQueue::new(paths, Some(rx)), slot)
}

/// The files to open: `paths` from the command line (spec 14b).
#[cfg(not(target_os = "macos"))]
fn open_queue(paths: Vec<std::path::PathBuf>) -> (launch::OpenQueue, ContextSlot) {
    (launch::OpenQueue::new(paths, None), ContextSlot::default())
}
