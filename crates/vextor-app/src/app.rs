//! The editor window: menus, shortcuts, panels and the canvas.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use vextor_core::{Command, Transform};

use crate::canvas::Canvas;
use crate::new_doc::{Outcome, Picker};
use crate::session::Session;
use crate::{debug, panels};

const NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
const UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
const REDO: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);
const FIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0);
const ACTUAL_SIZE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Num1);

/// A user request from a menu or a shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    New,
    Undo,
    Redo,
    Fit,
    ActualSize,
}

/// Editor root.
pub struct VextorApp {
    session: Option<Session>,
    /// The new-document picker, while it is open.
    picker: Option<Picker>,
    canvas: Canvas,
    show_code: bool,
    title: String,
}

impl VextorApp {
    /// Starts with the new-document picker.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // ⌘0 / ⌘+ / ⌘- belong to the canvas, not to egui's UI scale.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        Self {
            session: None,
            picker: Some(Picker::default()),
            canvas: Canvas::default(),
            show_code: true,
            title: String::new(),
        }
    }

    fn open(&mut self, session: Session) {
        self.session = Some(session);
        self.picker = None;
    }

    fn run(&mut self, action: Action) {
        if action == Action::New {
            self.picker = Some(Picker::default());
            return;
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        match action {
            Action::New => {}
            Action::Undo => {
                session.undo();
            }
            Action::Redo => {
                session.redo();
            }
            Action::Fit => self.canvas.fit(session),
            Action::ActualSize => self.canvas.actual_size(session),
        }
    }

    /// Keyboard shortcuts, most specific first (⇧⌘Z before ⌘Z).
    fn shortcuts(ctx: &egui::Context) -> Vec<Action> {
        ctx.input_mut(|i| {
            [
                (REDO, Action::Redo),
                (UNDO, Action::Undo),
                (NEW, Action::New),
                (FIT, Action::Fit),
                (ACTUAL_SIZE, Action::ActualSize),
            ]
            .into_iter()
            .filter(|(shortcut, _)| i.consume_shortcut(shortcut))
            .map(|(_, action)| action)
            .collect()
        })
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let mut actions = Vec::new();
        let ctx = ui.ctx().clone();
        let item = |ui: &mut egui::Ui, enabled: bool, text: &str, shortcut: &KeyboardShortcut| {
            let button = egui::Button::new(text).shortcut_text(ctx.format_shortcut(shortcut));
            ui.add_enabled(enabled, button).clicked()
        };
        let has_doc = self.session.is_some();
        let edit = self.session.as_ref().map(Session::edit_menu);
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if item(ui, true, "New…", &NEW) {
                    actions.push(Action::New);
                }
            });
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
            if debug::enabled() {
                match debug::menu(ui, &mut self.canvas.show_stats) {
                    Some(debug::Action::Load(doc)) => self.open(Session::new(*doc)),
                    Some(debug::Action::NudgeLayer) => nudge_first_layer(self.session.as_mut()),
                    None => {}
                }
            }
        });
        actions
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match &self.session {
            Some(s) => format!("{} — Vextor", s.doc.name),
            None => "Vextor".to_owned(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

impl eframe::App for VextorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        for action in Self::shortcuts(&ctx) {
            self.run(action);
        }

        egui::Panel::top("menu").show(ui, |ui| {
            for action in self.menu_bar(ui) {
                self.run(action);
            }
        });
        egui::Panel::bottom("code")
            .resizable(true)
            .default_size(140.0)
            .show_collapsible(ui, &mut self.show_code, panels::code);
        egui::Panel::left("tools")
            .resizable(false)
            .exact_size(48.0)
            .show(ui, panels::tools);
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(240.0)
            .show(ui, panels::inspector_and_layers);
        egui::CentralPanel::no_frame().show(ui, |ui| {
            if let Some(picker) = &mut self.picker {
                match picker.ui(ui, self.session.is_some()) {
                    Outcome::Open => {}
                    Outcome::Cancel => self.picker = None,
                    Outcome::Create(doc) => self.open(Session::new(*doc)),
                }
            } else if let Some(session) = &mut self.session {
                self.canvas.ui(ui, session);
            }
        });

        self.update_title(&ctx);
    }
}

/// Debug: moves the first layer 10 px right through the history.
fn nudge_first_layer(session: Option<&mut Session>) {
    let Some(session) = session else {
        return;
    };
    let Some(&layer) = session.doc.layers.first() else {
        return;
    };
    let Ok(node) = session.doc.node(layer) else {
        return;
    };
    let transform = node.transform.then(Transform::translate(10.0, 0.0));
    // A layer transform is always valid; a failure would leave nothing changed.
    let _ = session.apply(Command::SetTransform {
        ids: vec![layer],
        transform,
    });
}
