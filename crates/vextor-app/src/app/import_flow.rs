//! Import (spec 13): File ▸ Import…, File ▸ Open of an SVG, ⌘V of SVG
//! text and files dropped on the window. Parsing runs on its own thread
//! (a large file never stalls a frame); the result lands in the document
//! it was meant for, if that is still open.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use eframe::egui;
use vextor_core::Point;
use vextor_svg::Imported;

use super::VextorApp;
use crate::documents::DocId;
use crate::files;
use crate::import::{self, FileKind, IMAGE_EXTENSIONS, PasteChoice, SVG_EXTENSIONS};
use crate::view;

/// usvg recurses once per nesting level (bounded at 128 by the import
/// guard); a roomy stack keeps that far from the end.
const STACK: usize = 32 << 20;

/// Where an import goes once parsed.
#[derive(Clone, Debug)]
pub(super) enum Target {
    /// A new untitled document, named after the file.
    NewDocument(PathBuf),
    /// Into document `doc`, centred at `at` (document space).
    Place {
        doc: DocId,
        at: Point,
        name: Option<String>,
        label: &'static str,
    },
}

/// An import being parsed.
pub(super) struct Job {
    target: Target,
    rx: Receiver<Result<Imported, String>>,
}

/// What an import reads.
enum Source {
    File(PathBuf, FileKind),
    Text(String),
    /// Straight RGBA pixels from the clipboard.
    Pixels {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
}

impl VextorApp {
    /// File ▸ Import…: asks for an SVG or image, then places it in the
    /// active document (or opens it as a new one when none is open).
    pub(super) fn import_dialog(&mut self) {
        let mut all: Vec<&str> = SVG_EXTENSIONS.to_vec();
        all.extend(IMAGE_EXTENSIONS);
        let dialog = rfd::FileDialog::new()
            .set_title("Import")
            .add_filter("SVG or image", &all)
            .add_filter("SVG", &SVG_EXTENSIONS)
            .add_filter("PNG, JPEG or WebP", &IMAGE_EXTENSIONS);
        if let Some(path) = self.in_current_folder(dialog).pick_file() {
            let at = self.view_centre();
            self.import_file(path, at);
        }
    }

    /// Imports the file at `path` into the active document centred at
    /// `at`; with no document open, as a new document.
    pub(super) fn import_file(&mut self, path: PathBuf, at: Option<Point>) {
        let kind = FileKind::of(&path);
        if matches!(kind, FileKind::Document | FileKind::Other) {
            self.message = Some(format!(
                "Couldn't import “{}”: only SVG, PNG, JPEG and WebP files can be imported.",
                files::display_name(&path)
            ));
            return;
        }
        let target = match (self.active_doc(), at) {
            (Some(doc), Some(at)) => Target::Place {
                doc,
                at,
                name: name_of(&path),
                label: "Import",
            },
            _ => Target::NewDocument(path.clone()),
        };
        self.start(Source::File(path, kind), target);
    }

    /// File ▸ Open of an SVG or an image: a new untitled document.
    pub(super) fn open_as_document(&mut self, path: PathBuf, kind: FileKind) {
        let target = Target::NewDocument(path.clone());
        self.start(Source::File(path, kind), target);
    }

    /// ⌘V with `text` on the system clipboard.
    pub(super) fn paste_text(&mut self, text: String) {
        let internal = self
            .docs
            .session()
            .is_some_and(|s| s.editor.clipboard.is_some());
        let files = || clipboard_files().unwrap_or_default();
        match import::paste_choice(&text, self.last_copied.as_deref(), internal, files) {
            PasteChoice::Internal => self.edit(&super::Action::Paste),
            PasteChoice::Files(paths) => {
                let at = self.view_centre();
                if at.is_none() {
                    return;
                }
                for path in paths {
                    self.import_file(path, at);
                }
            }
            PasteChoice::Svg => {
                let (Some(doc), Some(at)) = (self.active_doc(), self.view_centre()) else {
                    return;
                };
                let target = Target::Place {
                    doc,
                    at,
                    name: None,
                    label: "Paste SVG",
                };
                self.start(Source::Text(text), target);
            }
        }
    }

    /// ⌘V with a bitmap on the clipboard: an embedded PNG in the middle of
    /// the view ("Paste image").
    pub(super) fn paste_image(&mut self, width: u32, height: u32, rgba: Vec<u8>) {
        let (Some(doc), Some(at)) = (self.active_doc(), self.view_centre()) else {
            return;
        };
        let target = Target::Place {
            doc,
            at,
            name: None,
            label: "Paste image",
        };
        self.start(
            Source::Pixels {
                width,
                height,
                rgba,
            },
            target,
        );
    }

    /// Files dropped on the window this frame: documents open, SVGs and
    /// images are imported where they were dropped.
    pub(super) fn handle_drops(&mut self, ctx: &egui::Context) {
        let (dropped, pointer) =
            ctx.input(|i| (i.raw.dropped_files.clone(), i.pointer.hover_pos()));
        if dropped.is_empty() {
            return;
        }
        let at = pointer
            .filter(|p| self.canvas_rect.contains(*p))
            .and_then(|p| self.doc_point(p))
            .or_else(|| self.view_centre());
        for file in dropped {
            let path = file.path().to_path_buf();
            if path.as_os_str().is_empty() {
                continue;
            }
            match FileKind::of(&path) {
                FileKind::Document => self.open_path(path),
                _ => self.import_file(path, at),
            }
        }
    }

    /// While files are dragged over the window: a frame around the canvas
    /// saying they can be dropped.
    pub(super) fn drop_hint(&self, ctx: &egui::Context) {
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if !hovering || !self.canvas_rect.is_positive() {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("drop-hint"),
        ));
        let rect = self.canvas_rect.shrink(4.0);
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(2.0, crate::theme::YELLOW),
            egui::StrokeKind::Inside,
        );
        let text = if self.active_doc().is_some() {
            "Drop to import"
        } else {
            "Drop to open"
        };
        painter.text(
            rect.center_top() + egui::vec2(0.0, 16.0),
            egui::Align2::CENTER_TOP,
            text,
            egui::FontId::proportional(14.0),
            crate::theme::YELLOW,
        );
    }

    /// Lands every import that finished parsing; asks for a repaint while
    /// some are still running.
    pub(super) fn poll_imports(&mut self, ctx: &egui::Context) {
        let mut done = Vec::new();
        self.imports.retain(|job| match job.rx.try_recv() {
            Ok(result) => {
                done.push((job.target.clone(), result));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => {
                done.push((
                    job.target.clone(),
                    Err("Couldn't import: it stopped.".to_owned()),
                ));
                false
            }
        });
        for (target, result) in done {
            self.land(target, result);
        }
        if !self.imports.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }

    fn land(&mut self, target: Target, result: Result<Imported, String>) {
        let imported = match result {
            Ok(i) => i,
            Err(e) => {
                self.message = Some(e);
                return;
            }
        };
        match target {
            Target::NewDocument(path) => match import::document(&imported, &path) {
                Ok(session) => {
                    self.open(session);
                    self.message = (!imported.report.is_clean()).then(|| imported.report.summary());
                }
                Err(e) => self.message = Some(e),
            },
            Target::Place {
                doc,
                at,
                name,
                label,
            } => {
                self.finish_text();
                let Some(tab) = self.docs.tabs_mut().iter_mut().find(|t| t.id == doc) else {
                    return;
                };
                match import::place(&mut tab.session, imported, at, name.as_deref(), label) {
                    Ok(message) => self.message = message,
                    Err(e) => self.message = Some(e),
                }
            }
        }
    }

    fn start(&mut self, source: Source, target: Target) {
        let fonts = Arc::clone(&self.fonts.current());
        let (tx, rx) = mpsc::channel();
        let spawned = thread::Builder::new()
            .name("vextor-import".to_owned())
            .stack_size(STACK)
            .spawn(move || {
                let result = match source {
                    Source::File(path, kind) => import::read(&path, kind, &fonts),
                    Source::Text(text) => import::parse(text.as_bytes(), FileKind::Svg, &fonts)
                        .map_err(|e| format!("Couldn't paste the SVG: {e}")),
                    Source::Pixels {
                        width,
                        height,
                        rgba,
                    } => import::pasted_image(width, height, rgba),
                };
                // The receiver is gone if the app closed first.
                let _ = tx.send(result);
            });
        match spawned {
            Ok(_) => self.imports.push(Job { target, rx }),
            Err(e) => self.message = Some(format!("Couldn't import: {e}")),
        }
    }

    /// The active document, when it shows (not under the picker).
    fn active_doc(&self) -> Option<DocId> {
        if self.picker.is_some() {
            return None;
        }
        self.docs.active().map(|t| t.id)
    }

    /// The document point under screen position `p` on the canvas.
    fn doc_point(&self, p: egui::Pos2) -> Option<Point> {
        let s = self.docs.session()?;
        let local = p - self.canvas_rect.min;
        Some(view::screen_to_doc(
            s.editor.view,
            Point::new(f64::from(local.x), f64::from(local.y)),
        ))
    }

    /// The document point at the centre of the canvas.
    fn view_centre(&self) -> Option<Point> {
        self.active_doc()?;
        self.doc_point(self.canvas_rect.center())
    }
}

/// The files on the system clipboard (copied in Finder / Explorer).
fn clipboard_files() -> Option<Vec<PathBuf>> {
    arboard::Clipboard::new().ok()?.get().file_list().ok()
}

/// The group name an imported file's objects get: its file name.
fn name_of(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .and_then(vextor_core::clean_name)
}
