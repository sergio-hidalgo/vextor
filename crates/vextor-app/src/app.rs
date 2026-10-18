//! The editor window: menus, shortcuts, panels and the canvas.

mod file_flow;

use std::path::PathBuf;
use std::time::Instant;

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use vextor_core::{Command, Transform};

use self::file_flow::Then;
use crate::canvas::Canvas;
use crate::files::{self, RecentFiles};
use crate::new_doc::{Outcome, Picker};
use crate::recovery::{Orphan, Recovery};
use crate::session::Session;
use crate::{debug, panels, prompts};

const NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
const SAVE_AS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);
const UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
const REDO: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);
const FIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0);
const ACTUAL_SIZE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Num1);

/// A user request from a menu or a shortcut.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    New,
    Open,
    OpenRecent(PathBuf),
    ClearRecent,
    Save,
    SaveAs,
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
    /// Vextor's own folder; `None` turns off recent files and autosave.
    data_dir: Option<PathBuf>,
    recent: RecentFiles,
    recovery: Option<Recovery>,
    /// Autosaves from runs that did not exit cleanly; the first is offered.
    orphans: Vec<Orphan>,
    /// Waiting on "Save changes?" before doing this.
    confirm: Option<Then>,
    /// One-line message under the menu bar (failed open/save).
    message: Option<String>,
    /// The window may close: unsaved changes were dealt with.
    quitting: bool,
    /// The canvas area last frame; dialogs centre on it.
    canvas_rect: egui::Rect,
}

impl VextorApp {
    /// Starts with the new-document picker, and the recovery prompt when a
    /// previous run left unsaved work behind.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // ⌘0 / ⌘+ / ⌘- belong to the canvas, not to egui's UI scale.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let data_dir = files::app_data_dir();
        let recent = data_dir
            .as_deref()
            .map(RecentFiles::load)
            .unwrap_or_default();
        let mut message = None;
        let (recovery, orphans) = match data_dir
            .as_deref()
            .map(|d| Recovery::start(d, Instant::now()))
        {
            Some(Ok((r, o))) => (Some(r), o),
            Some(Err(e)) => {
                message = Some(format!("Autosave is off: {e}"));
                (None, Vec::new())
            }
            None => (None, Vec::new()),
        };
        Self {
            session: None,
            picker: Some(Picker::default()),
            canvas: Canvas::default(),
            show_code: true,
            title: String::new(),
            data_dir,
            recent,
            recovery,
            orphans,
            confirm: None,
            message,
            quitting: false,
            canvas_rect: egui::Rect::NOTHING,
        }
    }

    /// Replaces the current document (already confirmed) with `session`.
    fn open(&mut self, session: Session) {
        if let Some(r) = &mut self.recovery {
            r.clear(Instant::now());
        }
        self.session = Some(session);
        self.picker = None;
    }

    fn run(&mut self, action: Action) {
        match action {
            Action::New => self.request(Then::New),
            Action::Open => self.request(Then::Open),
            Action::OpenRecent(path) => self.request(Then::OpenPath(path)),
            Action::ClearRecent => {
                self.recent.clear();
                self.save_recent();
            }
            Action::Save => {
                self.save();
            }
            Action::SaveAs => {
                self.save_as();
            }
            Action::Undo | Action::Redo | Action::Fit | Action::ActualSize => {
                let Some(session) = self.session.as_mut() else {
                    return;
                };
                match action {
                    Action::Undo => {
                        session.undo();
                    }
                    Action::Redo => {
                        session.redo();
                    }
                    Action::Fit => self.canvas.fit(session),
                    _ => self.canvas.actual_size(session),
                }
            }
        }
    }

    /// Keyboard shortcuts, most specific first (⇧⌘Z before ⌘Z).
    fn shortcuts(ctx: &egui::Context) -> Vec<Action> {
        ctx.input_mut(|i| {
            [
                (REDO, Action::Redo),
                (UNDO, Action::Undo),
                (NEW, Action::New),
                (OPEN, Action::Open),
                (SAVE_AS, Action::SaveAs),
                (SAVE, Action::Save),
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
                if item(ui, true, "Open…", &OPEN) {
                    actions.push(Action::Open);
                }
                self.recent_menu(ui, &mut actions);
                ui.separator();
                if item(ui, has_doc, "Save", &SAVE) {
                    actions.push(Action::Save);
                }
                if item(ui, has_doc, "Save As…", &SAVE_AS) {
                    actions.push(Action::SaveAs);
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

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = self
            .session
            .as_ref()
            .map_or_else(|| "Vextor".to_owned(), Session::title);
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

impl eframe::App for VextorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_close(&ctx);
        let prompt_open = self.confirm.is_some() || !self.orphans.is_empty();
        if !prompt_open {
            for action in Self::shortcuts(&ctx) {
                self.run(action);
            }
        }

        egui::Panel::top("menu").show(ui, |ui| {
            for action in self.menu_bar(ui) {
                self.run(action);
            }
        });
        prompts::message_bar(ui, &mut self.message);
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
            self.canvas_rect = ui.max_rect();
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

        self.prompts(&ctx);
        self.autosave_tick(&ctx);
        self.update_title(&ctx);
    }
}

impl Drop for VextorApp {
    /// However the app ends: unsaved work stays as an autosave (offered at
    /// the next launch); otherwise the autosave slot is removed.
    fn drop(&mut self) {
        if self.session.as_ref().is_some_and(Session::is_dirty) && !self.quitting {
            self.autosave_now();
        } else if let Some(r) = &mut self.recovery {
            r.shutdown();
        }
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
