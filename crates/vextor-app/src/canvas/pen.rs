//! The canvas side of the pen tool (spec 07): pointer and `Enter` to
//! `pen_tool`, and the overlay — the nodes drawn so far, the rubber band
//! to the pointer, the handles being dragged out and what a click would
//! do (close, continue or join, add a node on a segment).

use eframe::egui::{self, Key, Painter, Pos2, Stroke};
use vextor_core::ToolId;

use super::{Canvas, path_marks, to_egui};
use crate::pen_tool::Hover;
use crate::select::Mods;
use crate::session::Session;
use crate::{theme, view};

impl Canvas {
    /// True while the pen is drawing a path.
    #[must_use]
    pub fn pen_drawing(&self) -> bool {
        self.pen.is_drawing()
    }

    /// Ends the path the pen is drawing, left open. Returns true when
    /// that took a step back (a lone first node).
    pub fn finish_pen(&mut self, session: &mut Session) -> bool {
        self.pen.finish(session)
    }

    /// `⌫` while the pen draws: removes the last node.
    pub fn pen_backspace(&mut self, session: &mut Session) -> Option<String> {
        self.pen.backspace(session).err()
    }

    /// A press at `p` (document space) with the pen. Returns true when the
    /// press was used, and a message when drawing is refused.
    pub(super) fn pen_press(
        &mut self,
        session: &mut Session,
        p: kurbo::Point,
        mods: Mods,
    ) -> (bool, Option<String>) {
        if session.editor.tool != ToolId::Pen {
            return (false, None);
        }
        let zoom = session.editor.view.zoom;
        (true, self.pen.press(session, p, mods, zoom).err())
    }

    /// The pointer at `at` with the button `down`, or released.
    pub(super) fn pen_drag(
        &mut self,
        session: &mut Session,
        at: Option<kurbo::Point>,
        down: bool,
        mods: Mods,
    ) -> Option<String> {
        self.pen.sync(session);
        if !self.pen.is_pressed() {
            return None;
        }
        let zoom = session.editor.view.zoom;
        let mut message = None;
        if down && let Some(p) = at {
            message = self.pen.drag_to(session, p, mods, zoom).err();
        }
        if !down {
            message = self.pen.release(session).err().or(message);
        }
        message
    }

    /// `Enter` while the pen draws ends the path, left open.
    pub(super) fn pen_keys(&mut self, ui: &egui::Ui, session: &mut Session) {
        if self.pen.is_drawing()
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter))
        {
            self.pen.finish(session);
        }
    }

    /// The pen's overlay; `hover` is the pointer (document space) when it
    /// is over the canvas.
    pub(super) fn draw_pen(
        &self,
        ui: &egui::Ui,
        painter: &Painter,
        origin: Pos2,
        session: &Session,
        hover: Option<kurbo::Point>,
    ) {
        let v = session.editor.view;
        let screen = |p: kurbo::Point| {
            origin + to_egui(view::doc_to_screen(v, vextor_core::Point::new(p.x, p.y)))
        };
        let shift = ui.input(|i| i.modifiers.shift);
        let what = hover.map(|p| (p, self.pen.hover(session, p, v.zoom)));
        let overlay = self.pen.overlay();
        if let Some(o) = &overlay {
            let last = o.nodes.last().copied();
            // The rubber band: the next segment, from the last node to where
            // a click would put the next one.
            if !self.pen.is_pressed()
                && let (Some(last), Some((p, h))) = (last, what)
            {
                let to = match h {
                    Hover::Close => o.nodes[0],
                    Hover::End(r) => crate::path_ops::anchor_in_doc(session, r).unwrap_or(p),
                    _ => self.pen.next_point(session, p, shift),
                };
                let out = o.last_out.unwrap_or(last);
                let band = [screen(last), screen(out), screen(to), screen(to)];
                path_marks::curve(painter, band, Stroke::new(1.0, theme::YELLOW_SOFT));
            }
            for (i, &n) in o.nodes.iter().enumerate() {
                path_marks::node(
                    painter,
                    screen(n),
                    Some(i + 1) == Some(o.nodes.len()),
                    false,
                );
            }
            if let Some((tip_in, anchor, tip_out)) = o.handles {
                for tip in [tip_in, tip_out].into_iter().flatten() {
                    path_marks::handle(painter, screen(anchor), screen(tip));
                }
            }
        }
        match what {
            Some((_, Hover::Close)) => {
                if let Some(first) = overlay.as_ref().and_then(|o| o.nodes.first()) {
                    path_marks::ring(painter, screen(*first));
                }
            }
            Some((p, Hover::End(r))) => {
                let at = crate::path_ops::anchor_in_doc(session, r).unwrap_or(p);
                path_marks::ring(painter, screen(at));
            }
            Some((_, Hover::Insert(_, hit))) => {
                path_marks::node(painter, screen(hit.point), false, false);
                path_marks::plus(painter, screen(hit.point));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use vextor_core::Point;

    use super::super::harness::{Harness, click, key};
    use super::*;
    use crate::new_doc::new_document;

    #[test]
    fn clicks_on_the_canvas_draw_a_path_and_enter_ends_it() {
        let mut s = Session::new(new_document("a", 400.0, 300.0));
        s.editor.tool = ToolId::Pen;
        let mut h = Harness::new();
        let mut frames = click(Pos2::new(50.0, 60.0));
        frames.extend(click(Pos2::new(150.0, 60.0)));
        frames.extend(click(Pos2::new(150.0, 160.0)));
        h.run(&mut s, frames);
        assert!(h.canvas.pen_drawing());
        h.run(&mut s, vec![vec![key(Key::Enter)], vec![]]);
        assert!(!h.canvas.pen_drawing(), "Enter ended the path");
        let [id] = s.selected()[..] else {
            panic!("the path is selected");
        };
        let anchors: Vec<Point> = s.path_of(id).expect("path").subpaths[0]
            .nodes
            .iter()
            .map(|n| n.anchor)
            .collect();
        assert_eq!(
            anchors,
            vec![
                Point::new(50.0, 60.0),
                Point::new(150.0, 60.0),
                Point::new(150.0, 160.0)
            ]
        );
        assert_eq!(
            s.editor.tool,
            ToolId::Pen,
            "the pen stays for the next path"
        );
    }
}
