//! The editor window: menus, shortcuts, tabs, panels and the canvas.

mod actions;
mod closing;
mod export_flow;
mod file_flow;
mod import_flow;
mod keys;
mod layout;
mod menus;
mod object_menu;
mod paint;
mod shape_menu;
mod view_menu;

use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;

use self::actions::Action;
use self::closing::{Next, Pending};
use crate::canvas::Canvas;
use crate::code_panel::CodePanel;
use crate::documents::{DocId, Documents};
use crate::export_dialog::ExportDialog;
use crate::files::{self, RecentFiles};
use crate::fonts::SystemFonts;
use crate::new_doc::Picker;
use crate::prompts;
use crate::recovery::{Orphan, Recovery};
use crate::session::Session;
use crate::swatch_library::Swatches;
use crate::tabs;
use crate::theme;

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
    /// The bottom code panel (spec 12).
    code_panel: CodePanel,
    /// File ▸ Export…, while open (spec 12).
    export_dialog: Option<ExportDialog>,
    /// The choices the export dialog opens with: the last ones used.
    last_export: ExportDialog,
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
    /// The text vextor last put on the system clipboard: ⌘V of that same
    /// text pastes the internal clipboard instead of importing it.
    last_copied: Option<String>,
    /// Imports being parsed (spec 13).
    imports: Vec<import_flow::Job>,
    /// Fonts for text; the OS fonts arrive from a background thread.
    fonts: SystemFonts,
    /// Waiting on "Convert with the fallback font?" (a selected text's
    /// font is missing).
    outline_prompt: Option<vextor_geom::MissingFont>,
    /// Waiting on "Convert text to outlines first?" before this path
    /// operation (spec 11).
    shape_prompt: Option<crate::shape_ops::ShapeOp>,
    /// Offset Path… or Simplify…, while open (spec 11).
    path_dialog: Option<crate::path_dialog::PathDialog>,
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
            code_panel: CodePanel::default(),
            export_dialog: None,
            last_export: ExportDialog::default(),
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
            last_copied: None,
            imports: Vec::new(),
            fonts,
            outline_prompt: None,
            shape_prompt: None,
            path_dialog: None,
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
            Action::Import => self.import_dialog(),
            Action::PasteText(text) => self.paste_text(text),
            Action::PasteImage {
                width,
                height,
                rgba,
            } => self.paste_image(width, height, rgba),
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
            Action::Export => self.open_export(self.last_export),
            Action::ExportSelection => self.open_export(ExportDialog {
                what: crate::export::What::Selection,
                kind: crate::export::Kind::Svg,
            }),
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
        self.poll_imports(&ctx);
        self.handle_drops(&ctx);
        let prompt_open = self.confirm.is_some()
            || !self.orphans.is_empty()
            || self.outline_prompt.is_some()
            || self.shape_prompt.is_some()
            || self.path_dialog.is_some()
            || self.export_dialog.is_some();
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
        self.code_panel_ui(ui);
        self.tools_panel(ui);
        self.inspector_panel(ui);
        let actions = egui::CentralPanel::no_frame()
            .show(ui, |ui| self.central(ui))
            .inner;
        for action in actions {
            self.run(action);
        }

        self.drop_hint(&ctx);
        self.prompts(&ctx);
        self.autosave_tick(&ctx);
        if let Some(text) = self.system_clipboard.take() {
            self.last_copied = Some(text.clone());
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
