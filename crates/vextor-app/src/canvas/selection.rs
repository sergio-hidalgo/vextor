//! The canvas's selection tool input and overlay: hover outline, selection
//! box with handles, marquee, and cursors (including a drawn rotate cursor:
//! the OS offers none).

use std::f32::consts::FRAC_PI_3;

use eframe::egui::{self, Color32, Painter, Pos2, Stroke, StrokeKind, Vec2};
use vextor_core::{NodeId, Point, Rect};
use vextor_geom::Handle;

use super::{Canvas, HANDLE_SIZE, screen_rect, to_egui, to_point};
use crate::select::{Mods, SelectTool, Target};
use crate::session::Session;
use crate::{theme, view};

impl Canvas {
    /// Feeds the pointer to the selection tool (not while panning) and
    /// sets the cursor for what is under it.
    pub(super) fn tool_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        session: &mut Session,
    ) -> Option<String> {
        let view = session.editor.view;
        let to_doc = |pos: Pos2| {
            let p = view::screen_to_doc(view, to_point(pos - rect.min));
            kurbo::Point::new(p.x, p.y)
        };
        let (pressed, down, released, press_at, at, mods, space, free) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.primary_released(),
                i.pointer.press_origin(),
                i.pointer.interact_pos(),
                Mods {
                    shift: i.modifiers.shift,
                    alt: i.modifiers.alt,
                },
                i.key_down(egui::Key::Space),
                // ⌘ suspends snapping (spec 08).
                i.modifiers.command,
            )
        });
        let mut message: Option<String> = None;
        if pressed
            && response.hovered()
            && !space
            && let Some(p) = press_at
        {
            // Clicking the canvas leaves any inspector field, so the
            // keyboard edits the selection again.
            if let Some(id) = ui.ctx().memory(egui::Memory::focused) {
                ui.ctx().memory_mut(|m| m.surrender_focus(id));
            }
            let (used, text_message) = self.text_press(ui.ctx(), session, to_doc(p), mods.shift);
            message = text_message;
            let snapped = self.snap_press(session, to_doc(p), mods, free);
            let used = used || {
                let (used, shape_message) = self.shape_press(session, snapped, mods);
                message = message.or(shape_message);
                used
            };
            let used = used || {
                let (used, pen_message) = self.pen_press(session, snapped, mods);
                message = message.or(pen_message);
                used
            };
            let used = used || self.node_press(session, to_doc(p), mods);
            if !used {
                self.tool.press(session, to_doc(p), mods, view.zoom);
            }
            self.snap_after_press(session);
        }
        let at_doc = at.map(to_doc);
        let snapped_at = match at_doc {
            Some(q) if down && !released => Some(self.snap_drag(session, q, mods, free)),
            other => other,
        };
        if self.is_editing_text() {
            self.text_drag(session, at.filter(|_| down).map(to_doc), down);
        }
        if let Some(e) = self.shape_drag(session, snapped_at, down && !released, mods) {
            message = Some(e);
        }
        if let Some(e) = self.pen_drag(session, snapped_at, down && !released, mods) {
            message = Some(e);
        }
        if let Some(e) = self.node_drag(session, snapped_at, down && !released, mods) {
            message = Some(e);
        }
        if self.tool.is_dragging() {
            if down && let Some(p) = snapped_at {
                message = self.tool.drag_to(session, p, mods, view.zoom).err();
            }
            if released || !down {
                self.tool.release(session);
            }
        }
        if released || !down {
            self.snap.end();
        }
        // After the tools' release, which closes their own gesture.
        if response.double_clicked()
            && !space
            && !self.is_editing_text()
            && let Some(p) = at
        {
            if session.editor.tool == vextor_core::ToolId::Select {
                self.text_double_click(session, to_doc(p));
            }
            if !self.is_editing_text()
                && let Some(e) = super::nodes::node_double_click(session, to_doc(p))
            {
                message = Some(e);
            }
        }
        if response.hovered() && !space {
            self.set_cursor(ui, session, ui.input(|i| i.pointer.hover_pos()).map(to_doc));
        }
        message
    }

    /// Resize arrows over (or dragging) a handle, the move cursor while
    /// moving, and no system cursor where the drawn rotate cursor shows.
    fn set_cursor(&self, ui: &egui::Ui, session: &Session, at: Option<kurbo::Point>) {
        let tool = session.editor.tool;
        let cursor = if self.is_editing_text() || tool == vextor_core::ToolId::Text {
            Some(egui::CursorIcon::Text)
        } else if tool.is_shape() || tool == vextor_core::ToolId::Pen {
            Some(egui::CursorIcon::Crosshair)
        } else if tool == vextor_core::ToolId::Node {
            at.and_then(|p| self.node_cursor(session, p))
        } else if at.is_some_and(|p| self.over_corner_widget(session, p)) {
            Some(egui::CursorIcon::PointingHand)
        } else if self.tool.is_moving() {
            Some(egui::CursorIcon::Move)
        } else if let Some(h) = self.tool.scaling() {
            Some(resize_cursor(h))
        } else if self.tool.rotating() {
            Some(egui::CursorIcon::None)
        } else if let Some(p) = at {
            match SelectTool::target(session, p, session.editor.view.zoom) {
                Target::Handle(h) => Some(resize_cursor(h)),
                Target::Rotate => Some(egui::CursorIcon::None),
                _ => None,
            }
        } else {
            None
        };
        if let Some(c) = cursor {
            ui.ctx().set_cursor_icon(c);
        }
    }

    /// Hover outline, then the selection box (see `draw_box`).
    pub(super) fn draw_selection(
        &self,
        ui: &egui::Ui,
        painter: &Painter,
        response: &egui::Response,
        rect: egui::Rect,
        session: &Session,
    ) {
        let view = session.editor.view;
        let screen = |r: kurbo::Rect| {
            screen_rect(view, rect.min, Rect::new(r.x0, r.y0, r.width(), r.height()))
        };
        let thin = Stroke::new(1.0, theme::YELLOW_SOFT);
        let selected = session.selected();
        self.draw_box(painter, rect.min, session, &selected);
        let Some(p) = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|_| response.hovered())
        else {
            return;
        };
        let at = view::screen_to_doc(view, to_point(p - rect.min));
        let target = (!self.tool.is_dragging())
            .then(|| SelectTool::target(session, kurbo::Point::new(at.x, at.y), view.zoom));
        if let Some(Target::Object(o)) = target
            && !selected.contains(&o)
            && let Some(b) = vextor_geom::bounds(&session.doc, &session.fonts, o)
        {
            painter.rect_stroke(screen(b), 0.0, thin, StrokeKind::Middle);
        }
        if (self.tool.rotating() || target == Some(Target::Rotate))
            && let Some(b) = vextor_geom::union_bounds(&session.doc, &session.fonts, &selected)
        {
            let centre = screen(b).center();
            draw_rotate_cursor(painter, p, p - centre);
        }
    }

    /// Per-object outlines (several selected), the box and its handles, and
    /// the marquee.
    pub(super) fn draw_box(
        &self,
        painter: &Painter,
        origin: Pos2,
        session: &Session,
        selected: &[NodeId],
    ) {
        let view = session.editor.view;
        let screen = |r: kurbo::Rect| {
            screen_rect(view, origin, Rect::new(r.x0, r.y0, r.width(), r.height()))
        };
        let thin = Stroke::new(1.0, theme::YELLOW_SOFT);
        if selected.len() > 1 {
            for b in selected
                .iter()
                .filter_map(|&id| vextor_geom::bounds(&session.doc, &session.fonts, id))
            {
                painter.rect_stroke(screen(b), 0.0, thin, StrokeKind::Middle);
            }
        }
        if let Some(b) = vextor_geom::union_bounds(&session.doc, &session.fonts, selected) {
            let r = screen(b);
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, theme::YELLOW), StrokeKind::Middle);
            let selecting = session.editor.tool == vextor_core::ToolId::Select;
            if selecting && !self.tool.is_moving() {
                super::shapes::draw_corner_widgets(painter, origin, session);
            }
            if !self.tool.is_moving() && selecting {
                for (_, p) in crate::select::handles(b) {
                    let s = view::doc_to_screen(view, Point::new(p.x, p.y));
                    let handle = egui::Rect::from_center_size(
                        origin + to_egui(s),
                        egui::vec2(HANDLE_SIZE, HANDLE_SIZE),
                    );
                    painter.rect_filled(handle, 1.0, theme::WHITE);
                    painter.rect_stroke(
                        handle,
                        1.0,
                        Stroke::new(1.0, theme::LOGO_AMBER),
                        StrokeKind::Inside,
                    );
                }
            }
        }
        if let Some(m) = self.tool.marquee() {
            let r = screen(m);
            painter.rect_filled(r, 0.0, theme::YELLOW.gamma_multiply(0.12));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, theme::YELLOW), StrokeKind::Middle);
        }
    }
}

/// Radius of the drawn rotate cursor's arc, in points.
const ROTATE_ARC: f32 = 9.0;
/// Half the angle the arc spans (60° each side, 120° in all).
const ROTATE_HALF_SPAN: f32 = FRAC_PI_3;

/// The rotate cursor's shape: an arc bulging along `dir` with its middle at
/// `at`, and an arrowhead (tip, two base corners) at each end pointing
/// along the arc. `dir` points away from the selection's centre, so the
/// arrow wraps every corner from outside and follows the pointer.
fn rotate_cursor_shape(at: Pos2, dir: Vec2) -> (Vec<Pos2>, [[Pos2; 3]; 2]) {
    let dir = if dir.length() > f32::EPSILON {
        dir.normalized()
    } else {
        Vec2::new(1.0, -1.0).normalized()
    };
    let centre = at - dir * ROTATE_ARC;
    let mid = dir.y.atan2(dir.x);
    let point = |a: f32| centre + Vec2::new(a.cos(), a.sin()) * ROTATE_ARC;
    let steps = 12;
    #[allow(clippy::cast_precision_loss)] // a dozen steps
    let arc: Vec<Pos2> = (0..=steps)
        .map(|i| mid - ROTATE_HALF_SPAN + 2.0 * ROTATE_HALF_SPAN * i as f32 / steps as f32)
        .map(point)
        .collect();
    let head = |a: f32, sign: f32| {
        let end = point(a);
        let radial = Vec2::new(a.cos(), a.sin());
        let along = Vec2::new(-a.sin(), a.cos()) * sign;
        [end + along * 4.5, end + radial * 3.5, end - radial * 3.5]
    };
    let heads = [
        head(mid + ROTATE_HALF_SPAN, 1.0),
        head(mid - ROTATE_HALF_SPAN, -1.0),
    ];
    (arc, heads)
}

/// Draws the rotate cursor: white with a dark outline, like system cursors.
fn draw_rotate_cursor(painter: &Painter, at: Pos2, dir: Vec2) {
    let (arc, heads) = rotate_cursor_shape(at, dir);
    for (width, color, grow) in [(4.0, Color32::BLACK, 1.6), (1.6, theme::WHITE, 0.0)] {
        painter.add(egui::Shape::line(arc.clone(), Stroke::new(width, color)));
        for [tip, a, b] in heads {
            let c = egui::pos2((tip.x + a.x + b.x) / 3.0, (tip.y + a.y + b.y) / 3.0);
            let out = |p: Pos2| p + (p - c).normalized() * grow;
            painter.add(egui::Shape::convex_polygon(
                vec![out(tip), out(a), out(b)],
                color,
                Stroke::NONE,
            ));
        }
    }
}

/// The resize cursor for dragging `h`.
fn resize_cursor(h: Handle) -> egui::CursorIcon {
    match h {
        Handle::N | Handle::S => egui::CursorIcon::ResizeVertical,
        Handle::E | Handle::W => egui::CursorIcon::ResizeHorizontal,
        Handle::NW | Handle::SE => egui::CursorIcon::ResizeNwSe,
        Handle::NE | Handle::SW => egui::CursorIcon::ResizeNeSw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Pos2, b: Pos2) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn rotate_cursor_is_centred_on_the_pointer_and_symmetric() {
        let at = Pos2::new(100.0, 50.0);
        // Pointing up-right, as at a NE corner.
        let (arc, [end, start]) = rotate_cursor_shape(at, Vec2::new(1.0, -1.0));
        assert!(close(arc[arc.len() / 2], at), "arc middle on the pointer");
        // Both tips lie the same distance from the pointer.
        let d1 = (end[0] - at).length();
        let d2 = (start[0] - at).length();
        assert!((d1 - d2).abs() < 1e-3);
    }

    #[test]
    fn rotate_cursor_turns_with_the_corner() {
        let at = Pos2::new(0.0, 0.0);
        for dir in [
            Vec2::new(1.0, -1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(-1.0, 1.0),
            Vec2::new(-1.0, -1.0),
        ] {
            let (arc, _) = rotate_cursor_shape(at, dir);
            // The arc bulges outward: its ends sit behind the pointer.
            let first = arc[0] - at;
            assert!(first.dot(dir) < 0.0, "{dir:?}");
        }
    }
}
