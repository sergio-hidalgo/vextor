//! File ▸ Restore from Backup… (spec 14): list the current file's
//! backups, open one as a new unsaved document, change the settings.

use std::sync::PoisonError;

use eframe::egui;

use super::VextorApp;
use crate::files;
use crate::restore_dialog::{Outcome, RestoreDialog};
use crate::session::Session;

impl VextorApp {
    /// Opens the dialog for the active document.
    pub(super) fn open_restore(&mut self) {
        let (Some(backups), Some(s)) = (&self.backups, self.docs.session()) else {
            return;
        };
        let b = backups.lock().unwrap_or_else(PoisonError::into_inner);
        let list = s.path().map(|p| b.of(p)).unwrap_or_default();
        let file = s.path().map(files::display_name);
        self.restore = Some(RestoreDialog::new(file, list, b.settings(), b.usage()));
    }

    /// The dialog, while it is open. Returns true when it showed.
    pub(super) fn restore_prompt(&mut self, ctx: &egui::Context) -> bool {
        let Some(dialog) = &mut self.restore else {
            return false;
        };
        match dialog.show(ctx, self.canvas_rect) {
            None => {}
            Some(Outcome::Close) => self.restore = None,
            Some(Outcome::Settings(settings)) => {
                if let Some(backups) = &self.backups {
                    let mut b = backups.lock().unwrap_or_else(PoisonError::into_inner);
                    if let Err(e) = b.set_settings(settings) {
                        self.message = Some(format!("Couldn't save the backup settings: {e}"));
                    }
                    dialog.refresh(b.settings(), b.usage());
                }
            }
            Some(Outcome::Open(path)) => {
                self.restore = None;
                self.open_backup(&path);
            }
        }
        true
    }

    /// Opens the backup at `path` as a new document with unsaved changes
    /// and no file, named after the active one.
    fn open_backup(&mut self, path: &std::path::Path) {
        let name = self
            .docs
            .session()
            .map_or_else(String::new, Session::display_name);
        match files::read_document(path) {
            Ok(mut doc) => {
                let stem = name.strip_suffix(".vxtr").unwrap_or(&name);
                doc.name = files::clean_name(&format!("{stem} (backup)"))
                    .unwrap_or_else(|_| "Backup".to_owned());
                let id = self.open(Session::recovered(doc, None));
                self.autosave_now(id);
            }
            Err(e) => self.message = Some(format!("Couldn't open the backup: {e}")),
        }
    }
}
