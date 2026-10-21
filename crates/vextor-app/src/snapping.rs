//! Snapping in the editor (spec 08): the one place every tool sends its
//! dragged point or box through, and what the canvas shows about it.
//!
//! A drag freezes its targets at press ([`Snapper::begin`]) so they never
//! include what it moves; the pen's clicks and hover use targets kept
//! until the document changes. `⌘` suspends snapping (and pixel snap).

use kurbo::{Point, Rect, Vec2};
use vextor_core::{NodeId, PathNodeRef};
use vextor_geom::{Guide, SnapKind, SnapTargets, pixel_snap, tolerance};

use crate::path_ops::world;
use crate::session::Session;

/// What the last snap landed on, for the canvas hint.
#[derive(Clone, Debug, PartialEq)]
pub struct Hint {
    /// Where the snapped point (or the moved box's snapped side) is.
    pub at: Point,
    pub kind: SnapKind,
    /// Smart guides of a box move.
    pub guides: Vec<Guide>,
}

/// Snapping state between frames.
#[derive(Debug, Default)]
pub struct Snapper {
    /// The targets of the drag in progress.
    frozen: Option<SnapTargets>,
    /// Targets for clicks and hover, and the revision they were taken at.
    live: Option<(u64, SnapTargets)>,
    /// The last snap, while it applies.
    pub hint: Option<Hint>,
}

impl Snapper {
    /// Starts a drag of `exclude` (its objects are no targets); `nodes`
    /// count as node targets (the edited path's other nodes).
    pub fn begin(&mut self, s: &Session, exclude: &[NodeId], nodes: Vec<Point>) {
        let mut t = SnapTargets::collect(&s.doc, &s.fonts, exclude);
        t.add_nodes(nodes);
        self.frozen = Some(t);
        self.hint = None;
    }

    /// Starts a drag of the artboard with bounds `artboard` and the
    /// objects `exclude` on it: none of them are targets (spec 08b).
    pub fn begin_artboard(&mut self, s: &Session, artboard: Rect, exclude: &[NodeId]) {
        let mut t = SnapTargets::collect(&s.doc, &s.fonts, exclude);
        t.without_artboard(artboard);
        self.frozen = Some(t);
        self.hint = None;
    }

    /// Ends the drag: targets and hint go.
    pub fn end(&mut self) {
        self.frozen = None;
        self.hint = None;
    }

    /// True while a drag's targets are frozen.
    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.frozen.is_some()
    }

    /// `p` snapped (unless `free`), and on whole px when `pixel` and the
    /// document's pixel snap are on.
    pub fn point(&mut self, s: &Session, p: Point, free: bool, pixel: bool) -> Point {
        self.hint = None;
        if free {
            return p;
        }
        let tol = tolerance(&s.doc.grid.snap, s.editor.view.zoom);
        let snapped = self.targets(s).point(p, tol);
        let mut q = snapped.map_or(p, |snap| snap.point);
        if pixel && s.doc.grid.snap.pixel {
            q = pixel_snap(q);
        }
        if let Some(snap) = snapped {
            self.hint = Some(Hint {
                at: q,
                kind: snap.kind,
                guides: Vec::new(),
            });
        }
        q
    }

    /// The pointer `p` of a drag that started at `start` with the grabbed
    /// point at `anchor`, moved so the grabbed point snaps.
    pub fn grabbed(
        &mut self,
        s: &Session,
        start: Point,
        anchor: Point,
        p: Point,
        free: bool,
    ) -> Point {
        let moved = anchor + (p - start);
        p + (self.point(s, moved, free, false) - moved)
    }

    /// The offset that snaps box `b` (the moved selection where the
    /// pointer puts it), unless `free`.
    pub fn moving_box(&mut self, s: &Session, b: Rect, free: bool) -> Vec2 {
        self.hint = None;
        if free {
            return Vec2::ZERO;
        }
        let tol = tolerance(&s.doc.grid.snap, s.editor.view.zoom);
        let snap = self.targets(s).moving_box(b, tol);
        if let Some(kind) = snap.kind() {
            let landed = b + snap.offset;
            // The label sits by the box's top-right corner.
            let at = Point::new(landed.x1, landed.y0);
            self.hint = Some(Hint {
                at,
                kind,
                guides: snap.guides,
            });
        }
        snap.offset
    }

    fn targets(&mut self, s: &Session) -> &SnapTargets {
        if let Some(t) = &self.frozen {
            return t;
        }
        let fresh = self
            .live
            .as_ref()
            .is_none_or(|(rev, _)| *rev != s.revision());
        if fresh {
            self.live = Some((s.revision(), SnapTargets::collect(&s.doc, &s.fonts, &[])));
        }
        &self.live.as_ref().expect("filled just above").1
    }
}

/// The anchors of `shape`'s path other than `moving`, in document space:
/// the targets a node drag keeps from its own path.
#[must_use]
pub fn other_nodes(s: &Session, shape: NodeId, moving: &[PathNodeRef]) -> Vec<Point> {
    let Some(path) = s.path_of(shape) else {
        return Vec::new();
    };
    let w = world(s, shape);
    let mut out = Vec::new();
    for (subpath, sp) in path.subpaths.iter().enumerate() {
        for (index, n) in sp.nodes.iter().enumerate() {
            let r = PathNodeRef {
                shape,
                subpath,
                index,
            };
            if !moving.contains(&r) {
                out.push(w * vextor_geom::to_kurbo(n.anchor));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use vextor_core::{Command, Document, Geometry, Node, NodeKind, Shape, Style};

    fn session_with_rect() -> (Session, NodeId) {
        let mut doc = Document::new("t");
        let layer = doc.layers[0];
        let id = doc.alloc_node_id();
        let node = Node::new(
            id,
            NodeKind::Shape(Shape {
                geometry: Geometry::Rect {
                    rect: vextor_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                    radius: vextor_core::CornerRadii::default(),
                },
                style: Style::default(),
            }),
        );
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![node],
        })
        .expect("inserts");
        (Session::new(doc), id)
    }

    #[test]
    fn a_point_snaps_and_leaves_a_hint() {
        let (s, _) = session_with_rect();
        let mut snap = Snapper::default();
        let p = snap.point(&s, Point::new(10.5, 10.5), false, false);
        assert_eq!(p, Point::new(10.0, 10.0));
        assert_eq!(snap.hint.as_ref().map(|h| h.kind), Some(SnapKind::Node));
    }

    #[test]
    fn command_suspends_snapping() {
        let (s, _) = session_with_rect();
        let mut snap = Snapper::default();
        let p = Point::new(10.5, 10.5);
        assert_eq!(snap.point(&s, p, true, true), p);
        assert!(snap.hint.is_none());
    }

    #[test]
    fn the_tolerance_follows_the_zoom() {
        let (mut s, _) = session_with_rect();
        let mut snap = Snapper::default();
        let p = Point::new(13.0, 13.0);
        s.editor.view.zoom = 0.25;
        assert_eq!(snap.point(&s, p, false, false), Point::new(10.0, 10.0));
        s.editor.view.zoom = 8.0;
        assert_eq!(snap.point(&s, p, false, false), p);
    }

    #[test]
    fn pixel_snap_rounds_when_on() {
        let (mut s, _) = session_with_rect();
        s.doc.grid.snap.pixel = true;
        let mut snap = Snapper::default();
        let p = snap.point(&s, Point::new(40.4, 50.6), false, true);
        assert_eq!(p, Point::new(40.0, 51.0));
        // Only for the tools that ask for it.
        let q = snap.point(&s, Point::new(40.4, 50.6), false, false);
        assert_eq!(q, Point::new(40.4, 50.6));
    }

    #[test]
    fn a_drag_does_not_snap_to_what_it_moves() {
        let (s, id) = session_with_rect();
        let mut snap = Snapper::default();
        snap.begin(&s, &[id], Vec::new());
        let p = Point::new(10.5, 10.5);
        assert_eq!(snap.point(&s, p, false, false), p);
        snap.end();
        assert!(!snap.is_dragging());
    }

    #[test]
    fn a_grabbed_point_snaps_not_the_pointer() {
        let (s, _) = session_with_rect();
        let mut snap = Snapper::default();
        // Grabbed 2 px left of the pointer: the grabbed point (10.3, 9.8)
        // lands on the corner (10, 10), so the pointer moves by the same.
        let p = snap.grabbed(
            &s,
            Point::new(50.0, 50.0),
            Point::new(48.0, 50.0),
            Point::new(12.3, 9.8),
            false,
        );
        assert!((p - Point::new(12.0, 10.0)).hypot() < 1e-9);
    }
}
