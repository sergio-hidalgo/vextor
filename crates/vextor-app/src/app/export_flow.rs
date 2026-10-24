//! File ▸ Export… (spec 12): the dialog, then where to write, then the
//! files, each written atomically. The place comes only from the save or
//! folder dialog (`security-check` §4).

use std::path::{Path, PathBuf};

use eframe::egui;

use super::VextorApp;
use crate::export::{self, Kind, What};
use crate::export_dialog::ExportDialog;
use crate::files;

impl VextorApp {
    /// Opens the export dialog with `choice` (File ▸ Export…: the last
    /// choices; the context menu: the selection as SVG).
    pub(super) fn open_export(&mut self, choice: ExportDialog) {
        if self.picker.is_none() && self.docs.session().is_some() {
            self.export_dialog = Some(choice);
        }
    }

    /// The export dialog, while it is open. Returns true when it showed.
    pub(super) fn export_prompt(&mut self, ctx: &egui::Context) -> bool {
        let (Some(dialog), Some(s)) = (&mut self.export_dialog, self.docs.session_mut()) else {
            self.export_dialog = None;
            return false;
        };
        match dialog.show(ctx, self.canvas_rect, s) {
            None => {}
            Some(go) => {
                let choice = *dialog;
                self.export_dialog = None;
                self.last_export = choice;
                if go {
                    self.export(choice);
                }
            }
        }
        true
    }

    /// Makes the files, asks where, writes them, and says what happened.
    fn export(&mut self, choice: ExportDialog) {
        let Some(s) = self.docs.session() else {
            return;
        };
        let made = match export::files(s, choice.what, choice.kind) {
            Ok(made) => made,
            Err(e) => {
                self.message = Some(format!("Couldn't export: {e}"));
                return;
            }
        };
        let result = if choice.what == What::AllArtboards {
            let dialog = rfd::FileDialog::new().set_title("Export to Folder");
            match self.in_current_folder(dialog).pick_folder() {
                Some(dir) => write_into(&dir, &made),
                None => return,
            }
        } else {
            let [file] = made.as_slice() else {
                return;
            };
            let ext = choice.kind.extension();
            let dialog = rfd::FileDialog::new()
                .set_title("Export")
                .add_filter(ext.to_uppercase(), &[ext])
                .set_file_name(&file.name);
            match self.in_current_folder(dialog).save_file() {
                Some(path) => write_file(&with_extension(path, choice.kind), &file.bytes),
                None => return,
            }
        };
        self.message = Some(match result {
            Ok(done) => done,
            Err(e) => format!("Couldn't export: {e}"),
        });
    }
}

/// `path`, with `kind`'s extension added when it has another one or none.
fn with_extension(path: PathBuf, kind: Kind) -> PathBuf {
    let ext = kind.extension();
    let has = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext));
    if has {
        path
    } else {
        let mut s = path.into_os_string();
        s.push(".");
        s.push(ext);
        PathBuf::from(s)
    }
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<String, String> {
    files::write_atomic(path, bytes).map_err(|e| e.to_string())?;
    Ok(format!("Exported “{}”.", files::display_name(path)))
}

/// Writes each file into `dir` (replacing files of the same name, as an
/// export to a folder does) and says how many.
fn write_into(dir: &Path, made: &[export::File]) -> Result<String, String> {
    let mut replaced = 0;
    for file in made {
        let path = dir.join(&file.name);
        if path.exists() {
            replaced += 1;
        }
        files::write_atomic(&path, &file.bytes).map_err(|e| format!("“{}”: {e}", file.name))?;
    }
    let count = match made.len() {
        1 => "1 file".to_owned(),
        n => format!("{n} files"),
    };
    let replaced = match replaced {
        0 => String::new(),
        n => format!(" ({n} replaced)"),
    };
    Ok(format!(
        "Exported {count} to “{}”{replaced}.",
        files::display_name(dir)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::test_dir::TestDir;

    #[test]
    fn a_folder_export_writes_each_file_and_counts_replacements() {
        let dir = TestDir::new();
        let made = vec![
            export::File {
                name: "a.svg".to_owned(),
                bytes: b"<svg/>".to_vec(),
            },
            export::File {
                name: "b.svg".to_owned(),
                bytes: b"<svg/>".to_vec(),
            },
        ];
        std::fs::write(dir.path().join("a.svg"), b"old").expect("writes");
        let said = write_into(dir.path(), &made).expect("writes");
        assert!(said.starts_with("Exported 2 files to"), "{said}");
        assert!(said.ends_with("(1 replaced)."), "{said}");
        assert_eq!(
            std::fs::read(dir.path().join("a.svg")).expect("reads"),
            b"<svg/>"
        );
    }

    #[test]
    fn the_extension_is_added_when_the_name_has_another() {
        assert_eq!(
            with_extension(PathBuf::from("/x/icon"), Kind::Svg),
            PathBuf::from("/x/icon.svg")
        );
        assert_eq!(
            with_extension(PathBuf::from("/x/icon@2x.PNG"), Kind::Png(2)),
            PathBuf::from("/x/icon@2x.PNG")
        );
        assert_eq!(
            with_extension(PathBuf::from("/x/icon.svg"), Kind::Png(1)),
            PathBuf::from("/x/icon.svg.png")
        );
    }
}
