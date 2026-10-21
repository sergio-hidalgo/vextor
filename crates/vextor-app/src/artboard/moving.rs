//! Moving an artboard (spec 08b): dragging its label on the canvas, or
//! Inspector ▸ Artboard X and Y. The objects lying wholly on it move with
//! it, unless it moves alone (`⌥`-drag). Either way it is one undo step,
//! "Move artboard".

use kurbo::{Affine, Point, Rect as KRect, Vec2};
use vextor_core::{ArtboardId, Command, Document, NodeId};
use vextor_geom::{Fonts, retarget};

use crate::session::Session;

/// The undo label of a move.
pub const MOVE_LABEL: &str = "Move artboard";
/// Movement before a label press becomes a drag, in screen points (the
/// tools' threshold): a click only picks the artboard.
const DRAG_THRESHOLD: f64 = 3.0;

/// Artboard `id`'s bounds as a kurbo rect.
fn bounds(doc: &Document, id: ArtboardId) -> Option<KRect> {
    let r = doc.artboards.iter().find(|a| a.id == id)?.rect;
    Some(KRect::new(r.x, r.y, r.x + r.width, r.y + r.height))
}

/// The top-level objects (children of a layer) lying wholly inside
/// artboard `id`: the ones that move with it. Hidden and locked objects
/// count; they lie on it too.
#[must_use]
pub fn objects_on(doc: &Document, fonts: &Fonts, id: ArtboardId) -> Vec<NodeId> {
    let Some(board) = bounds(doc, id) else {
        return Vec::new();
    };
    let inside = board.inflate(vextor_core::EPS, vextor_core::EPS);
    doc.layers
        .iter()
        .filter_map(|l| doc.nodes.get(l)?.container())
        .flat_map(|c| c.children.iter().copied())
        .filter(|&o| vextor_geom::bounds(doc, fonts, o).is_some_and(|b| inside.contains_rect(b)))
        .collect()
}

/// The command that moves artboard `id` of `doc` by `by`, with `objects`;
/// `None` when the artboard is missing or the move is not finite.
fn move_command(doc: &Document, id: ArtboardId, objects: &[NodeId], by: Vec2) -> Option<Command> {
    if !(by.x.is_finite() && by.y.is_finite()) {
        return None;
    }
    let mut artboards = doc.artboards.clone();
    let a = artboards.iter_mut().find(|a| a.id == id)?;
    a.rect.x += by.x;
    a.rect.y += by.y;
    let mut commands = vec![Command::SetArtboards(artboards)];
    if !objects.is_empty() {
        commands.push(Command::SetTransforms {
            items: retarget(doc, objects, Affine::translate(by)),
        });
    }
    Some(Command::Batch {
        label: MOVE_LABEL,
        commands,
    })
}

/// Moves artboard `id` so its top-left corner is at (`x`, `y`), with the
/// objects on it, as one undo step; inside a gesture, consecutive moves
/// merge into one. Returns false when nothing changed.
pub fn move_to(s: &mut Session, id: ArtboardId, x: f64, y: f64) -> bool {
    let Some(b) = bounds(&s.doc, id) else {
        return false;
    };
    let by = Vec2::new(x - b.x0, y - b.y0);
    if by.hypot() <= vextor_core::EPS {
        return false;
    }
    let objects = objects_on(&s.doc, &s.fonts, id);
    move_command(&s.doc, id, &objects, by).is_some_and(|cmd| s.apply(cmd).is_ok())
}

/// A label drag in progress: every frame moves the artboard (and its
/// objects) from where they were at press, so the drag never drifts. The
/// whole drag is one history gesture.
#[derive(Debug)]
pub struct ArtboardDrag {
    id: ArtboardId,
    start: Point,
    /// The document at press.
    before: Box<Document>,
    /// The artboard's bounds at press.
    rect: KRect,
    /// What moves with it: the objects on it at press, or none (`⌥`).
    objects: Vec<NodeId>,
    /// Past the drag threshold.
    moved: bool,
}

impl ArtboardDrag {
    /// A press at `p` on artboard `id`'s label; `alone` (`⌥`) leaves the
    /// objects on it where they are. `None` when there is no such artboard.
    pub fn press(s: &mut Session, id: ArtboardId, p: Point, alone: bool) -> Option<Self> {
        let rect = bounds(&s.doc, id)?;
        let objects = if alone {
            Vec::new()
        } else {
            objects_on(&s.doc, &s.fonts, id)
        };
        s.begin_gesture();
        Some(Self {
            id,
            start: p,
            before: Box::new(s.doc.clone()),
            rect,
            objects,
            moved: false,
        })
    }

    /// The artboard's bounds at press.
    #[must_use]
    pub fn rect(&self) -> KRect {
        self.rect
    }

    /// The artboard's bounds with the pointer at `p`, before snapping.
    #[must_use]
    pub fn moved_rect(&self, p: Point) -> KRect {
        self.rect + (p - self.start)
    }

    /// What moves with the artboard.
    #[must_use]
    pub fn objects(&self) -> &[NodeId] {
        &self.objects
    }

    /// The pointer at `p` (document px) is past the drag threshold at
    /// `zoom`, or was once.
    #[must_use]
    pub fn started(&self, p: Point, zoom: f64) -> bool {
        self.moved || (p - self.start).hypot() * zoom >= DRAG_THRESHOLD
    }

    /// The pointer moved to `p`; `snap` is the extra offset that snaps the
    /// moved artboard. Below the drag threshold nothing moves.
    ///
    /// # Errors
    /// A message for the user when the move is refused.
    pub fn drag_to(
        &mut self,
        s: &mut Session,
        p: Point,
        snap: Vec2,
        zoom: f64,
    ) -> Result<(), String> {
        if !self.started(p, zoom) {
            return Ok(());
        }
        self.moved = true;
        let by = p - self.start + snap;
        let Some(cmd) = move_command(&self.before, self.id, &self.objects, by) else {
            return Ok(());
        };
        s.apply(cmd)
            .map_err(|e| format!("{MOVE_LABEL} failed: {e}"))
    }

    /// The button was released: the gesture ends. Returns true when the
    /// artboard moved (false: it was a click).
    pub fn release(self, s: &mut Session) -> bool {
        s.end_gesture();
        self.moved
    }
}

#[cfg(test)]
mod tests {
    use vextor_core::{CornerRadii, Geometry, Node, NodeKind, Shape, Style};

    use super::*;
    use crate::new_doc::new_document;

    /// A 200×200 artboard at the origin with a rect wholly on it (10, 10,
    /// 20×20) and one half off it (190, 50, 20×20).
    fn session() -> (Session, ArtboardId, NodeId, NodeId) {
        let mut doc = new_document("a", 200.0, 200.0);
        let layer = doc.layers[0];
        let mut rect = |x: f64, y: f64| {
            Node::new(
                doc.alloc_node_id(),
                NodeKind::Shape(Shape {
                    geometry: Geometry::Rect {
                        rect: vextor_core::Rect::new(x, y, 20.0, 20.0),
                        radius: CornerRadii::default(),
                    },
                    style: Style::default(),
                }),
            )
        };
        let (on, off) = (rect(10.0, 10.0), rect(190.0, 50.0));
        let (on_id, off_id) = (on.id, off.id);
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![on, off],
        })
        .expect("inserts");
        let id = doc.artboards[0].id;
        (Session::new(doc), id, on_id, off_id)
    }

    fn left_top(s: &Session, id: NodeId) -> (f64, f64) {
        let b = vextor_geom::bounds(&s.doc, &s.fonts, id).expect("has bounds");
        (b.x0, b.y0)
    }

    fn board(s: &Session) -> (f64, f64) {
        let r = s.doc.artboards[0].rect;
        (r.x, r.y)
    }

    fn drag(s: &mut Session, id: ArtboardId, to: Point, alone: bool) -> bool {
        let start = Point::new(5.0, -10.0);
        let mut d = ArtboardDrag::press(s, id, start, alone).expect("an artboard");
        for t in [0.25, 0.5, 1.0] {
            let p = start + (to - start) * t;
            d.drag_to(s, p, Vec2::ZERO, 1.0).expect("moves");
        }
        d.release(s)
    }

    #[test]
    fn only_objects_wholly_on_the_artboard_belong_to_it() {
        let (s, id, on, _) = session();
        assert_eq!(objects_on(&s.doc, &s.fonts, id), vec![on]);
    }

    #[test]
    fn moving_an_artboard_moves_its_objects_as_one_undo_step() {
        let (mut s, id, on, off) = session();
        let start = s.doc.clone();
        assert!(drag(&mut s, id, Point::new(105.0, 40.0), false));
        assert_eq!(board(&s), (100.0, 50.0));
        assert_eq!(left_top(&s, on), (110.0, 60.0));
        assert_eq!(left_top(&s, off), (190.0, 50.0), "half off: stays");
        assert_eq!(s.edit_menu().undo, Some(MOVE_LABEL));
        assert!(s.undo());
        assert_eq!(s.doc, start, "one undo restores both");
        assert!(!s.undo(), "the drag was a single step");
    }

    #[test]
    fn option_drag_moves_the_artboard_alone() {
        let (mut s, id, on, _) = session();
        assert!(drag(&mut s, id, Point::new(105.0, 40.0), true));
        assert_eq!(board(&s), (100.0, 50.0));
        assert_eq!(left_top(&s, on), (10.0, 10.0));
    }

    #[test]
    fn a_tiny_drag_is_a_click_and_changes_nothing() {
        let (mut s, id, _, _) = session();
        let start = s.doc.clone();
        let p = Point::new(5.0, -10.0);
        let mut d = ArtboardDrag::press(&mut s, id, p, false).expect("an artboard");
        d.drag_to(&mut s, p + Vec2::new(1.0, 1.0), Vec2::ZERO, 1.0)
            .expect("no-op");
        assert!(!d.release(&mut s));
        assert_eq!(s.doc, start);
        assert!(!s.undo(), "nothing to undo");
    }

    #[test]
    fn the_snap_offset_is_added_to_the_move() {
        let (mut s, id, _, _) = session();
        let p = Point::new(0.0, 0.0);
        let mut d = ArtboardDrag::press(&mut s, id, p, false).expect("an artboard");
        d.drag_to(&mut s, Point::new(48.0, 0.0), Vec2::new(2.0, 0.0), 1.0)
            .expect("moves");
        d.release(&mut s);
        assert_eq!(board(&s), (50.0, 0.0));
    }

    #[test]
    fn move_to_places_the_corner_and_takes_the_objects() {
        let (mut s, id, on, off) = session();
        let start = s.doc.clone();
        assert!(move_to(&mut s, id, -40.0, 30.0));
        assert_eq!(board(&s), (-40.0, 30.0));
        assert_eq!(left_top(&s, on), (-30.0, 40.0));
        assert_eq!(left_top(&s, off), (190.0, 50.0));
        assert!(!move_to(&mut s, id, -40.0, 30.0), "already there");
        assert!(!move_to(&mut s, id, f64::NAN, 0.0), "refused");
        assert!(s.undo());
        assert_eq!(s.doc, start);
    }
}
