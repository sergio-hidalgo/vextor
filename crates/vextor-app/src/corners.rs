//! Rect corner widgets (spec 06): with the selection tool and one rect
//! selected, a dot inside each corner rounds the corners when dragged —
//! all four, or with Alt only the one dragged. One drag is one undo step.
//! The radius maths is `vextor_geom::corner_radius_at`.

use kurbo::{Affine, Point};
use vextor_core::{Command, CornerRadii, Document, Geometry, NodeId, NodeKind, Rect};
use vextor_geom::{Corner, corner_radius_at, corner_widget, fitted_radii, to_affine};

use crate::session::Session;

/// How far in from its corner a widget sits at least, in screen points.
pub const MIN_INSET: f64 = 12.0;
/// How near a widget the pointer must be to grab it, in screen points.
pub const HIT: f64 = 6.0;
/// Widgets show only on rects at least this big on screen, in points, so
/// they never crowd the scale handles.
pub const MIN_SIDE: f64 = 40.0;

/// Undo label of a corner drag.
pub const LABEL: &str = "Round Corners";

/// The one selected rect: its id, rect, radii and world transform.
fn selected_rect(s: &Session) -> Option<(NodeId, Rect, CornerRadii, Affine)> {
    let [id] = s.selected()[..] else {
        return None;
    };
    let node = s.doc.nodes.get(&id)?;
    let NodeKind::Shape(shape) = &node.kind else {
        return None;
    };
    let Geometry::Rect { rect, radius } = shape.geometry else {
        return None;
    };
    Some((id, rect, radius, to_affine(s.doc.world_transform(id))))
}

/// The smaller scale of `a` (document px per shape px).
fn scale(a: Affine) -> f64 {
    let [m0, m1, m2, m3, _, _] = a.as_coeffs();
    m0.hypot(m1).min(m2.hypot(m3))
}

/// Each corner's widget in document space, for the one selected rect when
/// it is big enough on screen at `zoom`; empty otherwise.
#[must_use]
pub fn widgets(s: &Session, zoom: f64) -> Vec<(Corner, Point)> {
    let Some((_, rect, radii, world)) = selected_rect(s) else {
        return Vec::new();
    };
    let px = zoom * scale(world);
    if px <= vextor_core::EPS || rect.width.min(rect.height) * px < MIN_SIDE {
        return Vec::new();
    }
    Corner::ALL
        .into_iter()
        .map(|c| (c, world * corner_widget(rect, radii, c, MIN_INSET / px)))
        .collect()
}

/// The widget under `p` (document space) at `zoom`, if any.
#[must_use]
pub fn target(s: &Session, p: Point, zoom: f64) -> Option<Corner> {
    widgets(s, zoom)
        .into_iter()
        .find(|(_, w)| (*w - p).hypot() * zoom <= HIT)
        .map(|(c, _)| c)
}

/// A corner drag in progress.
#[derive(Clone, Debug, PartialEq)]
pub struct CornerDrag {
    id: NodeId,
    corner: Corner,
    rect: Rect,
    /// The radii as drawn when the drag began.
    radii: CornerRadii,
    /// Document space → the rect's own space.
    to_local: Affine,
    from: Point,
    /// The document when the press began, for the canvas's fast path.
    before: Box<Document>,
}

impl CornerDrag {
    /// Grabs the widget under `p` (document space), if any, and starts
    /// the drag's undo step.
    #[must_use]
    pub fn press(s: &mut Session, p: Point, zoom: f64) -> Option<Self> {
        let corner = target(s, p, zoom)?;
        let (id, rect, radius, world) = selected_rect(s)?;
        let to_local = world.inverse();
        s.begin_gesture();
        Some(Self {
            id,
            corner,
            rect,
            radii: fitted_radii(rect, radius),
            to_local,
            from: to_local * p,
            before: Box::new(s.doc.clone()),
        })
    }

    /// The press-time document and the rect being rounded.
    #[must_use]
    pub fn preview(&self) -> (&Document, NodeId) {
        (&self.before, self.id)
    }

    /// The pointer moved to `p`: every corner takes the new radius, or
    /// only the dragged one with `one` (Alt).
    ///
    /// # Errors
    /// A message for the user when the change is refused.
    pub fn drag_to(&self, s: &mut Session, p: Point, one: bool) -> Result<(), String> {
        let start = self.corner.radius(self.radii);
        let r = corner_radius_at(self.rect, self.corner, start, self.from, self.to_local * p);
        let radius = if one {
            self.corner.with(self.radii, r)
        } else {
            CornerRadii {
                tl: r,
                tr: r,
                br: r,
                bl: r,
            }
        };
        let geometry = Geometry::Rect {
            rect: self.rect,
            radius,
        };
        s.apply(Command::Batch {
            label: LABEL,
            commands: vec![Command::SetGeometry {
                items: vec![(self.id, geometry)],
            }],
        })
        .map_err(|e| format!("Round corners failed: {e}"))
    }

    /// The pointer was released: the drag's undo step ends.
    #[allow(clippy::unused_self)] // taken by value: the drag is over
    pub fn release(self, s: &mut Session) {
        s.end_gesture();
    }
}

#[cfg(test)]
mod tests {
    use vextor_core::{Node, Shape, Style, Transform};

    use super::*;
    use crate::new_doc::new_document;

    /// A session with one selected 100×60 rect at (0, 0), moved by `t`.
    fn session(t: Transform) -> (Session, NodeId) {
        let mut doc = new_document("a", 400.0, 200.0);
        let layer = doc.layers[0];
        let id = doc.alloc_node_id();
        let mut node = Node::new(
            id,
            NodeKind::Shape(Shape {
                geometry: Geometry::Rect {
                    rect: Rect::new(0.0, 0.0, 100.0, 60.0),
                    radius: CornerRadii::default(),
                },
                style: Style::default(),
            }),
        );
        node.transform = t;
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![node],
        })
        .expect("inserts");
        let mut s = Session::new(doc);
        s.select(vec![id]);
        (s, id)
    }

    fn radii(s: &Session, id: NodeId) -> CornerRadii {
        match &s.doc.nodes[&id].kind {
            NodeKind::Shape(Shape {
                geometry: Geometry::Rect { radius, .. },
                ..
            }) => *radius,
            other => panic!("rect, got {other:?}"),
        }
    }

    #[test]
    fn widgets_sit_inside_the_corners_at_any_zoom() {
        let (s, _) = session(Transform::IDENTITY);
        let w = widgets(&s, 1.0);
        assert_eq!(w[0], (Corner::TopLeft, Point::new(12.0, 12.0)));
        assert_eq!(w[2], (Corner::BottomRight, Point::new(88.0, 48.0)));
        let zoomed = widgets(&s, 2.0);
        assert_eq!(zoomed[0].1, Point::new(6.0, 6.0), "12 screen points in");
    }

    #[test]
    fn small_rects_and_other_selections_have_no_widgets() {
        let (mut s, _) = session(Transform::IDENTITY);
        assert!(widgets(&s, 0.5).is_empty(), "30 points tall on screen");
        s.select(Vec::new());
        assert_eq!(widgets(&s, 1.0), vec![]);
    }

    #[test]
    fn dragging_a_widget_rounds_all_corners_in_one_step() {
        let (mut s, id) = session(Transform::IDENTITY);
        let drag = CornerDrag::press(&mut s, Point::new(12.0, 12.0), 1.0).expect("grabs");
        drag.drag_to(&mut s, Point::new(14.0, 14.0), false)
            .expect("rounds");
        drag.drag_to(&mut s, Point::new(20.0, 20.0), false)
            .expect("rounds");
        drag.release(&mut s);
        let r = radii(&s, id);
        assert_eq!((r.tl, r.tr, r.br, r.bl), (8.0, 8.0, 8.0, 8.0));
        assert_eq!(s.edit_menu().undo, Some(LABEL));
        s.undo();
        assert_eq!(radii(&s, id), CornerRadii::default(), "one undo step");
    }

    #[test]
    fn alt_rounds_only_the_dragged_corner() {
        let (mut s, id) = session(Transform::IDENTITY);
        let drag = CornerDrag::press(&mut s, Point::new(88.0, 48.0), 1.0).expect("grabs");
        drag.drag_to(&mut s, Point::new(78.0, 38.0), true)
            .expect("rounds");
        drag.release(&mut s);
        let r = radii(&s, id);
        assert_eq!((r.tl, r.tr, r.br, r.bl), (0.0, 0.0, 10.0, 0.0));
    }

    #[test]
    fn widgets_follow_a_rotated_rect() {
        // A quarter turn about the origin: (x, y) → (-y, x).
        let (mut s, id) = session(Transform([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]));
        let w = widgets(&s, 1.0);
        let tl = w[0].1;
        assert!(
            (tl.x + 12.0).abs() < 1e-9 && (tl.y - 12.0).abs() < 1e-9,
            "{tl:?}"
        );
        let drag = CornerDrag::press(&mut s, tl, 1.0).expect("grabs");
        // Inward along the rotated diagonal by 4 √2: radius 4.
        drag.drag_to(&mut s, Point::new(tl.x - 4.0, tl.y + 4.0), false)
            .expect("rounds");
        drag.release(&mut s);
        assert!((radii(&s, id).tl - 4.0).abs() < 1e-9);
    }

    #[test]
    fn a_press_away_from_the_widgets_grabs_nothing() {
        let (mut s, _) = session(Transform::IDENTITY);
        assert!(CornerDrag::press(&mut s, Point::new(50.0, 30.0), 1.0).is_none());
    }
}
