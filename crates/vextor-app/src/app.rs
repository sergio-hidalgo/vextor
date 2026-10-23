//! The editor window: menus, shortcuts, tabs, panels and the canvas.

mod actions;
mod closing;
mod file_flow;
mod keys;
mod menus;
mod paint;
mod view_menu;

use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;

use self::actions::Action;
use self::closing::{Next, Pending};
use crate::canvas::Canvas;
use crate::documents::{DocId, Documents};
use crate::files::{self, RecentFiles};
use crate::fonts::SystemFonts;
use crate::new_doc::{Outcome, Picker};
use crate::recovery::{Orphan, Recovery};
use crate::session::Session;
use crate::swatch_library::Swatches;
use crate::tabs;
use crate::theme;
use crate::{panels, prompts};

/// Where quitting stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Quit {
    Running,
    /// The last "Save changes?" of a quit was answered: close the window
    /// at the end of this frame.
    CloseWindow,
    /// Unsaved changes were dealt with: the window may close.
    Quitting,
}

/// Editor root.
pub struct VextorApp {
    /// The open documents (spec 05f).
    docs: Documents,
    /// The new-document picker, while it is open. It is also the start
    /// screen, shown whenever no document is open.
    picker: Option<Picker>,
    canvas: Canvas,
    /// The tab strip's state (a rename in progress).
    tab_strip: tabs::Strip,
    show_code: bool,
    title: String,
    /// Vextor's own folder; `None` turns off recent files and autosave.
    data_dir: Option<PathBuf>,
    recent: RecentFiles,
    /// The swatch library every document shares (spec 09).
    swatches: Swatches,
    /// Each new document claims an autosave slot (off when the recovery
    /// folder could not be read at launch).
    autosave: bool,
    /// Autosaves from runs that did not exit cleanly, offered one by one.
    orphans: Vec<Orphan>,
    /// Waiting on "Save changes?" about this document.
    confirm: Option<(Pending, DocId)>,
    /// One-line message under the menu bar (failed open/save).
    message: Option<String>,
    quit: Quit,
    /// The canvas area last frame; dialogs centre on it.
    canvas_rect: egui::Rect,
    /// Object ▸ Align ▸ Align to Artboard.
    align_to_artboard: bool,
    /// SVG text to put on the system clipboard this frame.
    system_clipboard: Option<String>,
    /// Fonts for text; the OS fonts arrive from a background thread.
    fonts: SystemFonts,
    /// Waiting on "Convert with the fallback font?" (a selected text's
    /// font is missing).
    outline_prompt: Option<vextor_geom::MissingFont>,
}

impl VextorApp {
    /// Starts with the start screen, and the recovery prompt when a
    /// previous run left unsaved work behind.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // ⌘0 / ⌘+ / ⌘- belong to the canvas, not to egui's UI scale.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        theme::apply(&cc.egui_ctx);
        let fonts = SystemFonts::load(&cc.egui_ctx);
        let data_dir = files::app_data_dir();
        let recent = data_dir
            .as_deref()
            .map(RecentFiles::load)
            .unwrap_or_default();
        let swatches = data_dir.as_deref().map(Swatches::load).unwrap_or_default();
        let mut message = None;
        let (autosave, orphans) = match data_dir.as_deref().map(Recovery::orphans) {
            Some(Ok(o)) => (true, o),
            Some(Err(e)) => {
                message = Some(format!("Autosave is off: {e}"));
                (false, Vec::new())
            }
            None => (false, Vec::new()),
        };
        Self {
            docs: Documents::default(),
            picker: Some(Picker::default()),
            canvas: Canvas::default(),
            tab_strip: tabs::Strip::default(),
            show_code: true,
            title: String::new(),
            data_dir,
            recent,
            swatches,
            autosave,
            orphans,
            confirm: None,
            message,
            quit: Quit::Running,
            canvas_rect: egui::Rect::NOTHING,
            align_to_artboard: false,
            system_clipboard: None,
            fonts,
            outline_prompt: None,
        }
    }

    /// Ends a text edit in the active document, so it is one undo step of
    /// its own before anything else happens (⌘Z then undoes the whole
    /// edit), and before another document becomes active.
    fn finish_text(&mut self) {
        if let Some(s) = self.docs.session_mut() {
            self.canvas.finish_text(s);
        }
    }

    /// Opens `session` in a new tab and makes it active.
    fn open(&mut self, mut session: Session) -> DocId {
        self.finish_text();
        session.fonts = self.fonts.current();
        session.swatches = self.swatches.clone();
        let recovery = self.claim_autosave();
        self.picker = None;
        self.docs.add(session, recovery)
    }

    /// An autosave slot for a new document; `None` when autosave is off.
    fn claim_autosave(&mut self) -> Option<Recovery> {
        if !self.autosave {
            return None;
        }
        match Recovery::claim(self.data_dir.as_deref()?, Instant::now()) {
            Ok(r) => Some(r),
            Err(e) => {
                self.message = Some(format!("Autosave is off for this document: {e}"));
                None
            }
        }
    }

    /// Makes `id` the active document.
    fn activate(&mut self, id: DocId) {
        self.finish_text();
        if self.docs.activate(id) {
            self.picker = None;
        }
    }

    fn run(&mut self, action: Action) {
        if !matches!(
            action,
            Action::Fit
                | Action::ActualSize
                | Action::Zoom { .. }
                | Action::ToggleGrid
                | Action::Snap(_)
        ) {
            self.finish_text();
        }
        if self.pen_action(&action) {
            return;
        }
        match action {
            Action::New => self.picker = Some(Picker::default()),
            Action::Open => self.open_dialog(),
            Action::OpenRecent(path) => self.open_path(path),
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
            // Over an open document, ⌘W closes the new-document picker.
            Action::Close if self.picker.is_some() && !self.docs.is_empty() => {
                self.picker = None;
            }
            Action::Close => {
                if let Some(id) = self.docs.active().map(|t| t.id) {
                    self.request(Pending::Close(id));
                }
            }
            Action::CloseTab(id) => self.request(Pending::Close(id)),
            Action::Activate(id) => self.activate(id),
            Action::RenameDocument(id, name) => self.rename_document(id, &name),
            Action::CycleTab { forward } => {
                self.docs.cycle(forward);
                self.picker = None;
            }
            Action::Undo
            | Action::Redo
            | Action::Fit
            | Action::ActualSize
            | Action::Zoom { .. } => {
                let Some(session) = self.docs.session_mut() else {
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
                    Action::Zoom { inward } => self.canvas.zoom_step(session, inward),
                    _ => self.canvas.actual_size(session),
                }
            }
            _ if self.view_action(&action) => {}
            _ => self.edit(&action),
        }
    }

    /// Takes a close or quit as far as it goes without asking, then asks.
    fn request(&mut self, pending: Pending) {
        let next = pending.next(&mut self.docs);
        self.follow(next);
    }

    /// Does what the close/quit flow says comes next.
    fn follow(&mut self, next: Next) {
        match next {
            Next::Ask(pending, id) => {
                self.activate(id);
                self.confirm = Some((pending, id));
            }
            Next::Done => {}
            Next::Quit => {
                self.quit_now();
                self.quit = Quit::CloseWindow;
            }
        }
    }

    /// The active document's title; "Vextor" when none is open.
    fn update_title(&mut self, ctx: &egui::Context) {
        let title = self
            .docs
            .session()
            .map_or_else(|| "Vextor".to_owned(), Session::title);
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    /// The tab strip, then the start screen or the active document.
    /// Returns what the tabs or the start screen asked for, in order.
    fn central(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
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
        actions
    }
}

impl eframe::App for VextorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.save_swatches();
        if let Some(fonts) = self.fonts.poll() {
            for tab in self.docs.tabs_mut() {
                tab.session.set_fonts(fonts.clone());
            }
        }
        self.handle_close(&ctx);
        let prompt_open =
            self.confirm.is_some() || !self.orphans.is_empty() || self.outline_prompt.is_some();
        self.paint_escape(&ctx);
        // While a shape is drawn, keys belong to it (arrows, Esc).
        if !prompt_open && !self.canvas.is_drawing() {
            // While text is typed, letters and Edit keys belong to it.
            let editing =
                !self.docs.is_empty() && self.picker.is_none() && !self.canvas.is_editing_text();
            for action in Self::shortcuts(&ctx, editing) {
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
        let actions = egui::CentralPanel::no_frame()
            .show(ui, |ui| self.central(ui))
            .inner;
        for action in actions {
            self.run(action);
        }

        self.prompts(&ctx);
        self.autosave_tick(&ctx);
        if let Some(text) = self.system_clipboard.take() {
            ctx.copy_text(text);
        }
        self.update_title(&ctx);
        if self.quit == Quit::CloseWindow {
            self.quit = Quit::Quitting;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl Drop for VextorApp {
    /// However the app ends: each document's unsaved work stays as an
    /// autosave (offered at the next launch); otherwise its slot is removed.
    fn drop(&mut self) {
        let quitting = self.quit != Quit::Running;
        for tab in self.docs.tabs_mut() {
            let Some(r) = &mut tab.recovery else {
                continue;
            };
            let s = &tab.session;
            if s.is_dirty() && !quitting {
                let _ = r.write(&s.doc, s.path(), s.revision(), Instant::now());
            } else {
                r.shutdown();
            }
        }
    }
}
