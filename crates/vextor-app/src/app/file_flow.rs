//! Opening, saving, recovering and closing documents, the "Save
//! changes?" and recovery prompts, and the autosave timer.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;
use vextor_core::Command;

use super::closing::{Next, Pending};
use super::{Quit, VextorApp};
use crate::documents::DocId;
use crate::files::{self, FileError};
use crate::prompts::{self, RecoverAnswer, SaveAnswer};
use crate::recovery::Orphan;
use crate::saver::{Done, Job};
use crate::session::Session;

impl VextorApp {
    /// File ▸ Open…: asks for a file, then opens it.
    pub(super) fn open_dialog(&mut self) {
        let dialog = rfd::FileDialog::new()
            .set_title("Open")
            .add_filter(
                "Vextor document, SVG or image",
                &[files::EXTENSION, "svg", "png", "jpg", "jpeg", "webp"],
            )
            .add_filter("Vextor document", &[files::EXTENSION])
            .add_filter("SVG", &crate::import::SVG_EXTENSIONS)
            .add_filter("PNG, JPEG or WebP", &crate::import::IMAGE_EXTENSIONS);
        if let Some(path) = self.in_current_folder(dialog).pick_file() {
            self.open_path(path);
        }
    }

    /// Opens `path` in a new tab, or switches to its tab when it is open.
    /// An SVG or image opens as a new untitled document (spec 13).
    pub(super) fn open_path(&mut self, path: PathBuf) {
        use crate::import::FileKind;
        let kind = FileKind::of(&path);
        if matches!(kind, FileKind::Svg | FileKind::Image) {
            self.open_as_document(path, kind);
            return;
        }
        if let Some(id) = self.docs.find_path(&path) {
            self.activate(id);
            self.recent.push(path);
            self.save_recent();
            return;
        }
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

    /// Renames document `id` from its tab: a document not saved yet takes
    /// the name (one undo step, used as the file name on its first save); a
    /// saved one has its file renamed in its folder.
    pub(super) fn rename_document(&mut self, id: DocId, input: &str) {
        let name = match files::clean_name(input) {
            Ok(name) => name,
            Err(why) => {
                self.message = Some(why.to_owned());
                return;
            }
        };
        let Some(tab) = self.docs.get_mut(id) else {
            return;
        };
        let Some(from) = tab.session.path().map(Path::to_path_buf) else {
            if name != tab.session.doc.name
                && let Err(e) = tab.session.apply(Command::SetName(name))
            {
                self.message = Some(format!("Couldn't rename the document: {e}"));
            }
            return;
        };
        let dirty = tab.session.is_dirty();
        // A save in flight would write the old name again.
        if self.saver.is_saving(id) {
            self.finish_saves();
            return self.rename_document(id, input);
        }
        match files::rename_document(&from, &name) {
            Ok(to) if to != from => {
                tab.session.moved_to(to.clone());
                self.recent.remove(&from);
                self.recent.push(to);
                self.save_recent();
                self.message = None;
                // The autosave remembers the file it belongs to.
                if dirty {
                    self.autosave_now(id);
                }
            }
            Ok(_) => {}
            Err(e) => {
                self.message = Some(format!(
                    "Couldn't rename “{}”: {e}",
                    files::display_name(&from)
                ));
            }
        }
    }

    /// Saves the active document to its file, or asks for one. Returns
    /// true when it was saved.
    pub(super) fn save(&mut self) -> bool {
        match self.docs.session().and_then(Session::path) {
            Some(path) => self.save_to(path.to_path_buf()),
            None => self.save_as(),
        }
    }

    /// Asks where to save the active document, then saves. Returns true
    /// when saved.
    pub(super) fn save_as(&mut self) -> bool {
        let Some(session) = self.docs.session() else {
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

    /// Saves the active document to `path` in the background (spec 14):
    /// a snapshot goes to the saving thread and the frame goes on. The
    /// document counts as saved once the file is written and checked
    /// ([`VextorApp::poll_saves`]). Returns true when the save was queued.
    pub(super) fn save_to(&mut self, path: PathBuf) -> bool {
        let Some(tab) = self.docs.active() else {
            return false;
        };
        let job = Job::Save {
            doc: tab.id,
            path,
            document: Box::new(tab.session.doc.clone()),
            revision: tab.session.revision(),
        };
        self.saver.send(job);
        true
    }

    /// Takes in the saves and autosaves that finished.
    pub(super) fn poll_saves(&mut self) {
        let done = self.saver.poll();
        self.took_in(done);
    }

    /// Waits for every save and autosave still running, then takes them
    /// in: before closing or quitting, nothing may be left half done.
    pub(super) fn finish_saves(&mut self) {
        let done = self.saver.wait();
        self.took_in(done);
    }

    fn took_in(&mut self, done: Vec<Done>) {
        for d in done {
            match d {
                Done::Saved {
                    doc,
                    path,
                    revision,
                } => {
                    if let Some(tab) = self.docs.get_mut(doc) {
                        tab.session.mark_saved_at(path.clone(), revision);
                        // Edits made while it saved stay in the autosave.
                        if !tab.session.is_dirty()
                            && let Some(r) = &mut tab.recovery
                        {
                            r.clear(Instant::now());
                        }
                    }
                    self.recent.push(path);
                    self.save_recent();
                    self.message = None;
                }
                Done::Autosaved { doc, revision } => {
                    if let Some(r) = self.docs.get_mut(doc).and_then(|t| t.recovery.as_mut()) {
                        r.written(revision);
                    }
                }
                Done::SaveFailed { error, .. } | Done::AutosaveFailed { error, .. } => {
                    self.message = Some(error);
                }
            }
        }
    }

    /// Whether document `id` was saved, once every save in flight is done.
    pub(super) fn saved_now(&mut self, id: DocId) -> bool {
        self.finish_saves();
        self.docs.get(id).is_some_and(|t| !t.session.is_dirty())
    }

    /// Opens a recovered document in a new tab, autosaves it under this
    /// run at once, then deletes the orphan.
    pub(super) fn recover(&mut self, orphan: &Orphan) {
        match orphan.load() {
            Ok(doc) => {
                let id = self.open(Session::recovered(doc, orphan.original.clone()));
                self.autosave_now(id);
            }
            Err(e) => {
                self.message = Some(format!("Couldn't recover {}: {e}", orphan.name()));
            }
        }
        orphan.discard();
    }

    /// Opens file dialogs in the active document's folder.
    pub(super) fn in_current_folder(&self, dialog: rfd::FileDialog) -> rfd::FileDialog {
        match self
            .docs
            .session()
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

    /// Queues document `id`'s autosave now.
    pub(super) fn autosave_now(&mut self, id: DocId) {
        let Some(tab) = self.docs.get_mut(id) else {
            return;
        };
        let (Some(r), s) = (&mut tab.recovery, &tab.session) else {
            return;
        };
        let job = r.job(&s.doc, s.path(), s.revision(), Instant::now());
        self.saver.send(Job::Autosave {
            doc: id,
            job: Box::new(job),
        });
    }

    /// Autosaves every document that is due, and wakes the app up for the
    /// next one.
    pub(super) fn autosave_tick(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let mut wait: Option<Duration> = None;
        let mut jobs = Vec::new();
        for tab in self.docs.tabs_mut() {
            let (Some(r), s) = (&mut tab.recovery, &tab.session) else {
                continue;
            };
            let (revision, dirty) = (s.revision(), s.is_dirty());
            // Not while a job of the document runs: a save in flight
            // clears the autosave when it lands.
            if self.saver.is_busy(tab.id) {
                continue;
            }
            if r.due(now, revision, dirty) {
                let job = r.job(&s.doc, s.path(), revision, now);
                jobs.push(Job::Autosave {
                    doc: tab.id,
                    job: Box::new(job),
                });
            } else if r.pending(revision, dirty) {
                let left = r.time_left(now);
                wait = Some(wait.map_or(left, |w| w.min(left)));
            }
        }
        for job in jobs {
            self.saver.send(job);
        }
        if let Some(w) = wait {
            ctx.request_repaint_after(w.max(Duration::from_millis(100)));
        }
    }

    /// Keeps each document's latest unsaved work for the panic hook
    /// (spec 14), throttled.
    pub(super) fn keep_latest(&mut self) {
        let now = Instant::now();
        for tab in self.docs.tabs_mut() {
            let (Some(r), s) = (&mut tab.recovery, &tab.session) else {
                continue;
            };
            if s.is_dirty() {
                r.keep_latest(&s.doc, s.path(), s.revision(), now);
            }
        }
    }

    /// Unsaved changes were dealt with: deletes every autosave and lets
    /// the window close.
    pub(super) fn quit_now(&mut self) {
        self.finish_saves();
        self.quit = Quit::Quitting;
        self.confirm = None;
        for tab in self.docs.tabs_mut() {
            if let Some(r) = &mut tab.recovery {
                r.shutdown();
            }
        }
    }

    /// The window's close button (or ⌘Q): asks "Save changes?" about each
    /// document with unsaved changes first.
    pub(super) fn handle_close(&mut self, ctx: &egui::Context) {
        if self.quit != Quit::Running || !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        self.finish_saves();
        match (Pending::Quit {
            dont_save: Vec::new(),
        })
        .next(&mut self.docs)
        {
            Next::Quit => self.quit_now(),
            next => {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.follow(next);
            }
        }
    }

    /// "Convert with the fallback font?", "Save changes?" and the recovery
    /// prompt, one at a time.
    pub(super) fn prompts(&mut self, ctx: &egui::Context) {
        if self.path_prompts(ctx) || self.export_prompt(ctx) || self.restore_prompt(ctx) {
            return;
        }
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
                    && let Some(s) = self.docs.session_mut()
                    && let Err(e) = crate::outline_text::convert(s)
                {
                    self.message = Some(e);
                }
            }
            return;
        }
        if let Some((pending, id)) = self.confirm.take() {
            let name = self
                .docs
                .get(id)
                .map_or_else(String::new, |t| t.session.display_name());
            match prompts::save_changes(ctx, self.canvas_rect, &name) {
                None => self.confirm = Some((pending, id)),
                Some(answer) => {
                    // Save acts on the active document: the one asked about.
                    self.docs.activate(id);
                    let saved = answer == SaveAnswer::Save && self.save() && self.saved_now(id);
                    let next = pending.answer(&mut self.docs, id, answer, saved);
                    self.follow(next);
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
                self.recover(&orphan);
            }
            Some(RecoverAnswer::Discard) => self.orphans.remove(0).discard(),
        }
    }

    /// The path operations' dialog or text prompt (spec 11), when one is
    /// open. Returns true when one was shown.
    fn path_prompts(&mut self, ctx: &egui::Context) -> bool {
        if let Some(dialog) = &mut self.trace_dialog {
            if let Some(s) = self.docs.session_mut() {
                if dialog.show(ctx, self.canvas_rect, s) {
                    self.trace_dialog = None;
                }
            } else {
                self.trace_dialog = None;
            }
            return true;
        }
        if let Some(dialog) = &mut self.path_dialog {
            if let Some(s) = self.docs.session_mut() {
                if dialog.show(ctx, self.canvas_rect, s) {
                    self.path_dialog = None;
                }
            } else {
                self.path_dialog = None;
            }
            return true;
        }
        let Some(op) = self.shape_prompt else {
            return false;
        };
        let missing = self
            .docs
            .session()
            .and_then(crate::outline_text::missing_font);
        if let Some(go) =
            crate::path_dialog::outlines_first(ctx, self.canvas_rect, op, missing.as_ref())
        {
            self.shape_prompt = None;
            if go
                && let Some(s) = self.docs.session_mut()
                && let Err(e) = op.run(s)
            {
                self.message = Some(e);
            }
        }
        true
    }
}
