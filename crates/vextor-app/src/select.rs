//! The selection tool (`V`) as a state machine fed with pointer events in
//! document coordinates, so it is tested without a window.
//!
//! Press on a handle scales, just outside a corner rotates, on an object or
//! inside the selection box moves, on empty canvas starts a marquee. A whole
//! drag is one undo step (a history gesture).

use kurbo::{Affine, Point, Rect};
use vextor_core::{Command, Document, NodeId};
use vextor_geom::{Handle, pick, retarget, rotate, rotation_angle, scale, touching, union_bounds};

use crate::session::Session;

/// Half the side of a handle's hit area, in screen points (an 8 pt square,
/// the `ui-editor` minimum; larger squares swallow small objects).
pub const HANDLE_HIT: f64 = 4.0;
/// How far outside a corner the rotate zone reaches, in screen points.
pub const ROTATE_REACH: f64 = 20.0;
/// Pick tolerance around outlines, in screen points.
pub const PICK_TOLERANCE: f64 = 4.0;
/// Movement before a press becomes a drag, in screen points.
const DRAG_THRESHOLD: f64 = 3.0;

/// Modifier keys that change a drag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    /// Shift: add to the selection; keep proportions; 15° steps.
    pub shift: bool,
    /// Alt / Option: scale from the centre.
    pub alt: bool,
}

/// What the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Handle(Handle),
    Rotate,
    Object(NodeId),
    /// Inside the selection box, not on an object.
    Inside,
    Empty,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Move,
    Scale(Handle),
    Rotate,
}

#[derive(Debug, Default)]
enum Drag {
    #[default]
    Idle,
    Marquee {
        start: Point,
        now: Point,
        /// The selection the marquee adds to (Shift), else empty.
        base: Vec<NodeId>,
    },
    Transform {
        kind: Kind,
        start: Point,
        /// The document when the press began; every frame is computed from
        /// it so the drag never accumulates error.
        before: Box<Document>,
        ids: Vec<NodeId>,
        bounds: Rect,
        /// Past the drag threshold.
        moved: bool,
        /// The object pressed on, to select it alone on a click without drag.
        clicked: Option<NodeId>,
        /// The document-space change applied by the last frame.
        change: Option<Affine>,
    },
}

/// What a transform drag shows: the press-time document, the dragged
/// objects and the change applied to them so far (document space).
pub struct Preview<'a> {
    pub before: &'a Document,
    pub ids: &'a [NodeId],
    pub change: Affine,
}

/// The selection tool's state between events.
#[derive(Debug, Default)]
pub struct SelectTool {
    drag: Drag,
}

impl SelectTool {
    /// True while a drag is in progress.
    #[must_use]
    pub fn is_dragging(&self) -> bool {
        !matches!(self.drag, Drag::Idle)
    }

    /// True while the selection is being moved.
    #[must_use]
    pub fn is_moving(&self) -> bool {
        matches!(
            self.drag,
            Drag::Transform {
                kind: Kind::Move,
                moved: true,
                ..
            }
        )
    }

    /// True while the selection is being rotated.
    #[must_use]
    pub fn rotating(&self) -> bool {
        matches!(
            self.drag,
            Drag::Transform {
                kind: Kind::Rotate,
                ..
            }
        )
    }

    /// The handle being dragged, while scaling.
    #[must_use]
    pub fn scaling(&self) -> Option<Handle> {
        match self.drag {
            Drag::Transform {
                kind: Kind::Scale(h),
                ..
            } => Some(h),
            _ => None,
        }
    }

    /// The transform drag in progress, from the press (no change yet) on.
    #[must_use]
    pub fn preview(&self) -> Option<Preview<'_>> {
        match &self.drag {
            Drag::Transform {
                before,
                ids,
                change,
                ..
            } => Some(Preview {
                before,
                ids,
                change: change.unwrap_or(Affine::IDENTITY),
            }),
            _ => None,
        }
    }

    /// The marquee rectangle (document space) while one is drawn.
    #[must_use]
    pub fn marquee(&self) -> Option<Rect> {
        match self.drag {
            Drag::Marquee { start, now, .. } => Some(Rect::from_points(start, now)),
            _ => None,
        }
    }

    /// What is under `p` at `zoom` (screen points per document px).
    #[must_use]
    pub fn target(s: &Session, p: Point, zoom: f64) -> Target {
        let selected = s.selected();
        if let Some(b) = union_bounds(&s.doc, &s.fonts, &selected) {
            let near = |q: Point, reach: f64| {
                (p.x - q.x).abs() * zoom <= reach && (p.y - q.y).abs() * zoom <= reach
            };
            let nearest = Handle::ALL
                .into_iter()
                .filter(|h| near(h.position(b), HANDLE_HIT))
                .min_by(|x, y| {
                    let d = |h: &Handle| (h.position(b) - p).hypot();
                    d(x).total_cmp(&d(y))
                });
            if let Some(h) = nearest {
                return Target::Handle(h);
            }
            let outside = !b.contains(p);
            if outside
                && Handle::ALL
                    .into_iter()
                    .filter(|h| h.is_corner())
                    .any(|h| near(h.position(b), ROTATE_REACH))
            {
                return Target::Rotate;
            }
            if let Some(o) = pick(&s.doc, &s.fonts, p, PICK_TOLERANCE / zoom) {
                return Target::Object(o);
            }
            if b.contains(p) {
                return Target::Inside;
            }
            return Target::Empty;
        }
        pick(&s.doc, &s.fonts, p, PICK_TOLERANCE / zoom).map_or(Target::Empty, Target::Object)
    }

    /// Primary button pressed at `p`.
    pub fn press(&mut self, s: &mut Session, p: Point, mods: Mods, zoom: f64) {
        let mut clicked = None;
        let kind = match Self::target(s, p, zoom) {
            Target::Handle(h) => Kind::Scale(h),
            Target::Rotate => Kind::Rotate,
            Target::Inside => Kind::Move,
            Target::Object(o) => {
                let mut sel = s.selected();
                if mods.shift {
                    if let Some(i) = sel.iter().position(|&id| id == o) {
                        sel.remove(i);
                        s.select(sel);
                        return;
                    }
                    sel.push(o);
                    s.select(sel);
                } else if sel.contains(&o) {
                    clicked = Some(o);
                } else {
                    s.select(vec![o]);
                }
                Kind::Move
            }
            Target::Empty => {
                let base = if mods.shift { s.selected() } else { Vec::new() };
                if !mods.shift {
                    s.select(Vec::new());
                }
                self.drag = Drag::Marquee {
                    start: p,
                    now: p,
                    base,
                };
                return;
            }
        };
        let ids = s.selected();
        let Some(bounds) = union_bounds(&s.doc, &s.fonts, &ids) else {
            return;
        };
        s.begin_gesture();
        self.drag = Drag::Transform {
            kind,
            start: p,
            before: Box::new(s.doc.clone()),
            ids,
            bounds,
            moved: false,
            clicked,
            change: None,
        };
    }

    /// The pointer moved to `p` with the button held.
    ///
    /// # Errors
    /// A message for the user when the change is refused.
    pub fn drag_to(
        &mut self,
        s: &mut Session,
        p: Point,
        mods: Mods,
        zoom: f64,
    ) -> Result<(), String> {
        match &mut self.drag {
            Drag::Idle => Ok(()),
            Drag::Marquee { start, now, base } => {
                *now = p;
                let mut sel = base.clone();
                for id in touching(&s.doc, &s.fonts, Rect::from_points(*start, p)) {
                    if !sel.contains(&id) {
                        sel.push(id);
                    }
                }
                s.select(sel);
                Ok(())
            }
            Drag::Transform {
                kind,
                start,
                before,
                ids,
                bounds,
                moved,
                change: last,
                ..
            } => {
                let delta = p - *start;
                if !*moved && delta.hypot() * zoom < DRAG_THRESHOLD {
                    return Ok(());
                }
                *moved = true;
                let (change, label) = transform_for(*kind, *bounds, *start, p, mods);
                *last = Some(change);
                // A scale is baked into the shapes so strokes keep their
                // width; moves and rotations only change transforms.
                let commands = if matches!(kind, Kind::Scale(_)) {
                    crate::edit_ops::resize_commands(before, ids, change)
                } else {
                    vec![Command::SetTransforms {
                        items: retarget(before, ids, change),
                    }]
                };
                s.apply(Command::Batch { label, commands })
                    .map_err(|e| format!("{label} failed: {e}"))
            }
        }
    }

    /// Primary button released.
    pub fn release(&mut self, s: &mut Session) {
        if let Drag::Transform { moved, clicked, .. } = &self.drag {
            s.end_gesture();
            // A click (no drag) on one object of a multi-selection selects
            // just that object.
            if !moved && let Some(o) = clicked {
                s.select(vec![*o]);
            }
        }
        self.drag = Drag::Idle;
    }
}

/// The document-space change for a drag from `start` to `now`, and its
/// undo label.
fn transform_for(
    kind: Kind,
    b: Rect,
    start: Point,
    now: Point,
    mods: Mods,
) -> (Affine, &'static str) {
    match kind {
        Kind::Move => (Affine::translate(now - start), "Move"),
        Kind::Scale(h) => (scale(b, h, now - start, mods.shift, mods.alt), "Scale"),
        Kind::Rotate => {
            let c = b.center();
            (
                rotate(c, rotation_angle(c, start, now, mods.shift)),
                "Rotate",
            )
        }
    }
}

/// The selection's box handles, for drawing: `(handle, position)`.
#[must_use]
pub fn handles(b: Rect) -> Vec<(Handle, Point)> {
    Handle::ALL.iter().map(|&h| (h, h.position(b))).collect()
}

#[cfg(test)]
mod tests {
    use vextor_core::{CornerRadii, Geometry, Node, NodeKind, Shape, Style};

    use super::*;
    use crate::new_doc::new_document;

    const NONE: Mods = Mods {
        shift: false,
        alt: false,
    };
    const SHIFT: Mods = Mods {
        shift: true,
        alt: false,
    };

    /// Two 10×10 rects at (0, 0) and (40, 0).
    fn session() -> (Session, NodeId, NodeId) {
        let mut doc = new_document("a", 100.0, 100.0);
        let layer = doc.layers[0];
        let (a, b) = (doc.alloc_node_id(), doc.alloc_node_id());
        let rect = |id, x| {
            Node::new(
                id,
                NodeKind::Shape(Shape {
                    geometry: Geometry::Rect {
                        rect: vextor_core::Rect::new(x, 0.0, 10.0, 10.0),
                        radius: CornerRadii::default(),
                    },
                    style: Style::default(),
                }),
            )
        };
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![rect(a, 0.0), rect(b, 40.0)],
        })
        .expect("inserts");
        (Session::new(doc), a, b)
    }

    fn bounds(s: &Session, id: NodeId) -> Rect {
        vextor_geom::bounds(&s.doc, &s.fonts, id).expect("visible")
    }

    fn click(t: &mut SelectTool, s: &mut Session, p: Point, mods: Mods) {
        t.press(s, p, mods, 1.0);
        t.release(s);
    }

    fn drag(t: &mut SelectTool, s: &mut Session, from: Point, to: Point, mods: Mods) {
        t.press(s, from, mods, 1.0);
        t.drag_to(
            s,
            Point::new(f64::midpoint(from.x, to.x), f64::midpoint(from.y, to.y)),
            mods,
            1.0,
        )
        .expect("drags");
        t.drag_to(s, to, mods, 1.0).expect("drags");
        t.release(s);
    }

    #[test]
    fn click_selects_shift_click_toggles_empty_click_clears() {
        let (mut s, a, b) = session();
        let mut t = SelectTool::default();
        click(&mut t, &mut s, Point::new(5.0, 5.0), NONE);
        assert_eq!(s.selected(), vec![a]);
        click(&mut t, &mut s, Point::new(45.0, 5.0), SHIFT);
        assert_eq!(s.selected(), vec![a, b]);
        click(&mut t, &mut s, Point::new(5.0, 5.0), SHIFT);
        assert_eq!(s.selected(), vec![b]);
        click(&mut t, &mut s, Point::new(80.0, 80.0), NONE);
        assert_eq!(s.selected(), vec![]);
    }

    #[test]
    fn click_without_drag_inside_a_multi_selection_selects_one() {
        let (mut s, a, b) = session();
        s.select(vec![a, b]);
        let mut t = SelectTool::default();
        click(&mut t, &mut s, Point::new(45.0, 5.0), NONE);
        assert_eq!(s.selected(), vec![b]);
    }

    #[test]
    fn marquee_selects_what_it_touches_and_shift_adds() {
        let (mut s, a, b) = session();
        let mut t = SelectTool::default();
        drag(
            &mut t,
            &mut s,
            Point::new(-5.0, -5.0),
            Point::new(5.0, 20.0),
            NONE,
        );
        assert_eq!(s.selected(), vec![a]);
        assert!(t.marquee().is_none(), "gone after release");
        drag(
            &mut t,
            &mut s,
            Point::new(60.0, 20.0),
            Point::new(45.0, 5.0),
            SHIFT,
        );
        assert_eq!(s.selected(), vec![a, b]);
    }

    #[test]
    fn dragging_an_object_moves_it_as_one_undo_step() {
        let (mut s, a, _) = session();
        let mut t = SelectTool::default();
        let before = s.doc.clone();
        drag(
            &mut t,
            &mut s,
            Point::new(5.0, 5.0),
            Point::new(25.0, 15.0),
            NONE,
        );
        assert_eq!(s.selected(), vec![a]);
        assert_eq!(bounds(&s, a).origin(), Point::new(20.0, 10.0));
        assert_eq!(s.edit_menu().undo, Some("Move"));
        assert!(s.undo());
        assert_eq!(s.doc, before);
    }

    #[test]
    fn tiny_drags_are_clicks() {
        let (mut s, a, _) = session();
        let mut t = SelectTool::default();
        let before = s.doc.clone();
        drag(
            &mut t,
            &mut s,
            Point::new(5.0, 5.0),
            Point::new(6.0, 6.0),
            NONE,
        );
        assert_eq!(s.doc, before);
        assert_eq!(s.selected(), vec![a]);
        assert_eq!(s.edit_menu().undo, None);
    }

    #[test]
    fn corner_handle_scales_and_shift_keeps_proportions() {
        let (mut s, a, _) = session();
        s.select(vec![a]);
        let mut t = SelectTool::default();
        assert_eq!(
            SelectTool::target(&s, Point::new(10.0, 10.0), 1.0),
            Target::Handle(Handle::SE)
        );
        drag(
            &mut t,
            &mut s,
            Point::new(10.0, 10.0),
            Point::new(30.0, 15.0),
            SHIFT,
        );
        let b = bounds(&s, a);
        assert!((b.width() - 30.0).abs() < 1e-9 && (b.height() - 30.0).abs() < 1e-9);
        assert_eq!(s.edit_menu().undo, Some("Scale"));
    }

    #[test]
    fn scaling_resizes_the_shape_not_its_transform() {
        let (mut s, a, _) = session();
        s.select(vec![a]);
        let mut t = SelectTool::default();
        let before = s.doc.clone();
        // Stretch only horizontally, from the E handle.
        drag(
            &mut t,
            &mut s,
            Point::new(10.0, 5.0),
            Point::new(40.0, 5.0),
            NONE,
        );
        assert_eq!(s.doc.nodes[&a].transform, vextor_core::Transform::IDENTITY);
        let NodeKind::Shape(shape) = &s.doc.nodes[&a].kind else {
            panic!("a shape");
        };
        assert!(matches!(
            shape.geometry,
            Geometry::Rect { rect, .. } if (rect.width - 40.0).abs() < 1e-9
        ));
        assert!(s.undo());
        assert_eq!(s.doc, before, "one undo step");
    }

    #[test]
    fn outside_a_corner_rotates_in_15_degree_steps() {
        let (mut s, a, _) = session();
        s.select(vec![a]);
        let mut t = SelectTool::default();
        // 12 pt outside the NE corner (10, 0).
        let start = Point::new(18.0, -8.0);
        assert_eq!(SelectTool::target(&s, start, 1.0), Target::Rotate);
        // About the centre (5, 5): from -45° to about 50° → snaps to 90°.
        drag(&mut t, &mut s, start, Point::new(18.5, 20.0), SHIFT);
        let angle = vextor_geom::rotation_degrees(s.doc.world_transform(a));
        assert!((angle - 90.0).abs() < 1e-6, "{angle}");
        assert_eq!(s.edit_menu().undo, Some("Rotate"));
    }

    #[test]
    fn every_corner_has_a_rotate_zone() {
        let (mut s, a, _) = session();
        s.select(vec![a]);
        // 8 pt diagonally outside each corner of the 0..10 box.
        for p in [(-8.0, -8.0), (18.0, -8.0), (18.0, 18.0), (-8.0, 18.0)] {
            assert_eq!(
                SelectTool::target(&s, Point::new(p.0, p.1), 1.0),
                Target::Rotate,
                "{p:?}"
            );
        }
    }

    #[test]
    fn handles_are_reached_from_further_away_when_zoomed_out() {
        let (mut s, a, _) = session();
        s.select(vec![a]);
        // At 50 %, 4 pt of reach is 8 document px.
        assert_eq!(
            SelectTool::target(&s, Point::new(17.0, 17.0), 0.5),
            Target::Handle(Handle::SE)
        );
        assert_ne!(
            SelectTool::target(&s, Point::new(17.0, 17.0), 1.0),
            Target::Handle(Handle::SE)
        );
    }
}
