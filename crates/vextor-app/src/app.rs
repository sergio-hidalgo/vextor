//! The editor window: menus, shortcuts, panels and the canvas.

mod file_flow;
mod keys;
mod menus;

use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;
use vextor_core::{Arrange, ToolId};
use vextor_geom::Align;

use self::file_flow::Then;
use crate::canvas::Canvas;
use crate::files::{self, RecentFiles};
use crate::fonts::SystemFonts;
use crate::new_doc::{Outcome, Picker};
use crate::recovery::{Orphan, Recovery};
use crate::session::Session;
use crate::theme;
use crate::{edit_ops, panels, prompts};

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
    Cut,
    Copy,
    Paste,
    PasteInPlace,
    Duplicate,
    Delete,
    SelectAll,
    Deselect,
    Group,
    Ungroup,
    Arrange(Arrange),
    Align(Align),
    Distribute { horizontal: bool },
    Flip { horizontal: bool },
    Nudge { dx: i32, dy: i32 },
    Tool(ToolId),
    OutlineText,
}

impl From<panels::PanelAction> for Action {
    /// The menu action an inspector button stands for.
    fn from(a: panels::PanelAction) -> Self {
        match a {
            panels::PanelAction::Align(how) => Self::Align(how),
            panels::PanelAction::Distribute { horizontal } => Self::Distribute { horizontal },
            panels::PanelAction::Group => Self::Group,
            panels::PanelAction::Ungroup => Self::Ungroup,
        }
    }
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
    /// Starts with the new-document picker, and the recovery prompt when a
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
            align_to_artboard: false,
            system_clipboard: None,
            fonts,
            outline_prompt: None,
        }
    }

    /// Replaces the current document (already confirmed) with `session`.
    fn open(&mut self, mut session: Session) {
        if let Some(r) = &mut self.recovery {
            r.clear(Instant::now());
        }
        // The clipboard outlives the document, so copies paste elsewhere.
        if let Some(old) = &mut self.session {
            session.editor.clipboard = old.editor.clipboard.take();
        }
        session.fonts = self.fonts.current();
        self.canvas.cancel_rename();
        self.session = Some(session);
        self.picker = None;
    }

    fn run(&mut self, action: Action) {
        // Anything but a view change ends a text edit first, so it is one
        // undo step of its own (⌘Z then undoes the whole edit).
        if !matches!(action, Action::Fit | Action::ActualSize)
            && let Some(s) = self.session.as_mut()
        {
            self.canvas.finish_text(s);
        }
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
            _ => self.edit(&action),
        }
    }

    /// Edit and Object actions on the open document's selection.
    fn edit(&mut self, action: &Action) {
        let Some(s) = self.session.as_mut() else {
            return;
        };
        if self.picker.is_some() {
            return;
        }
        let result = match action {
            Action::Cut => edit_ops::cut(s).map(|svg| self.system_clipboard = svg),
            Action::Copy => {
                self.system_clipboard = edit_ops::copy(s);
                Ok(())
            }
            Action::Paste => edit_ops::paste(s, false),
            Action::PasteInPlace => edit_ops::paste(s, true),
            Action::Duplicate => edit_ops::duplicate(s),
            Action::Delete => edit_ops::delete(s),
            Action::SelectAll => {
                edit_ops::select_all(s);
                Ok(())
            }
            Action::Deselect => {
                s.select(Vec::new());
                Ok(())
            }
            Action::Group => edit_ops::group(s),
            Action::Ungroup => edit_ops::ungroup(s),
            Action::Arrange(how) => edit_ops::arrange(s, *how),
            Action::Align(how) => edit_ops::align(s, *how, self.align_to_artboard),
            Action::Distribute { horizontal } => edit_ops::distribute(s, *horizontal),
            Action::Flip { horizontal } => edit_ops::flip(s, *horizontal),
            Action::Nudge { dx, dy } => edit_ops::nudge(s, f64::from(*dx), f64::from(*dy)),
            Action::Tool(tool) => {
                s.editor.tool = *tool;
                Ok(())
            }
            Action::OutlineText => match crate::outline_text::missing_font(s) {
                // Ask first: the fallback face would be outlined.
                Some(missing) => {
                    self.outline_prompt = Some(missing);
                    Ok(())
                }
                None => crate::outline_text::convert(s),
            },
            _ => Ok(()),
        };
        if let Err(e) = result {
            self.message = Some(e);
        }
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
        if let Some(fonts) = self.fonts.poll()
            && let Some(s) = &mut self.session
        {
            s.set_fonts(fonts);
        }
        self.handle_close(&ctx);
        let prompt_open =
            self.confirm.is_some() || !self.orphans.is_empty() || self.outline_prompt.is_some();
        if !prompt_open {
            // While text is typed, letters and Edit keys belong to it.
            let editing =
                self.session.is_some() && self.picker.is_none() && !self.canvas.is_editing_text();
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
                    .session
                    .as_ref()
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
                if ui.input(|i| i.pointer.primary_pressed())
                    && ui.ui_contains_pointer()
                    && let Some(s) = self.session.as_mut()
                {
                    self.canvas.finish_text(s);
                }
                let session = self.session.as_mut().filter(|_| self.picker.is_none());
                let (message, clicked) =
                    panels::inspector_and_layers(ui, session, &mut self.align_to_artboard);
                if let Some(e) = message {
                    self.message = Some(e);
                }
                for action in clicked {
                    self.run(Action::from(action));
                }
            });
        egui::CentralPanel::no_frame().show(ui, |ui| {
            self.canvas_rect = ui.max_rect();
            if let Some(picker) = &mut self.picker {
                match picker.ui(ui, self.session.is_some()) {
                    Outcome::Open => {}
                    Outcome::Cancel => self.picker = None,
                    Outcome::Create(doc) => self.open(Session::new(*doc)),
                }
            } else if let Some(session) = &mut self.session
                && let Some(e) = self.canvas.ui(ui, session)
            {
                self.message = Some(e);
            }
        });

        self.prompts(&ctx);
        self.autosave_tick(&ctx);
        if let Some(text) = self.system_clipboard.take() {
            ctx.copy_text(text);
        }
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
