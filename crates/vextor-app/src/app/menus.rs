//! The menu bar: File, Edit, Object (`object_menu.rs`), View and Window.

use eframe::egui::{self, KeyboardShortcut};

use super::keys::{
    self, ACTUAL_SIZE, CLOSE, COPY, COPY_AS_SVG, CUT, DUPLICATE, EXIT, EXPORT, FIT, IMPORT, NEW,
    OPEN, PASTE, PASTE_IN_PLACE, SAVE, SAVE_AS, SELECT_ALL, UNDO, ZOOM_IN, ZOOM_OUT,
};
use super::{Action, VextorApp};
use crate::session::Session;
use crate::{edit_ops, files};

impl VextorApp {
    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let mut actions = Vec::new();
        let ctx = ui.ctx().clone();
        let item = |ui: &mut egui::Ui, enabled: bool, text: &str, shortcut: &KeyboardShortcut| {
            let button = egui::Button::new(text).shortcut_text(keys::label(&ctx, *shortcut));
            ui.add_enabled(enabled, button).clicked()
        };
        let os = ctx.os();
        let has_doc = !self.docs.is_empty() && self.picker.is_none();
        let edit = self.docs.session().map(Session::edit_menu);
        let can = match self.docs.session() {
            Some(s) if has_doc => edit_ops::can(s),
            _ => edit_ops::Can::default(),
        };
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| self.file_menu(ui, has_doc, &mut actions));
            ui.menu_button("Edit", |ui| {
                let undo = edit.and_then(|e| e.undo);
                let redo = edit.and_then(|e| e.redo);
                let undo_text = undo.map_or_else(|| "Undo".to_owned(), |l| format!("Undo {l}"));
                let redo_text = redo.map_or_else(|| "Redo".to_owned(), |l| format!("Redo {l}"));
                if item(ui, undo.is_some(), &undo_text, &UNDO) {
                    actions.push(Action::Undo);
                }
                if item(ui, redo.is_some(), &redo_text, &keys::redo(os)[0]) {
                    actions.push(Action::Redo);
                }
                ui.separator();
                let select_first = "Select an object first";
                let edit_items = [
                    ("Cut", CUT, Action::Cut, can.edit, select_first),
                    ("Copy", COPY, Action::Copy, can.edit, select_first),
                    (
                        "Paste",
                        PASTE,
                        Action::Paste,
                        can.paste,
                        "Copy something first",
                    ),
                    (
                        "Paste in Place",
                        PASTE_IN_PLACE,
                        Action::PasteInPlace,
                        can.paste,
                        "Copy something first",
                    ),
                    (
                        "Duplicate",
                        DUPLICATE,
                        Action::Duplicate,
                        can.edit,
                        select_first,
                    ),
                    (
                        "Delete",
                        keys::delete(os),
                        Action::Delete,
                        can.edit,
                        select_first,
                    ),
                ];
                for (text, shortcut, action, enabled, why) in edit_items {
                    if why_item(ui, enabled, text, shortcut, why) {
                        actions.push(action);
                    }
                    if text == "Copy" {
                        copy_as_menu(ui, can.edit, &mut actions);
                    }
                }
                ui.separator();
                if item(ui, has_doc, "Select All", &SELECT_ALL) {
                    actions.push(Action::SelectAll);
                }
            });
            ui.menu_button("Object", |ui| {
                self.object_menu(ui, can, &mut actions);
            });
            ui.menu_button("View", |ui| {
                if item(ui, has_doc, "Fit Artboard", &FIT) {
                    actions.push(Action::Fit);
                }
                if item(ui, has_doc, "Actual Size", &ACTUAL_SIZE) {
                    actions.push(Action::ActualSize);
                }
                if item(ui, has_doc, "Zoom In", &ZOOM_IN) {
                    actions.push(Action::Zoom { inward: true });
                }
                if item(ui, has_doc, "Zoom Out", &ZOOM_OUT) {
                    actions.push(Action::Zoom { inward: false });
                }
                ui.separator();
                self.grid_items(ui, has_doc, &mut actions);
                ui.separator();
                ui.checkbox(&mut self.show_code, "Code Panel");
            });
            ui.menu_button("Window", |ui| self.window_menu(ui, &mut actions));
        });
        actions
    }

    /// File: New, Open, Open Recent, Close, Save, Save As, and Exit off
    /// macOS (spec 14c), where the app menu has Quit.
    fn file_menu(&self, ui: &mut egui::Ui, has_doc: bool, actions: &mut Vec<Action>) {
        let items = [
            ("New…", NEW, Action::New, true),
            ("Open…", OPEN, Action::Open, true),
            ("Import…", IMPORT, Action::Import, true),
        ];
        for (text, shortcut, action, enabled) in items {
            if why_item(ui, enabled, text, shortcut, "") {
                actions.push(action);
            }
        }
        self.recent_menu(ui, actions);
        ui.separator();
        let items = [
            ("Close", CLOSE, Action::Close),
            ("Save", SAVE, Action::Save),
            ("Save As…", SAVE_AS, Action::SaveAs),
            ("Export…", EXPORT, Action::Export),
        ];
        for (text, shortcut, action) in items {
            if why_item(ui, has_doc, text, shortcut, "No document is open") {
                actions.push(action);
            }
        }
        ui.separator();
        let why = if self.backups.is_none() {
            "Vextor has no app-data folder for backups"
        } else {
            "No document is open"
        };
        let can = has_doc && self.backups.is_some();
        if ui
            .add_enabled(can, egui::Button::new("Restore from Backup…"))
            .on_disabled_hover_text(why)
            .clicked()
        {
            actions.push(Action::RestoreBackup);
        }
        if !ui.ctx().os().is_mac() {
            ui.separator();
            // Like the window's close button: "Save changes?" first.
            if why_item(ui, true, "Exit", EXIT, "") {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Window: next / previous document, then every open document (✓ on
    /// the active one).
    fn window_menu(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let many = self.docs.tabs().len() > 1;
        let [next, prev] = keys::cycle_docs(ui.ctx().os());
        let cycle = [
            ("Next Document", next, true),
            ("Previous Document", prev, false),
        ];
        for (text, shortcut, forward) in cycle {
            if why_item(ui, many, text, shortcut, "Open another document first") {
                actions.push(Action::CycleTab { forward });
            }
        }
        if self.docs.is_empty() {
            return;
        }
        ui.separator();
        let active = self.docs.active().map(|t| t.id);
        for tab in self.docs.tabs() {
            let mark = if active == Some(tab.id) {
                "✓ "
            } else {
                "    "
            };
            if ui
                .button(format!("{mark}{}", tab.session.display_name()))
                .clicked()
            {
                actions.push(Action::Activate(tab.id));
            }
        }
    }

    /// File ▸ Open Recent: file names, full path on hover.
    fn recent_menu(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let paths = self.recent.paths();
        ui.add_enabled_ui(!paths.is_empty(), |ui| {
            ui.menu_button("Open Recent", |ui| {
                for path in paths {
                    let button = ui
                        .button(files::display_name(path))
                        .on_hover_text(path.display().to_string());
                    if button.clicked() {
                        actions.push(Action::OpenRecent(path.clone()));
                    }
                }
                ui.separator();
                if ui.button("Clear Menu").clicked() {
                    actions.push(Action::ClearRecent);
                }
            });
        });
    }
}

/// Edit ▸ Copy As (spec 12): the selection's code in each format, for the
/// system clipboard.
fn copy_as_menu(ui: &mut egui::Ui, enabled: bool, actions: &mut Vec<Action>) {
    ui.add_enabled_ui(enabled, |ui| {
        ui.menu_button("Copy As", |ui| {
            for format in vextor_svg::Format::ALL {
                let clicked = if format == vextor_svg::Format::Svg {
                    why_item(ui, true, format.title(), COPY_AS_SVG, "")
                } else {
                    ui.button(format.title()).clicked()
                };
                if clicked {
                    actions.push(Action::CopyAs(format));
                }
            }
        });
    })
    .response
    .on_disabled_hover_text("Select an object first");
}

/// A menu item that says why it is disabled when it is.
pub(super) fn why_item(
    ui: &mut egui::Ui,
    enabled: bool,
    text: &str,
    shortcut: KeyboardShortcut,
    why: &str,
) -> bool {
    let button = egui::Button::new(text).shortcut_text(keys::label(ui.ctx(), shortcut));
    ui.add_enabled(enabled, button)
        .on_disabled_hover_text(why)
        .clicked()
}
