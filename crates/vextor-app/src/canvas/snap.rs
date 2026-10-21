//! The canvas's side of snapping (spec 08): which point each tool's drag
//! snaps, and the hint that says what it snapped to (a mark, a label and
//! smart guides).

use eframe::egui::{self, Painter, Pos2, Stroke};
use vextor_core::ToolId;

use super::{Canvas, to_egui};
use crate::path_ops::selected_nodes;
use crate::select::Mods;
use crate::session::Session;
use crate::snapping::other_nodes;
use crate::{theme, view};

/// Movement before a press becomes a drag, in screen points (matches the
/// tools): a click never snaps into a move.
const DRAG_THRESHOLD: f64 = 3.0;
/// Half the side of the snap mark, in points.
const MARK: f32 = 4.0;

/// Whether `tool` draws shapes that pixel snap puts on whole px.
fn pixel_tool(tool: ToolId) -> bool {
    matches!(tool, ToolId::Rect | ToolId::Line)
}

impl Canvas {
    /// A press at `p`: shape tools freeze their targets and snap the first
    /// corner; the pen snaps its click (not while `⇧` constrains it).
    pub(super) fn snap_press(
        &mut self,
        session: &Session,
        p: kurbo::Point,
        mods: Mods,
        free: bool,
    ) -> kurbo::Point {
        let tool = session.editor.tool;
        if tool.is_shape() {
            self.snap.begin(session, &[], Vec::new());
            return self.snap.point(session, p, free, pixel_tool(tool));
        }
        if tool == ToolId::Pen && !mods.shift {
            return self.snap.point(session, p, free, false);
        }
        p
    }

    /// After the tools took the press: a move, scale, node or handle drag
    /// freezes its targets, leaving out what it drags.
    pub(super) fn snap_after_press(&mut self, session: &Session) {
        if let Some(d) = self.tool.snap_drag() {
            let ids = d.ids.to_vec();
            self.snap.begin(session, &ids, Vec::new());
        } else if let Some((_, _, shape)) = self.nodes.snap_drag(session) {
            let nodes = other_nodes(session, shape, &selected_nodes(session));
            self.snap.begin(session, &[shape], nodes);
        }
    }

    /// The pointer `p` of the drag in progress, moved so that what the drag
    /// holds snaps: the shape's corner, the grabbed node, handle or box
    /// handle, or the moved selection's box.
    pub(super) fn snap_drag(
        &mut self,
        session: &Session,
        p: kurbo::Point,
        mods: Mods,
        free: bool,
    ) -> kurbo::Point {
        if !self.snap.is_dragging() {
            return p;
        }
        let zoom = session.editor.view.zoom;
        let started = |start: kurbo::Point| (p - start).hypot() * zoom >= DRAG_THRESHOLD;
        if self.shapes.is_drawing() {
            let pixel = pixel_tool(session.editor.tool);
            return self.snap.point(session, p, free, pixel);
        }
        if let Some((start, anchor, _)) = self.nodes.snap_drag(session) {
            if mods.shift || !started(start) {
                self.snap.hint = None;
                return p;
            }
            return self.snap.grabbed(session, start, anchor, p, free);
        }
        if let Some(d) = self.tool.snap_drag() {
            let (start, bounds, handle) = (d.start, d.bounds, d.handle);
            if !started(start) {
                return p;
            }
            return match handle {
                None => p + self.snap.moving_box(session, bounds + (p - start), free),
                Some(h) => self
                    .snap
                    .grabbed(session, start, h.position(bounds), p, free),
            };
        }
        p
    }

    /// The pen's hover point, snapped like its click will be.
    pub(super) fn snap_hover(
        &mut self,
        ui: &egui::Ui,
        session: &Session,
        hover: Option<kurbo::Point>,
    ) -> Option<kurbo::Point> {
        if session.editor.tool != ToolId::Pen || self.pen.is_pressed() {
            return hover;
        }
        let (shift, free) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
        if shift {
            self.snap.hint = None;
            return hover;
        }
        let snapped = hover.map(|p| self.snap.point(session, p, free, false));
        if snapped.is_none() {
            self.snap.hint = None;
        }
        snapped
    }

    /// The snap hint: smart guides, a mark on the snapped point and a
    /// label (on `surface`) naming what it snapped to.
    pub(super) fn draw_snap_hint(
        &self,
        painter: &Painter,
        origin: Pos2,
        session: &Session,
        surface: egui::Color32,
    ) {
        let Some(hint) = &self.snap.hint else {
            return;
        };
        let v = session.editor.view;
        let screen = |p: kurbo::Point| {
            origin + to_egui(view::doc_to_screen(v, vextor_core::Point::new(p.x, p.y)))
        };
        if !self.hide_smart_guides {
            let guide = Stroke::new(1.0, theme::LOGO_AMBER);
            for g in &hint.guides {
                painter.line_segment([screen(g.from), screen(g.to)], guide);
            }
        }
        let at = screen(hint.at);
        let mark = Stroke::new(1.5, theme::LOGO_YELLOW);
        let d = egui::vec2(MARK, MARK);
        painter.line_segment([at - d, at + d], mark);
        painter.line_segment(
            [at + egui::vec2(-MARK, MARK), at + egui::vec2(MARK, -MARK)],
            mark,
        );
        let font = egui::FontId::proportional(11.0);
        let galley = painter.layout_no_wrap(hint.kind.label().to_owned(), font, theme::GREY_100);
        let pos = at + egui::vec2(MARK + 4.0, -MARK - 4.0 - galley.size().y);
        let bg = egui::Rect::from_min_size(pos, galley.size()).expand2(egui::vec2(4.0, 1.0));
        painter.rect_filled(bg, 3.0, surface);
        painter.galley(pos, galley, theme::GREY_100);
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::Modifiers;
    use vextor_core::{
        Command, CornerRadii, Geometry, Node, NodeId, NodeKind, Path, PathNode, Shape, Style,
        SubPath,
    };

    use super::super::harness::{Harness, click, drag};
    use super::*;
    use crate::new_doc::new_document;

    /// Rects A (100, 100, 50×50) and B (200, 100, 50×50) on a 400×300
    /// artboard, B selected.
    fn session() -> (Session, NodeId) {
        let mut doc = new_document("a", 400.0, 300.0);
        let layer = doc.layers[0];
        let rect = |doc: &mut vextor_core::Document, x: f64| {
            Node::new(
                doc.alloc_node_id(),
                NodeKind::Shape(Shape {
                    geometry: Geometry::Rect {
                        rect: vextor_core::Rect::new(x, 100.0, 50.0, 50.0),
                        radius: CornerRadii::default(),
                    },
                    style: Style::default(),
                }),
            )
        };
        let (a, b) = (rect(&mut doc, 100.0), rect(&mut doc, 200.0));
        let id = b.id;
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![a, b],
        })
        .expect("inserts");
        let mut s = Session::new(doc);
        s.select(vec![id]);
        (s, id)
    }

    fn left(s: &Session, id: NodeId) -> f64 {
        vextor_geom::bounds(&s.doc, &s.fonts, id)
            .expect("has bounds")
            .x0
    }

    #[test]
    fn a_moved_object_snaps_to_its_neighbour_and_command_frees_it() {
        let (mut s, b) = session();
        let mut h = Harness::new();
        // 47 px left would leave B 3 px from A's right side: it snaps on.
        h.run(
            &mut s,
            drag(Pos2::new(225.0, 125.0), Pos2::new(178.0, 125.0)),
        );
        assert!(
            (left(&s, b) - 150.0).abs() < 1e-9,
            "left at {}",
            left(&s, b)
        );
        assert!(h.canvas.snap.hint.is_none(), "the hint goes on release");

        let (mut s, b) = session();
        let mut h = Harness::new();
        h.modifiers = Modifiers::COMMAND;
        h.run(
            &mut s,
            drag(Pos2::new(225.0, 125.0), Pos2::new(178.0, 125.0)),
        );
        assert!((left(&s, b) - 153.0).abs() < 1e-9, "⌘ moves freely");
    }

    #[test]
    fn a_drawn_rect_starts_on_a_node_and_pixel_snap_rounds_its_corner() {
        let (mut s, _) = session();
        s.select(Vec::new());
        s.editor.tool = ToolId::Rect;
        s.doc.grid.snap.pixel = true;
        let mut h = Harness::new();
        let before: Vec<NodeId> = s.doc.nodes.keys().copied().collect();
        h.run(
            &mut s,
            drag(Pos2::new(151.5, 148.6), Pos2::new(300.4, 249.6)),
        );
        let new = s
            .doc
            .nodes
            .keys()
            .copied()
            .find(|id| !before.contains(id))
            .expect("a rect was drawn");
        let r = vextor_geom::bounds(&s.doc, &s.fonts, new).expect("has bounds");
        // The first corner snapped to A's corner (150, 150); the second
        // landed on whole px.
        assert!(
            (r.x0 - 150.0).abs() < 1e-9 && (r.y0 - 150.0).abs() < 1e-9,
            "{r:?}"
        );
        assert!(
            (r.x1 - 300.0).abs() < 1e-9 && (r.y1 - 250.0).abs() < 1e-9,
            "{r:?}"
        );
    }

    /// A session with one open path (100,100) → (200,100) → (200,200).
    fn path_session() -> (Session, NodeId) {
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

    fn anchors(s: &Session, id: NodeId) -> Vec<(f64, f64)> {
        s.path_of(id).expect("a path").subpaths[0]
            .nodes
            .iter()
            .map(|n| (n.anchor.x, n.anchor.y))
            .collect()
    }

    #[test]
    fn a_dragged_node_snaps_to_another_node_of_its_path() {
        let (mut s, id) = path_session();
        s.select(vec![id]);
        s.editor.tool = ToolId::Node;
        let mut h = Harness::new();
        // From (200, 200) to 5.3 px from (200, 100).
        h.run(
            &mut s,
            drag(Pos2::new(200.0, 200.0), Pos2::new(196.0, 103.5)),
        );
        assert_eq!(anchors(&s, id)[2], (200.0, 100.0));
    }

    #[test]
    fn a_pen_click_snaps_to_a_node() {
        let (mut s, id) = path_session();
        s.editor.tool = ToolId::Pen;
        let mut h = Harness::new();
        let before: Vec<NodeId> = s.doc.nodes.keys().copied().collect();
        // 4.2 px from the path's middle node (not an open end).
        h.run(&mut s, click(Pos2::new(203.0, 103.0)));
        let new = s
            .doc
            .nodes
            .keys()
            .copied()
            .find(|n| !before.contains(n) && *n != id)
            .expect("the pen started a path");
        assert_eq!(anchors(&s, new)[0], (200.0, 100.0));
    }
}
