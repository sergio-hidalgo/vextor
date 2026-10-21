//! The zoom tool (`Z`, spec 08b): a click zooms in one stop about the
//! click, an `⌥`-click zooms out, a drag zooms to fit the dragged
//! rectangle. It never edits the document.

use eframe::egui::{self, Painter, Pos2, Stroke, StrokeKind};
use vextor_core::ToolId;

use super::{Canvas, to_point, to_vec2};
use crate::session::Session;
use crate::{theme, view};

/// Movement before a press becomes a drag, in screen points (the tools'
/// threshold): less is a click.
const DRAG_THRESHOLD: f32 = 3.0;

/// A zoom tool press in progress: where it started and where the pointer
/// is now, in screen points.
#[derive(Clone, Copy, Debug)]
pub(super) struct ZoomDrag {
    start: Pos2,
    now: Pos2,
}

impl ZoomDrag {
    fn is_drag(self) -> bool {
        self.start.distance(self.now) >= DRAG_THRESHOLD
    }
}

impl Canvas {
    /// The zoom tool's pointer input (not while panning). Returns false
    /// when another tool is on, leaving the input to it.
    pub(super) fn zoom_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        session: &mut Session,
    ) -> bool {
        if session.editor.tool != ToolId::Zoom {
            self.zoom_drag = None;
            return false;
        }
        let (pressed, down, press_at, at, out, space) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.press_origin(),
                i.pointer.interact_pos(),
                i.modifiers.alt,
                i.key_down(egui::Key::Space),
            )
        });
        if response.hovered() && !space {
            ui.ctx().set_cursor_icon(if out {
                egui::CursorIcon::ZoomOut
            } else {
                egui::CursorIcon::ZoomIn
            });
        }
        if pressed
            && response.hovered()
            && !space
            && let Some(p) = press_at
        {
            self.zoom_drag = Some(ZoomDrag { start: p, now: p });
        }
        let Some(drag) = &mut self.zoom_drag else {
            return true;
        };
        if let Some(p) = at {
            drag.now = p;
        }
        if down {
            return true;
        }
        let drag = *drag;
        self.zoom_drag = None;
        let local = |p: Pos2| to_point(p - rect.min);
        let v = session.editor.view;
        session.editor.view = if drag.is_drag() {
            view::zoom_rect(v, local(drag.start), local(drag.now), to_vec2(rect.size()))
        } else {
            view::step(v, local(drag.start), !out)
        };
        true
    }

    /// The zoom tool's rectangle while dragging.
    pub(super) fn draw_zoom_rect(&self, painter: &Painter) {
        if let Some(d) = self.zoom_drag.filter(|d| d.is_drag()) {
            let r = egui::Rect::from_two_pos(d.start, d.now);
            painter.rect_stroke(
                r,
                0.0,
                Stroke::new(1.0, theme::YELLOW_SOFT),
                StrokeKind::Middle,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::Modifiers;

    use super::super::harness::{Harness, click, drag};
    use super::*;
    use crate::new_doc::new_document;

    fn session() -> Session {
        let mut s = Session::new(new_document("a", 400.0, 300.0));
        s.editor.tool = ToolId::Zoom;
        s
    }

    #[test]
    fn a_click_zooms_in_one_stop_about_the_click() {
        let mut s = session();
        let mut h = Harness::new();
        h.keep_view = true;
        let at = Pos2::new(200.0, 100.0);
        h.run(&mut s, click(at));
        let v = s.editor.view;
        assert!((v.zoom - 1.5).abs() < 1e-9, "zoom {}", v.zoom);
        let p = view::screen_to_doc(v, to_point(at.to_vec2()));
        assert!((p.x - 200.0).abs() < 1e-9 && (p.y - 100.0).abs() < 1e-9);
        assert_eq!(s.revision(), 0, "the document is untouched");
    }

    #[test]
    fn an_option_click_zooms_out_one_stop() {
        let mut s = session();
        let mut h = Harness::new();
        h.keep_view = true;
        h.modifiers = Modifiers::ALT;
        h.run(&mut s, click(Pos2::new(200.0, 100.0)));
        assert!((s.editor.view.zoom - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn a_drag_zooms_to_fit_the_rectangle() {
        let mut s = session();
        let mut h = Harness::new();
        h.keep_view = true;
        h.run(
            &mut s,
            drag(Pos2::new(100.0, 100.0), Pos2::new(200.0, 150.0)),
        );
        let fit = view::fit(
            vextor_core::Rect::new(100.0, 100.0, 100.0, 50.0),
            vextor_core::Vec2::new(600.0, 400.0),
        );
        assert!((s.editor.view.zoom - fit.zoom).abs() < 1e-9);
        assert!(h.canvas.zoom_drag.is_none());
    }
}
