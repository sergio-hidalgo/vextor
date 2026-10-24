//! The Image to Vector… dialog (spec 13b): four modes and Keep the image,
//! the real result previewed on the canvas.
//!
//! Tracing runs on a worker thread, every mode once, the one shown first;
//! switching modes then shows a result already there. The preview is
//! applied like Offset… / Simplify… (spec 11): each change takes the last
//! preview back (undo) and applies the new one. OK keeps it as one undo
//! step (and waits while tracing); Cancel, `Esc` or a click outside takes
//! it back.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use eframe::egui::{self, Rect, Vec2};
use vextor_core::{NodeId, Shape};
use vextor_geom::TraceMode;

use crate::image_trace::{self, MODES};
use crate::prompts::{SHAPE_DIALOG_WIDTH, buttons_enabled, dialog_of_width};
use crate::session::Session;

/// The mode the dialog opens on.
const FIRST: TraceMode = TraceMode::BlackAndWhite;
/// How often the dialog looks for the worker's results.
const POLL: Duration = Duration::from_millis(30);

/// One mode's shapes, or why there are none.
type Traced = Result<Vec<Shape>, String>;

/// An open Image to Vector… dialog.
#[derive(Debug)]
pub struct TraceDialog {
    image: NodeId,
    mode: TraceMode,
    keep: bool,
    /// The worker's results so far.
    results: HashMap<TraceMode, Traced>,
    rx: Receiver<(TraceMode, Traced)>,
    /// The mode and Keep the image the canvas shows, once applied.
    shown: Option<(TraceMode, bool)>,
    /// The document holds a preview step to take back.
    applied: bool,
    /// Why the preview could not be applied.
    error: Option<String>,
    /// OK was pressed while tracing: close once the result is in.
    ok_waiting: bool,
}

impl TraceDialog {
    /// Starts tracing the selected image.
    ///
    /// # Errors
    /// Why it cannot: no single image selected, or no thread.
    pub fn open(s: &Session) -> Result<Self, String> {
        let src = image_trace::source(s).map_err(str::to_owned)?;
        let image = src.image;
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("vextor-trace".to_owned())
            .spawn(move || {
                // Stops once the dialog is gone (the receiver dropped).
                image_trace::trace_all(&src, FIRST, |mode, r| tx.send((mode, r)).is_ok());
            })
            .map_err(|e| format!("Couldn't start tracing: {e}"))?;
        Ok(Self::new(image, rx))
    }

    /// The dialog for `image`, its results arriving on `rx`.
    fn new(image: NodeId, rx: Receiver<(TraceMode, Traced)>) -> Self {
        Self {
            image,
            mode: FIRST,
            keep: true,
            results: HashMap::new(),
            rx,
            shown: None,
            applied: false,
            error: None,
            ok_waiting: false,
        }
    }

    /// Shows the dialog centred on `canvas`, previewing on `s`. Returns
    /// true once it is closed (kept or taken back).
    pub fn show(&mut self, ctx: &egui::Context, canvas: Rect, s: &mut Session) -> bool {
        while let Ok((mode, r)) = self.rx.try_recv() {
            self.results.insert(mode, r);
        }
        if self.results.len() < MODES.len() {
            ctx.request_repaint_after(POLL);
        }
        self.preview(s);
        let current = self.results.get(&self.mode);
        let ready = matches!(current, Some(Ok(_))) && self.error.is_none();
        if self.ok_waiting && ready {
            return true;
        }
        if !matches!(current, None | Some(Ok(_))) {
            self.ok_waiting = false;
        }
        let modal = dialog_of_width(ctx, "trace_dialog", canvas, SHAPE_DIALOG_WIDTH, |ui| {
            ui.add_space(4.0);
            ui.heading("Image to Vector");
            ui.add_space(10.0);
            for (mode, name, tooltip) in MODES {
                let choice = egui::Button::selectable(self.mode == mode, name)
                    .min_size(Vec2::new(SHAPE_DIALOG_WIDTH, 24.0));
                if ui.add(choice).on_hover_text(tooltip).clicked() {
                    self.mode = mode;
                }
            }
            ui.add_space(8.0);
            ui.checkbox(&mut self.keep, "Keep the image")
                .on_hover_text("Hidden under the shapes, to compare; show it again from Layers");
            ui.add_space(4.0);
            match (self.results.get(&self.mode), &self.error) {
                (_, Some(e)) | (Some(Err(e)), _) => {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                }
                (None, _) => {
                    ui.weak("Tracing…");
                }
                (Some(Ok(shapes)), _) => {
                    let nodes = image_trace::node_count(shapes);
                    ui.weak(format!("{} shapes, {nodes} nodes", shapes.len()));
                }
            }
            ui.add_space(12.0);
            let can_ok =
                !matches!(self.results.get(&self.mode), Some(Err(_))) && self.error.is_none();
            buttons_enabled(ui, &[("OK", true, can_ok), ("Cancel", false, true)])
        });
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let answer = match modal.inner {
            Some(keep) => Some(keep),
            None if enter => Some(true),
            None if modal.should_close() => Some(false),
            None => None,
        };
        match answer {
            Some(true) if ready => true,
            // still tracing: OK waits for the result
            Some(true) => {
                self.ok_waiting = !self.results.contains_key(&self.mode);
                false
            }
            Some(false) => {
                self.take_back(s);
                true
            }
            None => false,
        }
    }

    /// Puts the current mode on the canvas once its result is in; while it
    /// is not, the canvas shows the image again.
    fn preview(&mut self, s: &mut Session) {
        let want = (self.mode, self.keep);
        if self.shown == Some(want) {
            return;
        }
        self.take_back(s);
        self.shown = None;
        self.error = None;
        let Some(result) = self.results.get(&self.mode) else {
            return;
        };
        if let Ok(shapes) = result {
            let revision = s.revision();
            self.error = image_trace::apply(s, self.image, shapes.clone(), self.keep).err();
            self.applied = s.revision() != revision;
        }
        self.shown = Some(want);
    }

    /// Undoes the preview step, if one is in the document, and selects the
    /// image again.
    fn take_back(&mut self, s: &mut Session) {
        if self.applied {
            s.undo();
            s.select(vec![self.image]);
            self.applied = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::Sender;

    use eframe::egui::{Event, Key, Modifiers, Pos2, RawInput};
    use vextor_core::{Document, NodeKind, Selection};

    use super::*;

    /// A session holding a 40 × 40 image (a black square on white),
    /// selected.
    fn session() -> (Session, NodeId) {
        let rgba: Vec<u8> = (0..40 * 40)
            .flat_map(|i| {
                let (x, y) = (i % 40, i / 40);
                let dark = (10..30).contains(&x) && (10..30).contains(&y);
                if dark {
                    [0, 0, 0, 255]
                } else {
                    [255, 255, 255, 255]
                }
            })
            .collect();
        let png = vextor_render::rgba_png(40, 40, rgba).expect("encodes");
        let imported = vextor_svg::import_image(&png).expect("png");
        let doc = vextor_svg::into_document(&imported, "t").expect("places");
        let image = doc
            .nodes
            .values()
            .find(|n| matches!(n.kind, NodeKind::Image(_)))
            .expect("an image")
            .id;
        let mut s = Session::new(doc);
        s.select(vec![image]);
        (s, image)
    }

    /// A dialog on `s`'s image and the sender of its results.
    fn dialog(s: &Session) -> (TraceDialog, Sender<(TraceMode, Traced)>) {
        let (tx, rx) = mpsc::channel();
        let image = image_trace::traceable(s).expect("an image");
        (TraceDialog::new(image, rx), tx)
    }

    /// Every mode's result, sent as the worker would.
    fn send_all(s: &Session, tx: &Sender<(TraceMode, Traced)>) {
        let src = image_trace::source(s).expect("an image");
        image_trace::trace_all(&src, FIRST, |mode, r| tx.send((mode, r)).is_ok());
    }

    /// One frame of the dialog with `key` pressed; true once closed.
    fn frame(ctx: &egui::Context, d: &mut TraceDialog, s: &mut Session, key: Option<Key>) -> bool {
        let events = key
            .map(|key| Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            })
            .into_iter()
            .collect();
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
            events,
            ..Default::default()
        };
        let mut closed = false;
        let mut output = ctx.run_ui(input, |ui| closed = d.show(ui.ctx(), Rect::NOTHING, s));
        // Headless: no GPU to upload textures to.
        output.textures_delta.clear();
        closed
    }

    fn has_group(doc: &Document) -> bool {
        doc.nodes
            .values()
            .any(|n| matches!(n.kind, NodeKind::Group(_)))
    }

    #[test]
    fn the_canvas_previews_the_result_and_ok_keeps_it_as_one_undo_step() {
        let ctx = egui::Context::default();
        let (mut s, image) = session();
        let before = s.doc.clone();
        let (mut d, tx) = dialog(&s);
        send_all(&s, &tx);
        assert!(!frame(&ctx, &mut d, &mut s, None));
        assert!(has_group(&s.doc), "previewed");
        assert!(!s.doc.nodes[&image].visible, "kept, hidden");
        assert!(frame(&ctx, &mut d, &mut s, Some(Key::Enter)), "OK closes");
        assert!(has_group(&s.doc));
        assert!(s.undo());
        assert_eq!(s.doc, before);
    }

    #[test]
    fn another_mode_replaces_the_preview() {
        let ctx = egui::Context::default();
        let (mut s, _) = session();
        let before = s.doc.clone();
        let (mut d, tx) = dialog(&s);
        send_all(&s, &tx);
        frame(&ctx, &mut d, &mut s, None);
        d.mode = TraceMode::Outlines;
        d.keep = false;
        frame(&ctx, &mut d, &mut s, None);
        let stroked = s
            .doc
            .nodes
            .values()
            .any(|n| matches!(&n.kind, NodeKind::Shape(sh) if sh.style.stroke.is_some()));
        assert!(stroked, "outlines shown");
        assert!(s.undo(), "one step to take back");
        assert_eq!(s.doc, before);
    }

    #[test]
    fn escape_takes_the_preview_back_and_selects_the_image() {
        let ctx = egui::Context::default();
        let (mut s, image) = session();
        let before = s.doc.clone();
        let (mut d, tx) = dialog(&s);
        send_all(&s, &tx);
        frame(&ctx, &mut d, &mut s, None);
        assert!(frame(&ctx, &mut d, &mut s, Some(Key::Escape)), "Esc closes");
        assert_eq!(s.doc, before);
        assert_eq!(s.editor.selection, Selection::Objects(vec![image]));
    }

    #[test]
    fn ok_waits_while_tracing_then_closes_with_the_result() {
        let ctx = egui::Context::default();
        let (mut s, _) = session();
        let (mut d, tx) = dialog(&s);
        assert!(
            !frame(&ctx, &mut d, &mut s, Some(Key::Enter)),
            "nothing to keep yet"
        );
        assert!(!has_group(&s.doc));
        send_all(&s, &tx);
        assert!(frame(&ctx, &mut d, &mut s, None), "closes once traced");
        assert!(has_group(&s.doc));
    }

    #[test]
    fn a_mode_that_fails_shows_why_and_ok_does_nothing() {
        let ctx = egui::Context::default();
        let (mut s, _) = session();
        let (mut d, tx) = dialog(&s);
        tx.send((FIRST, Err("Too detailed".to_owned())))
            .expect("sends");
        frame(&ctx, &mut d, &mut s, None);
        assert!(!frame(&ctx, &mut d, &mut s, Some(Key::Enter)));
        assert!(!has_group(&s.doc));
    }
}
