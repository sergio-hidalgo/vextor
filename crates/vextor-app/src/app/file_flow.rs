//! Opening, saving, recovering and closing documents: everything that
//! may need "Save changes?" first, and the autosave timer.

use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;

use super::VextorApp;
use crate::files::{self, FileError};
use crate::new_doc::Picker;
use crate::prompts::{self, RecoverAnswer, SaveAnswer};
use crate::recovery::Orphan;
use crate::session::Session;

/// Something that replaces or closes the current document, so it waits
/// for "Save changes?" when there are unsaved changes.
#[derive(Debug)]
pub(super) enum Then {
    New,
    Open,
    OpenPath(PathBuf),
    Recover(Orphan),
    Quit,
}

impl VextorApp {
    /// Does `then` now, or first asks "Save changes?" when the document
    /// has unsaved changes.
    pub(super) fn request(&mut self, then: Then) {
        if self.session.as_ref().is_some_and(Session::is_dirty) {
            self.confirm = Some(then);
        } else {
            self.proceed(then);
        }
    }

    pub(super) fn proceed(&mut self, then: Then) {
        match then {
            Then::New => self.picker = Some(Picker::default()),
            Then::Open => {
                let dialog = rfd::FileDialog::new()
                    .set_title("Open")
                    .add_filter("Vextor document", &[files::EXTENSION]);
                if let Some(path) = self.in_current_folder(dialog).pick_file() {
                    self.open_path(path);
                }
            }
            Then::OpenPath(path) => self.open_path(path),
            Then::Recover(orphan) => self.recover(&orphan),
            Then::Quit => {
                self.quitting = true;
                if let Some(r) = &mut self.recovery {
                    r.shutdown();
                }
                self.confirm = None;
            }
        }
    }

    pub(super) fn open_path(&mut self, path: PathBuf) {
        match files::read_document(&path) {
            Ok(doc) => {
                self.open(Session::opened(doc, path.clone()));
                self.recent.push(path);
                self.message = None;
            }
            Err(e) => {
                if matches!(&e, FileError::Io(io) if io.kind() == io::ErrorKind::NotFound) {
                    self.recent.remove(&path);
                }
                self.message = Some(format!(
                    "Couldn't open “{}”: {e}",
                    files::display_name(&path)
                ));
            }
        }
        self.save_recent();
    }

    /// Saves to the document's file, or asks for one. Returns true when
    /// the document was saved.
    pub(super) fn save(&mut self) -> bool {
        match self.session.as_ref().and_then(Session::path) {
            Some(path) => self.save_to(path.to_path_buf()),
            None => self.save_as(),
        }
    }

    /// Asks where to save, then saves. Returns true when saved.
    pub(super) fn save_as(&mut self) -> bool {
        let Some(session) = &self.session else {
            return false;
        };
        let stem = session.path().and_then(|p| p.file_stem()).map_or_else(
            || session.doc.name.clone(),
            |s| s.to_string_lossy().into_owned(),
        );
        let dialog = rfd::FileDialog::new()
            .set_title("Save As")
            .add_filter("Vextor document", &[files::EXTENSION])
            .set_file_name(format!("{stem}.{}", files::EXTENSION));
        match self.in_current_folder(dialog).save_file() {
            Some(path) => self.save_to(files::with_extension(path)),
            None => false,
        }
    }

    pub(super) fn save_to(&mut self, path: PathBuf) -> bool {
        let Some(session) = &mut self.session else {
            return false;
        };
        if let Err(e) = files::write_document(&path, &session.doc) {
            self.message = Some(format!(
                "Couldn't save “{}”: {e}",
                files::display_name(&path)
            ));
            return false;
        }
        session.mark_saved(path.clone());
        if let Some(r) = &mut self.recovery {
            r.clear(Instant::now());
        }
        self.recent.push(path);
        self.save_recent();
        self.message = None;
        true
    }

    /// Opens a recovered document, autosaves it under this run at once,
    /// then deletes the orphan.
    pub(super) fn recover(&mut self, orphan: &Orphan) {
        match orphan.load() {
            Ok(doc) => {
                self.open(Session::recovered(doc, orphan.original.clone()));
                self.autosave_now();
            }
            Err(e) => {
                self.message = Some(format!("Couldn't recover {}: {e}", orphan.name()));
            }
        }
        orphan.discard();
    }

    /// Opens file dialogs in the current document's folder.
    pub(super) fn in_current_folder(&self, dialog: rfd::FileDialog) -> rfd::FileDialog {
        match self
            .session
            .as_ref()
            .and_then(Session::path)
            .and_then(|p| p.parent())
        {
            Some(dir) => dialog.set_directory(dir),
            None => dialog,
        }
    }

    pub(super) fn save_recent(&self) {
        if let Some(dir) = &self.data_dir {
            // The list is a convenience; failing to save it is not worth a message.
            let _ = self.recent.save(dir);
        }
    }

    pub(super) fn autosave_now(&mut self) {
        let (Some(r), Some(s)) = (&mut self.recovery, &self.session) else {
            return;
        };
        if let Err(e) = r.write(&s.doc, s.path(), s.revision(), Instant::now()) {
            self.message = Some(format!("Autosave failed: {e}"));
        }
    }

    /// Autosaves when due, and wakes the app up for the next one.
    pub(super) fn autosave_tick(&mut self, ctx: &egui::Context) {
        let (Some(r), Some(s)) = (&self.recovery, &self.session) else {
            return;
        };
        let now = Instant::now();
        if r.due(now, s.revision(), s.is_dirty()) {
            self.autosave_now();
        } else if r.pending(s.revision(), s.is_dirty()) {
            ctx.request_repaint_after(r.time_left(now).max(Duration::from_millis(100)));
        }
    }

    /// The window's close button (or ⌘Q): asks first when there are
    /// unsaved changes.
    pub(super) fn handle_close(&mut self, ctx: &egui::Context) {
        if self.quitting || !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        if self.session.as_ref().is_some_and(Session::is_dirty) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(Then::Quit);
        } else {
            self.proceed(Then::Quit);
        }
    }

    /// "Save changes?" and the recovery prompt, one at a time.
    pub(super) fn prompts(&mut self, ctx: &egui::Context) {
        if let Some(missing) = &self.outline_prompt {
            let answer = prompts::outline_missing_font(
                ctx,
                self.canvas_rect,
                &missing.wanted,
                missing.shown.as_deref(),
            );
            if let Some(convert) = answer {
                self.outline_prompt = None;
                if convert
                    && let Some(s) = self.session.as_mut()
                    && let Err(e) = crate::outline_text::convert(s)
                {
                    self.message = Some(e);
                }
            }
            return;
        }
        if let Some(then) = &self.confirm {
            let name = self
                .session
                .as_ref()
                .map_or_else(String::new, Session::display_name);
            let quit = matches!(then, Then::Quit);
            let answer = prompts::save_changes(ctx, self.canvas_rect, &name);
            let then = match answer {
                None => return,
                Some(SaveAnswer::Cancel) => None,
                Some(SaveAnswer::Save) if !self.save() => None,
                Some(SaveAnswer::Save | SaveAnswer::DontSave) => self.confirm.take(),
            };
            self.confirm = None;
            if let Some(then) = then {
                self.proceed(then);
                if quit {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            return;
        }
        let Some(orphan) = self.orphans.first() else {
            return;
        };
        match prompts::recover(ctx, self.canvas_rect, &orphan.name()) {
            None => {}
            Some(RecoverAnswer::Recover) => {
                let orphan = self.orphans.remove(0);
                self.request(Then::Recover(orphan));
            }
            Some(RecoverAnswer::Discard) => self.orphans.remove(0).discard(),
        }
    }
}
