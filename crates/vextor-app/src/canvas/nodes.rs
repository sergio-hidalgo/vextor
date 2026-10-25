//! The canvas side of node editing (spec 07): pointer to `node_tool`, a
//! double-click on a path entering node editing, the path commands'
//! context menu, and the overlay — the edited path's outline, its nodes
//! (squares for corners, circles for smooth nodes), the shown handles,
//! the selected or hovered segment, corner round widgets and the marquee.

use eframe::egui::{self, Painter, Pos2, Stroke, StrokeKind};
use kurbo::PathEl;
use vextor_core::{NodeKind, NodeType, ToolId};
use vextor_geom::{path_to_bezpath, segment};

use super::{Canvas, path_marks, screen_rect, to_egui};
use crate::node_tool::{self, NodeTarget, NodeTool};
use crate::path_ops::{self, PathAction};
use crate::select::{Mods, PICK_TOLERANCE};
use crate::session::Session;
use crate::{theme, view};

/// Radius of a drawn corner round widget, in points.
const WIDGET_RADIUS: f32 = 4.0;
/// How far a flattened outline may stray from the curve, in screen points.
const FLATTEN: f64 = 0.25;

impl Canvas {
    /// A press at `p` (document space) with the node tool. Returns true
    /// when the press was used.
    pub(super) fn node_press(
        &mut self,
        session: &mut Session,
        p: kurbo::Point,
        mods: Mods,
    ) -> bool {
        if session.editor.tool != ToolId::Node {
            return false;
        }
        let zoom = session.editor.view.zoom;
        self.nodes.press(session, p, mods, zoom);
        true
    }

    /// The pointer at `at` with the button `down`, or released.
    pub(super) fn node_drag(
        &mut self,
        session: &mut Session,
        at: Option<kurbo::Point>,
        down: bool,
        mods: Mods,
    ) -> Option<String> {
        if !self.nodes.is_dragging() {
            return None;
        }
        let zoom = session.editor.view.zoom;
        let mut message = None;
        if down && let Some(p) = at {
            message = self.nodes.drag_to(session, p, mods, zoom).err();
        }
        if !down {
            self.nodes.release(session);
        }
        message
    }

    /// The node tool's overlay; `hover` is the pointer (document space)
    /// when it is over the canvas.
    pub(super) fn draw_nodes(
        &self,
        painter: &Painter,
        origin: Pos2,
        session: &Session,
        hover: Option<kurbo::Point>,
    ) {
        let v = session.editor.view;
        let screen = |p: kurbo::Point| {
            origin + to_egui(view::doc_to_screen(v, vextor_core::Point::new(p.x, p.y)))
        };
        let target = hover
            .filter(|_| !self.nodes.is_dragging())
            .map(|p| NodeTool::target(session, p, v.zoom));
        if let Some(NodeTarget::Object(id)) = target
            && session.path_of(id).is_some()
        {
            draw_outline(painter, session, id, &screen, theme::YELLOW_SOFT);
        }
        if let Some(shape) = path_ops::edited(session) {
            draw_edited(painter, session, shape, target, &screen);
        }
        if let Some(m) = self.nodes.marquee() {
            let r = screen_rect(
                v,
                origin,
                vextor_core::Rect::new(m.x0, m.y0, m.width(), m.height()),
            );
            painter.rect_filled(r, 0.0, theme::YELLOW.gamma_multiply(0.12));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, theme::YELLOW), StrokeKind::Middle);
        }
    }

    /// The node tool's cursor at `p`: a hand over what can be dragged.
    pub(super) fn node_cursor(
        &self,
        session: &Session,
        p: kurbo::Point,
    ) -> Option<egui::CursorIcon> {
        if self.nodes.is_dragging() {
            return None;
        }
        match NodeTool::target(session, p, session.editor.view.zoom) {
            NodeTarget::Handle(..) | NodeTarget::Radius(_) | NodeTarget::Node(_) => {
                Some(egui::CursorIcon::PointingHand)
            }
            _ => None,
        }
    }
}

/// Whether the canvas context menu has path commands: a path is
/// selected, or two open paths to join.
pub(super) fn has_path_menu(session: &Session) -> bool {
    path_ops::edited(session).is_some() || path_ops::join_target(session).is_some()
}

/// The path commands of the canvas context menu, each group followed by
/// a separator. Returns a message when a command was refused.
pub(super) fn path_menu_items(ui: &mut egui::Ui, session: &mut Session) -> Option<String> {
    let mut message = None;
    for group in [&PathAction::STRUCTURE[..], &PathAction::EDIT[..]] {
        for &a in group {
            let button = ui
                .add_enabled(a.can(session), egui::Button::new(a.title()))
                .on_disabled_hover_text(a.why());
            if button.clicked() {
                message = a.run(session).err();
                ui.close();
            }
        }
        ui.separator();
    }
    message
}

/// A double-click at `p`: with the selection tool on a path, edits its
/// nodes (another shape must be converted first); with the node tool,
/// cycles a node's type or leaves on empty canvas.
pub(super) fn node_double_click(session: &mut Session, p: kurbo::Point) -> Option<String> {
    let zoom = session.editor.view.zoom;
    match session.editor.tool {
        ToolId::Node => NodeTool::double_click(session, p, zoom).err(),
        ToolId::Select => {
            let id = session.hit_index().pick(p, PICK_TOLERANCE / zoom)?;
            if session.path_of(id).is_some() {
                session.select(vec![id]);
                session.editor.tool = ToolId::Node;
                return None;
            }
            let shape = matches!(
                session.doc.nodes.get(&id).map(|n| &n.kind),
                Some(NodeKind::Shape(_))
            );
            shape.then(|| {
                "To edit its nodes, convert it to a path first: \
                 Object ▸ Convert to Path (⇧⌘P)"
                    .to_owned()
            })
        }
        _ => None,
    }
}

/// The edited path `shape`: outline, hovered and selected segment,
/// handles, nodes (a ring on the hovered one) and corner round widgets.
fn draw_edited(
    painter: &Painter,
    session: &Session,
    shape: vextor_core::NodeId,
    target: Option<NodeTarget>,
    screen: &impl Fn(kurbo::Point) -> Pos2,
) {
    let Some(path) = session.path_of(shape) else {
        return;
    };
    draw_outline(painter, session, shape, &screen, theme::YELLOW_SOFT);
    let w = path_ops::world(session, shape);
    let seg = |subpath: usize, index: usize, color, width| {
        if let Some(c) = path.subpaths.get(subpath).and_then(|sp| segment(sp, index)) {
            let c = w * c;
            let pts = [screen(c.p0), screen(c.p1), screen(c.p2), screen(c.p3)];
            path_marks::curve(painter, pts, Stroke::new(width, color));
        }
    };
    if let Some(NodeTarget::Segment(_, hit)) = target {
        seg(hit.subpath, hit.index, theme::YELLOW_SOFT, 2.0);
    }
    if let Some((_, subpath, index)) = path_ops::segment_target(session) {
        seg(subpath, index, theme::YELLOW, 2.0);
    }
    // Neighbours' handles first (grey), the selected node's on top.
    for h in node_tool::handles(session) {
        if h.own {
            path_marks::handle(painter, screen(h.anchor), screen(h.tip));
        } else {
            path_marks::neighbour_handle(painter, screen(h.anchor), screen(h.tip));
        }
    }
    let selected = path_ops::selected_nodes(session);
    for (subpath, sp) in path.subpaths.iter().enumerate() {
        for (index, n) in sp.nodes.iter().enumerate() {
            let r = vextor_core::PathNodeRef {
                shape,
                subpath,
                index,
            };
            let at = screen(w * vextor_geom::to_kurbo(n.anchor));
            path_marks::node(
                painter,
                at,
                selected.contains(&r),
                n.kind != NodeType::Corner,
            );
            if target == Some(NodeTarget::Node(r)) {
                path_marks::ring(painter, at);
            }
        }
    }
    for (_, at) in node_tool::radius_widgets(session, session.editor.view.zoom) {
        painter.circle_filled(screen(at), WIDGET_RADIUS, theme::WHITE);
        painter.circle_stroke(
            screen(at),
            WIDGET_RADIUS,
            Stroke::new(1.0, theme::LOGO_AMBER),
        );
    }
}

/// Path `id`'s outline (corner rounding included), thin, in `color`.
fn draw_outline(
    painter: &Painter,
    session: &Session,
    id: vextor_core::NodeId,
    screen: &impl Fn(kurbo::Point) -> Pos2,
    color: egui::Color32,
) {
    let Some(path) = session.path_of(id) else {
        return;
    };
    let bez = path_ops::world(session, id) * path_to_bezpath(path);
    let tolerance = FLATTEN / session.editor.view.zoom;
    let stroke = Stroke::new(1.0, color);
    let mut line: Vec<Pos2> = Vec::new();
    let mut start: Option<Pos2> = None;
    let flush = |line: &mut Vec<Pos2>| {
        if line.len() > 1 {
            painter.add(egui::Shape::line(std::mem::take(line), stroke));
        }
        line.clear();
    };
    kurbo::flatten(bez.elements().iter().copied(), tolerance, |el| match el {
        PathEl::MoveTo(p) => {
            flush(&mut line);
            let p = screen(p);
            start = Some(p);
            line.push(p);
        }
        PathEl::LineTo(p) => line.push(screen(p)),
        PathEl::ClosePath => {
            if let Some(s) = start {
                line.push(s);
            }
            flush(&mut line);
        }
        _ => {}
    });
    flush(&mut line);
}

#[cfg(test)]
mod tests {
    use vextor_core::{Command, Geometry, Node, Path, PathNode, Shape, Style, SubPath};

    use super::super::harness::{Harness, click, drag};
    use super::*;
    use crate::new_doc::new_document;

    /// A session with one open path (100,100) → (200,100) → (200,200).
    fn session() -> (Session, vextor_core::NodeId) {
        let mut doc = new_document("a", 400.0, 300.0);
        let layer = doc.layers[0];
        let id = doc.alloc_node_id();
        let nodes = [(100.0, 100.0), (200.0, 100.0), (200.0, 200.0)]
            .map(|(x, y)| PathNode::corner(x, y))
            .to_vec();
        let shape = Shape {
            geometry: Geometry::Path(Path {
                subpaths: vec![SubPath {
                    nodes,
                    closed: false,
                }],
                fill_rule: vextor_core::FillRule::NonZero,
            }),
            style: Style::default(),
        };
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![Node::new(id, NodeKind::Shape(shape))],
        })
        .expect("inserts");
        (Session::new(doc), id)
    }

    #[test]
    fn double_clicking_a_path_edits_its_nodes_and_a_node_drag_is_one_step() {
        let (mut s, id) = session();
        let mut h = Harness::new();
        let on_path = Pos2::new(150.0, 100.0);
        let mut frames = click(on_path);
        frames.extend(click(on_path));
        h.run(&mut s, frames);
        assert_eq!(
            s.editor.tool,
            ToolId::Node,
            "double-click entered node editing"
        );
        assert_eq!(path_ops::edited(&s), Some(id));

        let shown = |c: &Canvas| c.shown.map(|sh| sh.revision);
        let (from, to) = (Pos2::new(200.0, 100.0), Pos2::new(230.0, 80.0));
        let mut frames = drag(from, to);
        let release = frames.split_off(4);
        h.run(&mut s, frames);
        assert!(h.canvas.drag_layers.is_some(), "drawn from the layers");
        let mid_drag = shown(&h.canvas);
        h.run(&mut s, release);
        assert_ne!(shown(&h.canvas), mid_drag, "one sharp render on release");
        let r = vextor_core::PathNodeRef {
            shape: id,
            subpath: 0,
            index: 1,
        };
        assert_eq!(
            s.path_node(r).expect("node").anchor,
            vextor_core::Point::new(230.0, 80.0)
        );
        assert_eq!(s.edit_menu().undo, Some("Move nodes"));
        s.undo();
        assert_eq!(s.edit_menu().undo, None, "the drag was one step");
    }

    #[test]
    fn double_clicking_a_rect_says_to_convert_it_first() {
        let mut s = Session::new(new_document("a", 400.0, 300.0));
        let layer = s.doc.layers[0];
        let id = s.doc.alloc_node_id();
        let rect = Shape {
            geometry: Geometry::Rect {
                rect: vextor_core::Rect::new(50.0, 50.0, 100.0, 100.0),
                radius: vextor_core::CornerRadii::default(),
            },
            style: Style::default(),
        };
        s.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![Node::new(id, NodeKind::Shape(rect))],
        })
        .expect("inserts");
        let message = node_double_click(&mut s, kurbo::Point::new(100.0, 100.0));
        assert!(message.is_some_and(|m| m.contains("Convert to Path")));
        assert_eq!(s.editor.tool, ToolId::Select, "never converted silently");
    }
}
