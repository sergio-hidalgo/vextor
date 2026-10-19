//! The menu bar: File, Edit, Object, View, Window and (debug builds) Debug.

use eframe::egui::{self, KeyboardShortcut};
use vextor_core::Arrange;
use vextor_geom::Align;

use super::keys::{
    ACTUAL_SIZE, BACK, BACKWARD, CLOSE, COPY, CUT, DELETE, DUPLICATE, FIT, FORWARD, FRONT, GROUP,
    NEW, NEXT_DOC, OPEN, OUTLINE_TEXT, PASTE, PASTE_IN_PLACE, PREV_DOC, REDO, SAVE, SAVE_AS,
    SELECT_ALL, UNDO, UNGROUP,
};
use super::{Action, VextorApp};
use crate::session::Session;
use crate::{debug, edit_ops, files};

impl VextorApp {
    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let mut actions = Vec::new();
        let ctx = ui.ctx().clone();
        let item = |ui: &mut egui::Ui, enabled: bool, text: &str, shortcut: &KeyboardShortcut| {
            let button = egui::Button::new(text).shortcut_text(ctx.format_shortcut(shortcut));
            ui.add_enabled(enabled, button).clicked()
        };
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
                if item(ui, redo.is_some(), &redo_text, &REDO) {
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
                    ("Delete", DELETE, Action::Delete, can.edit, select_first),
                ];
                for (text, shortcut, action, enabled, why) in edit_items {
                    if why_item(ui, enabled, text, shortcut, why) {
                        actions.push(action);
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
                ui.separator();
                ui.checkbox(&mut self.show_code, "Code Panel");
            });
            ui.menu_button("Window", |ui| self.window_menu(ui, &mut actions));
            if debug::enabled()
                && let Some(debug::Action::Load(doc)) = debug::menu(ui, &mut self.canvas.show_stats)
            {
                self.open(Session::new(*doc));
            }
        });
        actions
    }

    /// Object: group, arrange, align, distribute, flip.
    fn object_menu(&mut self, ui: &mut egui::Ui, can: edit_ops::Can, actions: &mut Vec<Action>) {
        let select_first = "Select an object first";
        if why_item(
            ui,
            can.group,
            "Group",
            GROUP,
            "Select objects in the same group or layer",
        ) {
            actions.push(Action::Group);
        }
        if why_item(ui, can.ungroup, "Ungroup", UNGROUP, "Select a group") {
            actions.push(Action::Ungroup);
        }
        ui.separator();
        if why_item(
            ui,
            can.outline,
            "Convert Text to Outlines",
            OUTLINE_TEXT,
            "Select a text",
        ) {
            actions.push(Action::OutlineText);
        }
        ui.separator();
        ui.add_enabled_ui(can.edit, |ui| {
            ui.menu_button("Arrange", |ui| arrange_menu(ui, actions));
            ui.menu_button("Align", |ui| {
                align_menu(ui, &mut self.align_to_artboard, actions);
            });
        })
        .response
        .on_disabled_hover_text(select_first);
        ui.add_enabled_ui(can.distribute, |ui| {
            ui.menu_button("Distribute", |ui| {
                if ui.button("Horizontally").clicked() {
                    actions.push(Action::Distribute { horizontal: true });
                }
                if ui.button("Vertically").clicked() {
                    actions.push(Action::Distribute { horizontal: false });
                }
            });
        })
        .response
        .on_disabled_hover_text("Select three or more objects");
        ui.separator();
        for (text, horizontal) in [("Flip Horizontal", true), ("Flip Vertical", false)] {
            let button = ui
                .add_enabled(can.edit, egui::Button::new(text))
                .on_disabled_hover_text(select_first);
            if button.clicked() {
                actions.push(Action::Flip { horizontal });
            }
        }
    }

    /// File: New, Open, Open Recent, Close, Save, Save As.
    fn file_menu(&self, ui: &mut egui::Ui, has_doc: bool, actions: &mut Vec<Action>) {
        let items = [
            ("New…", NEW, Action::New, true),
            ("Open…", OPEN, Action::Open, true),
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
        ];
        for (text, shortcut, action) in items {
            if why_item(ui, has_doc, text, shortcut, "No document is open") {
                actions.push(action);
            }
        }
    }

    /// Window: next / previous document, then every open document (✓ on
    /// the active one).
    fn window_menu(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let many = self.docs.tabs().len() > 1;
        let cycle = [
            ("Next Document", NEXT_DOC, true),
            ("Previous Document", PREV_DOC, false),
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

/// Object ▸ Arrange.
fn arrange_menu(ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    let items = [
        ("Bring to Front", FRONT, Arrange::Front),
        ("Bring Forward", FORWARD, Arrange::Forward),
        ("Send Backward", BACKWARD, Arrange::Backward),
        ("Send to Back", BACK, Arrange::Back),
    ];
    for (text, shortcut, how) in items {
        if why_item(ui, true, text, shortcut, "") {
            actions.push(Action::Arrange(how));
        }
    }
}

/// Object ▸ Align: the six lines, then the artboard toggle.
fn align_menu(ui: &mut egui::Ui, to_artboard: &mut bool, actions: &mut Vec<Action>) {
    let items = [
        ("Left", Align::Left),
        ("Centre", Align::Centre),
        ("Right", Align::Right),
        ("Top", Align::Top),
        ("Middle", Align::Middle),
        ("Bottom", Align::Bottom),
    ];
    for (i, (text, how)) in items.into_iter().enumerate() {
        if i == 3 {
            ui.separator();
        }
        if ui.button(text).clicked() {
            actions.push(Action::Align(how));
        }
    }
    ui.separator();
    ui.checkbox(to_artboard, "Align to Artboard")
        .on_hover_text("One selected object always aligns to the artboard");
}

/// A menu item that says why it is disabled when it is.
fn why_item(
    ui: &mut egui::Ui,
    enabled: bool,
    text: &str,
    shortcut: KeyboardShortcut,
    why: &str,
) -> bool {
    let button = egui::Button::new(text).shortcut_text(ui.ctx().format_shortcut(&shortcut));
    ui.add_enabled(enabled, button)
        .on_disabled_hover_text(why)
        .clicked()
}
