//! The artboard name label above each artboard, and renaming it in place
//! with a double-click (spec 05e). Dragging the label moves the artboard
//! (spec 08b).

use eframe::egui::{self, Painter, Pos2};
use vextor_core::{Artboard, ArtboardId};

use super::{Canvas, screen_rect, to_point};
use crate::app::keys;
use crate::artboard::{self, ArtboardDrag, MAX_NAME_CHARS};
use crate::session::Session;
use crate::view;

/// Size of the label text, in points.
pub(super) const LABEL_FONT: f32 = 11.0;
/// Gap between the label and the artboard's top edge, in points.
pub(super) const LABEL_GAP: f32 = 4.0;
/// Extra hit area around a label, in points.
const LABEL_SLOP: f32 = 3.0;

/// A rename in progress.
pub(super) struct Rename {
    id: ArtboardId,
    text: String,
    /// The field has not been shown yet: focus it and select its text.
    fresh: bool,
}

/// Where `artboard`'s label sits on screen.
fn label_rect(
    painter: &Painter,
    origin: Pos2,
    session: &Session,
    artboard: &Artboard,
) -> egui::Rect {
    let r = screen_rect(session.editor.view, origin, artboard.rect);
    let galley = painter.layout_no_wrap(
        artboard.name.clone(),
        egui::FontId::proportional(LABEL_FONT),
        egui::Color32::PLACEHOLDER,
    );
    let bottom_left = r.left_top() - egui::vec2(0.0, LABEL_GAP);
    egui::Rect::from_min_size(
        bottom_left - egui::vec2(0.0, galley.size().y),
        galley.size(),
    )
}

/// The document point (as kurbo) under screen position `pos`.
fn to_doc(session: &Session, rect: egui::Rect, pos: Pos2) -> kurbo::Point {
    let d = view::screen_to_doc(session.editor.view, to_point(pos - rect.min));
    kurbo::Point::new(d.x, d.y)
}

/// The artboard whose label is under `at`, topmost first.
fn label_at(painter: &Painter, origin: Pos2, session: &Session, at: Pos2) -> Option<ArtboardId> {
    session
        .doc
        .artboards
        .iter()
        .rev()
        .find(|a| {
            label_rect(painter, origin, session, a)
                .expand(LABEL_SLOP)
                .contains(at)
        })
        .map(|a| a.id)
}

impl Canvas {
    /// Handles the pointer over artboard labels: a press picks the
    /// artboard and a drag moves it (`⌥`: without its objects); a
    /// double-click starts a rename. Presses on a label never reach the
    /// tools. Returns true when the pointer input was used here this
    /// frame, and a message for the user when a move was refused.
    pub(super) fn label_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        session: &mut Session,
    ) -> (bool, Option<String>) {
        if self.is_editing_text() {
            return (false, None);
        }
        if self.artboard_drag.is_some() {
            let message = self.drag_artboard(ui, rect, session);
            // On the release frame, go on: a double-click is seen there.
            if self.artboard_drag.is_some() || message.is_some() {
                return (true, message);
            }
        }
        let painter = ui.painter_at(rect);
        let (hover, pressed, press_at, alt) = ui.input(|i| {
            (
                i.pointer.hover_pos(),
                i.pointer.primary_pressed(),
                i.pointer.press_origin(),
                i.modifiers.alt,
            )
        });
        let hovered = hover
            .filter(|_| response.hovered())
            .and_then(|p| label_at(&painter, rect.min, session, p));
        if hovered.is_some() && self.rename.is_none() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            let alt = keys::modifier_name(ui.ctx(), egui::Modifiers::ALT);
            response.clone().on_hover_text_at_pointer(format!(
                "Drag to move it with its objects ({alt}: alone), \
                 click to show it in the inspector, double-click to rename"
            ));
        }
        if response.double_clicked()
            && let Some(id) = hovered
            && let Some(a) = session.doc.artboards.iter().find(|a| a.id == id)
        {
            self.rename = Some(Rename {
                id,
                text: a.name.clone(),
                fresh: true,
            });
            return (true, None);
        }
        // A press on a label makes its artboard current and shows it in
        // Inspector ▸ Artboard (spec 08).
        let pressed_on = press_at
            .filter(|_| pressed && response.hovered())
            .and_then(|p| label_at(&painter, rect.min, session, p));
        if let Some(id) = pressed_on {
            crate::artboard::pick(session, id);
            session.select(Vec::new());
            if self.rename.is_none()
                && let Some(p) = press_at
            {
                let at = to_doc(session, rect, p);
                self.artboard_drag = ArtboardDrag::press(session, id, at, alt);
                if let Some(d) = &self.artboard_drag {
                    self.snap.begin_artboard(session, d.rect(), d.objects());
                }
            }
        }
        (pressed_on.is_some(), None)
    }

    /// The label drag in progress: moves the artboard with the pointer,
    /// snapped like an object move (`⌘` frees it), until the release.
    /// Returns a message when the move was refused.
    fn drag_artboard(
        &mut self,
        ui: &egui::Ui,
        rect: egui::Rect,
        session: &mut Session,
    ) -> Option<String> {
        let (down, at, free) = ui.input(|i| {
            (
                i.pointer.primary_down(),
                i.pointer.interact_pos(),
                i.modifiers.command,
            )
        });
        let drag = self.artboard_drag.as_mut()?;
        if down && let Some(pos) = at {
            let p = to_doc(session, rect, pos);
            let zoom = session.editor.view.zoom;
            if !drag.started(p, zoom) {
                return None;
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            let snap = self.snap.moving_box(session, drag.moved_rect(p), free);
            return drag.drag_to(session, p, snap, zoom).err();
        }
        if let Some(drag) = self.artboard_drag.take() {
            drag.release(session);
        }
        self.snap.end();
        None
    }

    /// The label being renamed, if any: the canvas draws no label for it.
    pub(super) fn renaming(&self) -> Option<ArtboardId> {
        self.rename.as_ref().map(|r| r.id)
    }

    /// The rename field over the label. Enter or a click elsewhere commits
    /// (one undo step); `Esc` cancels.
    pub(super) fn rename_ui(&mut self, ui: &egui::Ui, rect: egui::Rect, session: &mut Session) {
        let Some(rename) = &mut self.rename else {
            return;
        };
        let Some(a) = session.doc.artboards.iter().find(|a| a.id == rename.id) else {
            self.rename = None;
            return;
        };
        let at = label_rect(&ui.painter_at(rect), rect.min, session, a).left_bottom();
        let output = egui::Area::new(egui::Id::unique("artboard-rename"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .fixed_pos(at + egui::vec2(-4.0, 3.0))
            .show(ui.ctx(), |ui| {
                egui::TextEdit::singleline(&mut rename.text)
                    .font(egui::FontId::proportional(LABEL_FONT + 1.0))
                    .char_limit(MAX_NAME_CHARS)
                    .desired_width(160.0)
                    .show(ui)
            })
            .inner;
        if rename.fresh {
            rename.fresh = false;
            let mut state = output.state;
            let all = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(rename.text.chars().count()),
            );
            state.cursor.set_char_range(Some(all));
            state.store(ui.ctx(), output.response.id);
            output.response.request_focus();
        } else if !output.response.has_focus() {
            if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                artboard::rename(session, rename.id, &rename.text);
            }
            self.rename = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::Modifiers;
    use vextor_core::{
        Command, CornerRadii, Geometry, Node, NodeId, NodeKind, Shape, Style, Vec2, View,
    };

    use super::super::harness::{Harness, click, drag};
    use super::*;
    use crate::new_doc::new_document;

    /// A 200×100 artboard at the origin with a rect on it, shown at 100 %
    /// panned by (50, 50): the label sits just above (50, 46).
    fn session() -> (Session, NodeId) {
        let mut doc = new_document("a", 200.0, 100.0);
        let layer = doc.layers[0];
        let id = doc.alloc_node_id();
        let rect = Node::new(
            id,
            NodeKind::Shape(Shape {
                geometry: Geometry::Rect {
                    rect: vextor_core::Rect::new(20.0, 20.0, 30.0, 30.0),
                    radius: CornerRadii::default(),
                },
                style: Style::default(),
            }),
        );
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![rect],
        })
        .expect("inserts");
        let mut s = Session::new(doc);
        s.editor.view = View {
            zoom: 1.0,
            pan: Vec2::new(50.0, 50.0),
        };
        (s, id)
    }

    fn harness() -> Harness {
        let mut h = Harness::new();
        h.keep_view = true;
        h
    }

    const LABEL: Pos2 = Pos2::new(55.0, 40.0);

    fn rect_x(s: &Session, id: NodeId) -> f64 {
        vextor_geom::bounds(&s.doc, &s.fonts, id)
            .expect("has bounds")
            .x0
    }

    #[test]
    fn dragging_the_label_moves_the_artboard_and_its_objects() {
        let (mut s, id) = session();
        let mut h = harness();
        // ⌘: no snapping, so the move is exactly the drag.
        h.modifiers = Modifiers::COMMAND;
        h.run(&mut s, drag(LABEL, LABEL + egui::vec2(77.0, 33.0)));
        let r = s.doc.artboards[0].rect;
        assert_eq!((r.x, r.y), (77.0, 33.0));
        assert!((rect_x(&s, id) - 97.0).abs() < 1e-9);
        assert!(h.canvas.artboard_drag.is_none(), "the drag ended");
        assert_eq!(s.edit_menu().undo, Some("Move artboard"));
        assert!(s.undo());
        assert_eq!(s.doc.artboards[0].rect.x, 0.0);
        assert!((rect_x(&s, id) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn option_dragging_the_label_leaves_the_objects() {
        let (mut s, id) = session();
        let mut h = harness();
        h.modifiers = Modifiers::COMMAND.plus(Modifiers::ALT);
        h.run(&mut s, drag(LABEL, LABEL + egui::vec2(77.0, 33.0)));
        assert_eq!(s.doc.artboards[0].rect.x, 77.0);
        assert!((rect_x(&s, id) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn a_label_drag_snaps_to_the_grid() {
        let (mut s, _) = session();
        s.doc.grid.snap.to_grid = true;
        let mut h = harness();
        h.run(&mut s, drag(LABEL, LABEL + egui::vec2(41.0, 0.0)));
        let x = s.doc.artboards[0].rect.x;
        let spacing = vextor_geom::minor_spacing(&s.doc.grid).expect("a grid");
        assert!(
            (x / spacing - (x / spacing).round()).abs() < 1e-9,
            "x {x} on the grid"
        );
    }

    #[test]
    fn a_double_click_on_the_label_still_renames() {
        let (mut s, _) = session();
        let mut h = harness();
        let mut frames = click(LABEL);
        frames.extend(click(LABEL));
        h.run(&mut s, frames);
        assert!(h.canvas.renaming().is_some(), "the rename field shows");
        assert!(h.canvas.artboard_drag.is_none());
    }

    #[test]
    fn a_click_on_the_label_only_picks_the_artboard() {
        let (mut s, id) = session();
        s.select(vec![id]);
        let before = s.doc.clone();
        let mut h = harness();
        h.run(&mut s, click(LABEL));
        assert_eq!(s.doc, before);
        assert!(s.selected().is_empty(), "the selection clears");
        assert_eq!(s.artboard, Some(s.doc.artboards[0].id));
    }
}
