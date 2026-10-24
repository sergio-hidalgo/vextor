//! The window's panels around the canvas: code (bottom), tools (left),
//! inspector and layers (right), and the centre (tabs, then the start
//! screen or the canvas).

use eframe::egui;

use super::{Action, VextorApp};
use crate::canvas::context_menu::MenuRequest;
use crate::new_doc::{Outcome, Picker};
use crate::panels;
use crate::session::Session;

impl VextorApp {
    /// The code panel (spec 12), collapsible from View ▸ Code Panel.
    pub(super) fn code_panel_ui(&mut self, ui: &mut egui::Ui) {
        let code_panel = &mut self.code_panel;
        let tab = self.docs.active_mut().filter(|_| self.picker.is_none());
        egui::Panel::bottom("code")
            .resizable(true)
            .default_size(160.0)
            .show_collapsible(ui, &mut self.show_code, |ui| {
                code_panel.ui(ui, tab.map(|t| (t.id, &mut t.session)));
            });
    }

    /// The tools strip.
    pub(super) fn tools_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("tools")
            .resizable(false)
            .exact_size(48.0)
            .show(ui, |ui| {
                let active = self
                    .docs
                    .session()
                    .filter(|_| self.picker.is_none())
                    .map(|s| s.editor.tool);
                if let Some(tool) = panels::tools(ui, active) {
                    self.run(Action::Tool(tool));
                }
            });
    }

    /// Inspector and Layers.
    pub(super) fn inspector_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(280.0)
            .show(ui, |ui| {
                // Working in the inspector ends a text edit first, so the
                // edit stays one undo step and the fields apply to it.
                if ui.input(|i| i.pointer.primary_pressed()) && ui.ui_contains_pointer() {
                    self.finish_text();
                }
                let tab = self.docs.active_mut().filter(|_| self.picker.is_none());
                // Each document gets its own widget ids, so a field being
                // typed in never carries its draft into another document.
                let id = tab.as_ref().map(|t| t.id);
                let session = tab.map(|t| &mut t.session);
                let (message, clicked) = ui
                    .push_id(id, |ui| {
                        panels::inspector_and_layers(ui, session, &mut self.align_to_artboard)
                    })
                    .inner;
                if let Some(e) = message {
                    self.message = Some(e);
                }
                for action in clicked {
                    self.run(Action::from(action));
                }
            });
    }

    /// The tab strip, then the start screen or the active document.
    /// Returns what the tabs or the start screen asked for, in order.
    pub(super) fn central(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let mut actions = Vec::new();
        if !self.docs.is_empty() {
            egui::Panel::top("tabs").show(ui, |ui| {
                let clicked = self.tab_strip.ui(ui, &self.docs);
                actions.extend(clicked.into_iter().map(Action::from));
            });
        }
        self.canvas_rect = ui.available_rect_before_wrap();
        if self.docs.is_empty() && self.picker.is_none() {
            self.picker = Some(Picker::default());
        }
        if let Some(picker) = &mut self.picker {
            let fonts = self.fonts.current();
            match picker.ui(ui, !self.docs.is_empty(), self.recent.paths(), fonts.db()) {
                Outcome::Open => {}
                Outcome::Cancel => self.picker = None,
                Outcome::Create(doc) if picker.adding => self.add_artboard(&doc),
                Outcome::Create(doc) => {
                    self.open(Session::new(*doc));
                }
                Outcome::Browse => actions.push(Action::Open),
                Outcome::OpenRecent(path) => actions.push(Action::OpenRecent(path)),
            }
        } else if let Some(tab) = self.docs.active_mut()
            && let Some(e) = self.canvas.ui(ui, tab.id, &mut tab.session)
        {
            self.message = Some(e);
        }
        // The canvas context menu's path operations and Export Selection…
        // run like the same items in the menu bar (prompts, dialogs).
        actions.extend(self.canvas.take_request().map(|r| match r {
            MenuRequest::Shape(op) => Action::Shape(op),
            MenuRequest::ExportSelection => Action::ExportSelection,
        }));
        actions
    }
}
