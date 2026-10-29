//! The canvas context menu: the path commands (spec 07) while a path is
//! selected, and the arrange commands (spec 08c) after a right-click on an
//! object, which selects it first when it was not selected. With several
//! objects selected, or a group, Group / Ungroup come first; with several,
//! the path operations that combine them (spec 11) follow; any selection
//! can be exported (spec 12).

use eframe::egui;
use vextor_core::ToolId;

use super::{Canvas, nodes, to_point};
use crate::app::keys::{self, GROUP, UNGROUP};
use crate::edit_ops;
use crate::select::PICK_TOLERANCE;
use crate::session::Session;
use crate::shape_ops::ShapeOp;
use crate::view;

/// What the menu asks the app to do; the app runs it like the same menu
/// item elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuRequest {
    /// A path operation (spec 11), as from Object ▸ Path Operations.
    Shape(ShapeOp),
    /// File ▸ Export… set to the selection as SVG (spec 12).
    ExportSelection,
    /// Object ▸ Image to Vector… (spec 13b), on an image.
    ImageToVector,
}

impl Canvas {
    /// Handles a right-click and shows the menu while it is open. Returns
    /// a message when a command was refused.
    pub(super) fn context_menu(
        &mut self,
        response: &egui::Response,
        rect: egui::Rect,
        session: &mut Session,
    ) -> Option<String> {
        if response.secondary_clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let d = view::screen_to_doc(session.editor.view, to_point(p - rect.min));
            self.menu_on_object = right_click(session, kurbo::Point::new(d.x, d.y));
        }
        let paths = nodes::has_path_menu(session);
        if !paths && !self.menu_on_object {
            return None;
        }
        let (grouping, several) = menu_sections(session, self.menu_on_object);
        let mut message = None;
        let mut request = None;
        response.context_menu(|ui| {
            if grouping {
                message = group_items(ui, session);
                if several {
                    request = combine_items(ui, session).map(MenuRequest::Shape);
                }
                ui.separator();
            }
            if paths {
                message = nodes::path_menu_items(ui, session).or(message.take());
            }
            message = arrange_items(ui, session).or(message.take());
            if self.menu_on_object {
                ui.separator();
                message = clip_items(ui, session).or(message.take());
                ui.separator();
                if crate::image_trace::traceable(session).is_ok()
                    && ui.button("Image to Vector…").clicked()
                {
                    request = Some(MenuRequest::ImageToVector);
                    ui.close();
                }
                if ui.button("Export Selection…").clicked() {
                    request = Some(MenuRequest::ExportSelection);
                    ui.close();
                }
            }
        });
        if request.is_some() {
            self.request = request;
        }
        message
    }

    /// What the context menu asked for since the last call.
    pub fn take_request(&mut self) -> Option<MenuRequest> {
        self.request.take()
    }
}

/// An edit run from a menu item.
type EditRun = fn(&mut Session) -> Result<(), String>;

/// Group and Ungroup, disabled with the reason.
fn group_items(ui: &mut egui::Ui, session: &mut Session) -> Option<String> {
    let can = edit_ops::can(session);
    let items: [(&str, _, bool, &str, EditRun); 2] = [
        (
            "Group",
            GROUP,
            can.group,
            "Select objects in the same group or layer",
            edit_ops::group,
        ),
        (
            "Ungroup",
            UNGROUP,
            can.ungroup,
            "Select a group",
            edit_ops::ungroup,
        ),
    ];
    let mut message = None;
    for (text, keys, enabled, why, run) in items {
        let button = egui::Button::new(text).shortcut_text(keys::label(ui.ctx(), keys));
        if ui
            .add_enabled(enabled, button)
            .on_disabled_hover_text(why)
            .clicked()
        {
            message = run(session).err();
            ui.close();
        }
    }
    message
}

/// Union, Subtract, Intersect, Exclude and Divide, disabled with the
/// reason. Returns the one clicked: the app runs it, as from the Object
/// menu.
fn combine_items(ui: &mut egui::Ui, session: &Session) -> Option<ShapeOp> {
    ui.separator();
    let mut picked = None;
    for op in ShapeOp::COMBINE {
        let keys = keys::shape_op(ui.ctx().os(), op).map(|k| keys::label(ui.ctx(), k));
        let button = egui::Button::new(op.title()).shortcut_text(keys.unwrap_or_default());
        if ui
            .add_enabled(op.can(session), button)
            .on_disabled_hover_text(op.why())
            .clicked()
        {
            picked = Some(op);
            ui.close();
        }
    }
    picked
}

/// Which object sections the menu shows: Group / Ungroup (several
/// objects, or a group), and the combining path operations (several).
fn menu_sections(session: &Session, on_object: bool) -> (bool, bool) {
    let several = on_object && session.selected().len() >= 2;
    let grouping = several || (on_object && edit_ops::can(session).ungroup);
    (grouping, several)
}

/// A right-click at `p` (document space): with the Select tool, an object
/// under it that is not selected becomes the selection. Returns whether
/// it landed on a selected object, so the arrange items act on it.
fn right_click(session: &mut Session, p: kurbo::Point) -> bool {
    let tolerance = PICK_TOLERANCE / session.editor.view.zoom;
    let Some(id) = session.hit_index().pick(p, tolerance) else {
        return false;
    };
    if session.editor.tool == ToolId::Select && !session.selected().contains(&id) {
        session.select(vec![id]);
    }
    session.selected().contains(&id)
}

/// Bring to Front … Send to Back with their keys, disabled with the
/// reason. Returns a message when the move was refused.
fn arrange_items(ui: &mut egui::Ui, session: &mut Session) -> Option<String> {
    let can = edit_ops::can(session);
    let mut message = None;
    for (how, text) in edit_ops::ARRANGE {
        let keys = keys::label(ui.ctx(), keys::arrange(how));
        let button = egui::Button::new(text).shortcut_text(keys);
        if ui
            .add_enabled(can.arrange(how), button)
            .on_disabled_hover_text(can.arrange_why(how))
            .clicked()
        {
            message = edit_ops::arrange(session, how).err();
            ui.close();
        }
    }
    message
}

/// Make Clip and Release Clip (spec 10), disabled with the reason.
fn clip_items(ui: &mut egui::Ui, session: &mut Session) -> Option<String> {
    let can = edit_ops::can(session);
    let mut message = None;
    if ui
        .add_enabled(can.make_clip, egui::Button::new("Make Clip"))
        .on_disabled_hover_text(edit_ops::MAKE_CLIP_HINT)
        .clicked()
    {
        message = edit_ops::make_clip(session).err();
        ui.close();
    }
    if ui
        .add_enabled(can.release_clip, egui::Button::new("Release Clip"))
        .on_disabled_hover_text("Select a clip group")
        .clicked()
    {
        message = edit_ops::release_clip(session).err();
        ui.close();
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::harness::Harness;
    use eframe::egui::{Event, Modifiers, PointerButton, Pos2};
    use vextor_core::{Command, CornerRadii, Geometry, Node, NodeId, NodeKind, Rect, Shape, Style};

    /// A session with two 10 × 10 rects at x 0 and x 20.
    fn two_rects() -> (Session, [NodeId; 2]) {
        let mut doc = crate::new_doc::new_document("x", 100.0, 100.0);
        let layer = doc.layers[0];
        let ids = [doc.alloc_node_id(), doc.alloc_node_id()];
        let nodes = ids
            .iter()
            .zip([0.0, 20.0])
            .map(|(&id, x)| {
                Node::new(
                    id,
                    NodeKind::Shape(Shape {
                        geometry: Geometry::Rect {
                            rect: Rect::new(x, 0.0, 10.0, 10.0),
                            radius: CornerRadii::default(),
                        },
                        style: Style::default(),
                    }),
                )
            })
            .collect();
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes,
        })
        .expect("inserts");
        (Session::new(doc), ids)
    }

    fn right_click_at(pos: Pos2) -> Vec<Vec<Event>> {
        let press = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        vec![
            vec![Event::PointerMoved(pos)],
            vec![press(true)],
            vec![press(false)],
            vec![],
        ]
    }

    #[test]
    fn right_click_selects_the_object_under_it() {
        let (mut s, [a, b]) = two_rects();
        s.select(vec![a]);
        let mut h = Harness::new();
        h.run(&mut s, right_click_at(Pos2::new(25.0, 5.0)));
        assert_eq!(s.selected(), vec![b]);
        assert!(h.canvas.menu_on_object);
    }

    #[test]
    fn right_click_keeps_a_selection_it_lands_in() {
        let (mut s, [a, b]) = two_rects();
        s.select(vec![a, b]);
        let mut h = Harness::new();
        h.run(&mut s, right_click_at(Pos2::new(5.0, 5.0)));
        assert_eq!(s.selected(), vec![a, b]);
    }

    #[test]
    fn right_click_with_the_node_tool_leaves_another_object_alone() {
        let (mut s, [a, _]) = two_rects();
        s.select(vec![a]);
        s.editor.tool = ToolId::Node;
        let mut h = Harness::new();
        h.run(&mut s, right_click_at(Pos2::new(25.0, 5.0)));
        assert_eq!(s.selected(), vec![a]);
        assert!(!h.canvas.menu_on_object);
    }

    #[test]
    fn two_selected_shapes_offer_group_and_every_combining_operation() {
        let (mut s, [a, b]) = two_rects();
        s.select(vec![a, b]);
        let mut h = Harness::new();
        h.run(&mut s, right_click_at(Pos2::new(5.0, 5.0)));
        assert!(h.canvas.menu_on_object);
        assert_eq!(menu_sections(&s, true), (true, true));
        assert!(edit_ops::can(&s).group, "Group is enabled");
        for op in ShapeOp::COMBINE {
            assert!(op.can(&s), "{} is enabled", op.title());
        }
        // A picked operation is handed to the app once.
        h.canvas.request = Some(MenuRequest::Shape(ShapeOp::Union));
        assert_eq!(
            h.canvas.take_request(),
            Some(MenuRequest::Shape(ShapeOp::Union))
        );
        assert_eq!(h.canvas.take_request(), None);
    }

    #[test]
    fn a_right_clicked_group_offers_ungroup() {
        let (mut s, [a, b]) = two_rects();
        s.select(vec![a, b]);
        edit_ops::group(&mut s).expect("groups");
        let group = s.selected()[0];
        s.select(Vec::new());
        let mut h = Harness::new();
        h.run(&mut s, right_click_at(Pos2::new(5.0, 5.0)));
        assert_eq!(s.selected(), vec![group], "the group is picked whole");
        assert!(h.canvas.menu_on_object);
        assert_eq!(
            menu_sections(&s, true),
            (true, false),
            "Group / Ungroup show, the path operations need several objects"
        );
        assert!(edit_ops::can(&s).ungroup, "Ungroup is enabled");
        edit_ops::ungroup(&mut s).expect("ungroups");
        assert!(!s.doc.nodes.contains_key(&group));
    }

    #[test]
    fn right_click_on_empty_canvas_opens_no_menu() {
        let (mut s, [a, _]) = two_rects();
        s.select(vec![a]);
        let mut h = Harness::new();
        h.run(&mut s, right_click_at(Pos2::new(60.0, 60.0)));
        assert_eq!(s.selected(), vec![a]);
        assert!(!h.canvas.menu_on_object);
    }
}
