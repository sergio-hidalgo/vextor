//! Gradient handles on the canvas (spec 09): with the Select tool and one
//! object selected whose fill (else stroke) is a gradient, its start and
//! end (linear) or centre, radius and focal point (radial) are drawn and
//! dragged. A drag is one "Edit gradient" undo step. With the focal point
//! on the centre, a press grabs the centre; hold `⌥` to pull the focal
//! point out instead.

use eframe::egui::{self, Painter, Pos2, Stroke};
use vextor_core::{GradientKind, NodeId, Paint, Point, Resource, ResourceId, ToolId, Transform};

use super::{Canvas, to_egui, to_point};
use crate::paint_edit::{self, GradientPoint, Target};
use crate::session::Session;
use crate::{theme, view};

/// Radius of a drawn handle, and how near a press must land, in points.
const HANDLE_RADIUS: f32 = 4.5;
const HIT: f32 = 8.0;

/// A gradient handle being dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GradientDrag {
    id: ResourceId,
    point: GradientPoint,
}

/// The gradient shown with handles, and the map from its coordinates to
/// the document.
struct Shown {
    id: ResourceId,
    gradient: vextor_core::Gradient,
    to_doc: Transform,
}

/// The gradient of the one selected object that gets handles: its fill,
/// else its stroke. `None` with another tool, several objects, or a flat
/// object whose bounding-box gradient paints nothing.
fn shown(session: &Session) -> Option<Shown> {
    if session.editor.tool != ToolId::Select {
        return None;
    }
    let [id]: [NodeId; 1] = session.selected().try_into().ok()?;
    let node = session.doc.nodes.get(&id)?;
    let target = [Target::Fill, Target::Stroke]
        .into_iter()
        .find(|t| matches!(paint_edit::paint(node, *t), Some(Paint::Gradient(_))))?;
    let Some(Paint::Gradient(rid)) = paint_edit::paint(node, target) else {
        return None;
    };
    let Some(Resource::Gradient(g)) = session.doc.resources.get(&rid) else {
        return None;
    };
    let bbox = vextor_geom::paint_box(&session.fonts, id, node, target.part())?;
    let to_doc = vextor_geom::gradient_space(g.units, bbox)?.then(session.doc.world_transform(id));
    Some(Shown {
        id: rid,
        gradient: g.clone(),
        to_doc,
    })
}

impl Canvas {
    /// Presses on, drags and releases a gradient handle. Returns true when
    /// it used the input (a press on a handle, or a drag in progress).
    pub(super) fn gradient_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        session: &mut Session,
    ) -> bool {
        let (pressed, down, at, alt) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.interact_pos(),
                i.modifiers.alt,
            )
        });
        if let Some(drag) = self.gradient_drag {
            if down && let Some(p) = at {
                drag_to(session, drag, rect, p);
            } else {
                session.end_gesture();
                self.gradient_drag = None;
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            self.gradient_hover = Some(drag.point);
            return true;
        }
        let Some(shown) = shown(session) else {
            return false;
        };
        let hover = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|_| response.hovered())
            .and_then(|p| hit(session, &shown, rect, p, alt));
        if hover.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            self.gradient_hover = hover;
        }
        if pressed
            && response.hovered()
            && let Some(point) = hover
        {
            session.begin_gesture();
            self.gradient_drag = Some(GradientDrag {
                id: shown.id,
                point,
            });
            return true;
        }
        false
    }

    /// The handles of the selected object's gradient: a line from start to
    /// end, or the circle with its centre, radius and focal handles.
    pub(super) fn draw_gradient_handles(&self, painter: &Painter, origin: Pos2, session: &Session) {
        let Some(shown) = shown(session) else {
            return;
        };
        let screen = |p: Point| origin + to_egui(view::doc_to_screen(session.editor.view, p));
        let to_screen = |p: Point| screen(shown.to_doc.apply(p));
        let dark = Stroke::new(3.0, theme::GREY_500);
        let light = Stroke::new(1.0, theme::WHITE);
        let points = paint_edit::gradient_points(&shown.gradient);
        match shown.gradient.kind {
            GradientKind::Linear { start, end } => {
                let line = [to_screen(start), to_screen(end)];
                painter.line_segment(line, dark);
                painter.line_segment(line, light);
            }
            GradientKind::Radial { center, radius, .. } => {
                let circle: Vec<Pos2> = (0..=48)
                    .map(|i| {
                        let a = f64::from(i) / 48.0 * std::f64::consts::TAU;
                        let (sin, cos) = a.sin_cos();
                        to_screen(Point::new(center.x + radius * cos, center.y + radius * sin))
                    })
                    .collect();
                painter.add(egui::Shape::line(circle.clone(), dark));
                painter.add(egui::Shape::line(circle, light));
            }
        }
        let dragged = self.gradient_drag.map(|d| d.point);
        for (point, at) in points {
            let c = to_screen(at);
            let fill = if Some(point) == dragged {
                theme::YELLOW
            } else if point == GradientPoint::Focal {
                theme::GREY_400
            } else {
                theme::WHITE
            };
            let r = if point == GradientPoint::Focal {
                HANDLE_RADIUS - 1.5
            } else {
                HANDLE_RADIUS
            };
            painter.circle(c, r, fill, Stroke::new(1.0, theme::GREY_500));
        }
    }
}

/// The handle under screen point `p`, the nearest. `⌥` grabs the focal
/// point first; without it, a focal point lying on the centre is left to
/// the centre.
fn hit(
    session: &Session,
    shown: &Shown,
    rect: egui::Rect,
    p: Pos2,
    alt: bool,
) -> Option<GradientPoint> {
    let screen = |q: Point| {
        rect.min
            + to_egui(view::doc_to_screen(
                session.editor.view,
                shown.to_doc.apply(q),
            ))
    };
    let points: Vec<(GradientPoint, Pos2)> = paint_edit::gradient_points(&shown.gradient)
        .into_iter()
        .map(|(point, at)| (point, screen(at)))
        .collect();
    let at = |want: GradientPoint| points.iter().find(|(pt, _)| *pt == want).map(|(_, q)| *q);
    if alt && at(GradientPoint::Focal).is_some_and(|q| q.distance(p) <= HIT) {
        return Some(GradientPoint::Focal);
    }
    let centre = at(GradientPoint::Center);
    points
        .into_iter()
        .filter(|(pt, q)| {
            *pt != GradientPoint::Focal || centre.is_none_or(|c| c.distance(*q) >= 1.0)
        })
        .map(|(pt, q)| (pt, q.distance(p)))
        .filter(|(_, d)| *d <= HIT)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(pt, _)| pt)
}

/// Moves the dragged handle to screen point `p`.
fn drag_to(session: &mut Session, drag: GradientDrag, rect: egui::Rect, p: Pos2) {
    let Some(shown) = shown(session).filter(|s| s.id == drag.id) else {
        return;
    };
    let Some(from_doc) = shown.to_doc.invert() else {
        return;
    };
    let doc = view::screen_to_doc(session.editor.view, to_point(p - rect.min));
    let at = from_doc.apply(doc);
    if let Some(g) = paint_edit::move_point(&shown.gradient, drag.point, at)
        && g != shown.gradient
    {
        // Refused only for invalid geometry, which the drag never makes.
        let _ = session.apply(paint_edit::edit_resource(drag.id, Resource::Gradient(g)));
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::Pos2;
    use vextor_core::{Command, Geometry, Node, NodeKind, Rect, Rgba, Shape, Style};

    use super::super::harness::{Harness, drag};
    use super::*;
    use crate::new_doc::new_document;
    use crate::paint_edit::{GradientShape, new_gradient};

    /// A 200×100 rect at (100, 100), selected, filled with a linear
    /// gradient running from its left middle to its top-right corner.
    fn session() -> (Session, NodeId, ResourceId) {
        let mut doc = new_document("g", 400.0, 300.0);
        let id = doc.alloc_node_id();
        let mut g = new_gradient(GradientShape::Linear, Rgba::rgb(255, 0, 0), None);
        g.kind = GradientKind::Linear {
            start: Point::new(0.0, 0.5),
            end: Point::new(1.0, 0.0),
        };
        let rid = doc.alloc_resource_id();
        doc.apply(Command::PutResource {
            id: rid,
            resource: Resource::Gradient(g),
            nodes: vec![],
        })
        .expect("gradient");
        let rect = Node::new(
            id,
            NodeKind::Shape(Shape {
                geometry: Geometry::Rect {
                    rect: Rect::new(100.0, 100.0, 200.0, 100.0),
                    radius: vextor_core::CornerRadii::default(),
                },
                style: Style {
                    fill: Paint::Gradient(rid),
                    stroke: None,
                },
            }),
        );
        doc.apply(Command::Insert {
            parent: Some(doc.layers[0]),
            index: 0,
            nodes: vec![rect],
        })
        .expect("rect");
        let mut s = Session::new(doc);
        s.select(vec![id]);
        (s, id, rid)
    }

    fn end(s: &Session, rid: ResourceId) -> Point {
        match s.doc.resources.get(&rid) {
            Some(Resource::Gradient(g)) => match g.kind {
                GradientKind::Linear { end, .. } => end,
                GradientKind::Radial { center, .. } => center,
            },
            _ => panic!("gradient"),
        }
    }

    #[test]
    fn a_handle_on_the_corner_of_the_box_drags_the_gradient_not_the_shape() {
        let (mut s, id, rid) = session();
        let before = s.doc.nodes[&id].clone();
        // The end handle sits on the top-right scale handle and next to
        // the rotate zone.
        Harness::new().run(
            &mut s,
            drag(Pos2::new(300.0, 100.0), Pos2::new(250.0, 150.0)),
        );
        let e = end(&s, rid);
        assert!(
            (e.x - 0.75).abs() < 1e-6 && (e.y - 0.5).abs() < 1e-6,
            "{e:?}"
        );
        assert_eq!(
            s.doc.nodes[&id], before,
            "the shape did not move, scale or turn"
        );
        assert_eq!(s.edit_menu().undo, Some("Edit gradient"));
    }

    #[test]
    fn a_handle_in_the_rotate_zone_drags_the_gradient_not_the_shape() {
        let (mut s, id, rid) = session();
        let g = match s.doc.resources.get(&rid) {
            Some(Resource::Gradient(g)) => g.clone(),
            _ => unreachable!("gradient"),
        };
        let moved =
            paint_edit::move_point(&g, GradientPoint::End, Point::new(1.02, 0.1)).expect("end");
        s.apply(paint_edit::edit_resource(rid, Resource::Gradient(moved)))
            .expect("applies");
        let before = s.doc.nodes[&id].clone();
        // (304, 110): just right of the box, in the rotate zone. Hovering
        // shows the handle's grab cursor, not the rotate one.
        let mut h = Harness::new();
        let at = Pos2::new(304.0, 110.0);
        h.run(&mut s, vec![vec![egui::Event::PointerMoved(at)], vec![]]);
        assert_eq!(
            h.canvas.gradient_hover,
            Some(GradientPoint::End),
            "the handle wins the cursor"
        );
        h.run(&mut s, drag(at, Pos2::new(250.0, 150.0)));
        let e = end(&s, rid);
        assert!(
            (e.x - 0.75).abs() < 1e-6 && (e.y - 0.5).abs() < 1e-6,
            "{e:?}"
        );
        assert_eq!(s.doc.nodes[&id], before, "the shape did not turn");
    }
}
