//! The pen tool (`P`, spec 07) as a state machine over a [`Session`] fed
//! with document-space points, so it is tested without a window.
//!
//! A click adds a corner node; a click-drag adds a curved node with
//! symmetric handles (`⌥` while dragging moves only the outgoing handle,
//! making a corner). `⇧` keeps the new segment, and the dragged handle,
//! at the document's angle step. Clicking the first node closes the path;
//! `Enter` or `Esc` leaves it open; `⌫` removes the last node. Starting on
//! an open end of a path continues that path, and clicking another path's
//! open end joins the two. With no path under way, a click on a segment
//! of the selected path inserts a node there.
//!
//! The whole path is one "Draw path" undo step: each change takes the
//! tool's previous step back and records the path again, as the shape
//! tools do. If anything else changed the document in between, the path
//! under way simply ends.

use kurbo::{Affine, Point, Vec2};
use vextor_core::{
    Command, Geometry, Node, NodeId, NodeKind, NodeType, Path, PathNode, PathNodeRef, Shape, Style,
    SubPath, Transform,
};
use vextor_geom::{
    SegmentHit, constrain_angle, constrain_vector, from_point, handle_or_none, hit_segment,
    join_open, open_endpoints, selectable, to_kurbo, transform_subpath,
};

use crate::session::Session;
use crate::{edit_ops, path_ops};

/// The style a new pen path gets: the session's default style, with its
/// stroke (1 px black when there is none) so open segments show.
#[must_use]
pub fn new_style(s: &Session) -> Style {
    let style = s.editor.default_style.clone();
    Style {
        stroke: Some(style.stroke.clone().unwrap_or_default()),
        ..style
    }
}

/// Undo label of drawing a path.
pub const LABEL: &str = "Draw path";
/// How near a node or end the pointer must be to snap to it, in screen
/// points.
pub const HIT: f64 = 6.0;
/// Movement before a press becomes a drag, in screen points.
const DRAG_THRESHOLD: f64 = 3.0;

/// What a click would do where the pointer is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hover {
    /// Over the first node of the path being drawn: close it.
    Close,
    /// Over the last node of the path being drawn: a drag reshapes its
    /// outgoing handle; a click adds nothing.
    Last,
    /// Over an open end of a path: continue it, or join it to the path
    /// being drawn.
    End(PathNodeRef),
    /// Over a segment of the selected path: insert a node there.
    Insert(NodeId, SegmentHit),
    /// Anywhere else: add a node.
    Free,
}

/// The path being drawn.
#[derive(Debug)]
struct Draft {
    shape: NodeId,
    subpath: usize,
    /// The whole path, in the shape's local space.
    path: Path,
    /// Document space → the shape's local space.
    to_local: Affine,
    /// Nodes the subpath had before drawing (continuing a path): `⌫`
    /// never removes those.
    kept: usize,
    /// The path as it was before drawing on it (continuing a path).
    original: Option<Path>,
    /// Where a new shape goes (parent, index); `None` when continuing.
    insert_at: Option<(NodeId, usize)>,
    style: Style,
    /// The session revision after the tool's last change.
    revision: u64,
    /// The top undo step is this path's own.
    recorded: bool,
}

impl Draft {
    fn nodes(&self) -> &[PathNode] {
        &self.path.subpaths[self.subpath].nodes
    }

    fn nodes_mut(&mut self) -> &mut Vec<PathNode> {
        &mut self.path.subpaths[self.subpath].nodes
    }

    fn to_doc(&self) -> Affine {
        self.to_local.inverse()
    }

    fn local(&self, p: Point) -> vextor_core::Point {
        from_point(self.to_local * p)
    }

    fn local_vec(&self, v: Vec2) -> Vec2 {
        self.to_local * (Point::ZERO + v) - self.to_local * Point::ZERO
    }

    fn last_doc(&self) -> Option<Point> {
        self.nodes()
            .last()
            .map(|n| self.to_doc() * to_kurbo(n.anchor))
    }
}

/// What the press under way will do on release.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pending {
    /// A node was added (or an end picked to continue): a drag shapes it.
    Node { out_only: bool },
    /// Close on release; a drag shapes the first node.
    Close,
    /// Join this end on release; a drag shapes the joining node.
    Join(PathNodeRef),
}

#[derive(Debug)]
struct Press {
    start: Point,
    pending: Pending,
    moved: bool,
}

/// What the canvas draws for the path being drawn (document space).
#[derive(Clone, Debug, PartialEq)]
pub struct PenOverlay {
    /// The anchors of the subpath being drawn.
    pub nodes: Vec<Point>,
    /// The last node's outgoing handle tip: the rubber band leaves along it.
    pub last_out: Option<Point>,
    /// While a node is dragged out: its incoming tip, anchor, outgoing tip.
    pub handles: Option<(Option<Point>, Point, Option<Point>)>,
}

/// The pen's state between events.
#[derive(Debug, Default)]
pub struct PenTool {
    draft: Option<Draft>,
    press: Option<Press>,
}

impl PenTool {
    /// True while a path is being drawn.
    #[must_use]
    pub fn is_drawing(&self) -> bool {
        self.draft.is_some()
    }

    /// True while the button is held.
    #[must_use]
    pub fn is_pressed(&self) -> bool {
        self.press.is_some()
    }

    /// What a click at `p` would do, at `zoom`.
    #[must_use]
    pub fn hover(&self, s: &Session, p: Point, zoom: f64) -> Hover {
        let reach = HIT / zoom;
        if let Some(d) = &self.draft {
            let first = d.nodes().first().map(|n| d.to_doc() * to_kurbo(n.anchor));
            if d.nodes().len() >= 2 && first.is_some_and(|f| (f - p).hypot() <= reach) {
                return Hover::Close;
            }
            if d.last_doc().is_some_and(|l| (l - p).hypot() <= reach) {
                return Hover::Last;
            }
        }
        if let Some(r) = self.end_at(s, p, reach) {
            return Hover::End(r);
        }
        if self.draft.is_none()
            && let Some(shape) = path_ops::edited(s)
            && let Some(path) = s.path_of(shape)
            && let Some(hit) = hit_segment(path, path_ops::world(s, shape), p, reach)
        {
            return Hover::Insert(shape, hit);
        }
        Hover::Free
    }

    /// The nearest open end of a selectable path within `reach` of `p`,
    /// other than the ends of the subpath being drawn.
    fn end_at(&self, s: &Session, p: Point, reach: f64) -> Option<PathNodeRef> {
        let own = self.draft.as_ref().map(|d| (d.shape, d.subpath));
        let mut best: Option<(PathNodeRef, f64)> = None;
        for shape in selectable(&s.doc) {
            let Some(path) = s.path_of(shape) else {
                continue;
            };
            let world = path_ops::world(s, shape);
            for (subpath, index) in open_endpoints(path) {
                if own == Some((shape, subpath)) {
                    continue;
                }
                let at = world * to_kurbo(path.subpaths[subpath].nodes[index].anchor);
                let d = (at - p).hypot();
                if d <= reach && best.is_none_or(|(_, b)| d < b) {
                    let r = PathNodeRef {
                        shape,
                        subpath,
                        index,
                    };
                    best = Some((r, d));
                }
            }
        }
        best.map(|(r, _)| r)
    }

    /// Where a click at `p` puts the next node: `p`, or with `shift` on
    /// the angle step from the last node.
    #[must_use]
    pub fn next_point(&self, s: &Session, p: Point, shift: bool) -> Point {
        match self.draft.as_ref().and_then(Draft::last_doc) {
            Some(last) if shift => constrain_angle(last, p, s.doc.grid.snap.angle_step_deg),
            _ => p,
        }
    }

    /// The path being drawn, for the canvas.
    #[must_use]
    pub fn overlay(&self) -> Option<PenOverlay> {
        let d = self.draft.as_ref()?;
        let doc = d.to_doc();
        let tip = |n: &PathNode, h: Option<vextor_core::Vec2>| {
            h.map(|h| doc * (to_kurbo(n.anchor) + Vec2::new(h.x, h.y)))
        };
        let last = d.nodes().last()?;
        let shaped = match self.press.as_ref() {
            Some(p) if p.moved => match p.pending {
                Pending::Close => d.nodes().first(),
                _ => Some(last),
            },
            _ => None,
        };
        Some(PenOverlay {
            nodes: d.nodes().iter().map(|n| doc * to_kurbo(n.anchor)).collect(),
            last_out: tip(last, last.handle_out),
            handles: shaped.map(|n| {
                (
                    tip(n, n.handle_in),
                    doc * to_kurbo(n.anchor),
                    tip(n, n.handle_out),
                )
            }),
        })
    }

    /// The pointer pressed at `p` with `shift` / `alt` as in `mods`.
    ///
    /// # Errors
    /// A message for the user when the path is refused or there is no
    /// unlocked layer to draw in.
    pub fn press(
        &mut self,
        s: &mut Session,
        p: Point,
        mods: crate::select::Mods,
        zoom: f64,
    ) -> Result<(), String> {
        self.press = None;
        self.sync(s);
        let hover = self.hover(s, p, zoom);
        let pending = match (self.draft.is_some(), hover) {
            (false, Hover::Insert(shape, hit)) => {
                return path_ops::insert_node(s, shape, hit.subpath, hit.index, hit.t);
            }
            (false, Hover::End(r)) => {
                self.continue_from(s, r);
                Pending::Node { out_only: true }
            }
            (false, _) => {
                self.start(s, p)?;
                Pending::Node { out_only: false }
            }
            (true, Hover::Close) => Pending::Close,
            (true, Hover::Last) => Pending::Node { out_only: true },
            (true, Hover::End(r)) => {
                let at = path_ops::anchor_in_doc(s, r).unwrap_or(p);
                self.add_node(s, at)?;
                Pending::Join(r)
            }
            (true, _) => {
                let at = self.next_point(s, p, mods.shift);
                self.add_node(s, at)?;
                Pending::Node { out_only: false }
            }
        };
        self.press = Some(Press {
            start: p,
            pending,
            moved: false,
        });
        Ok(())
    }

    /// The pointer moved to `p` with the button held: drags out handles.
    ///
    /// # Errors
    /// A message for the user when the path is refused.
    pub fn drag_to(
        &mut self,
        s: &mut Session,
        p: Point,
        mods: crate::select::Mods,
        zoom: f64,
    ) -> Result<(), String> {
        let (Some(press), Some(d)) = (&mut self.press, &mut self.draft) else {
            return Ok(());
        };
        let mut h = p - press.start;
        if !press.moved && h.hypot() * zoom < DRAG_THRESHOLD {
            return Ok(());
        }
        press.moved = true;
        if mods.shift {
            h = constrain_vector(h, s.doc.grid.snap.angle_step_deg);
        }
        let h = d.local_vec(h);
        let (index, out_only) = match press.pending {
            Pending::Close => (0, false),
            Pending::Node { out_only } => (d.nodes().len() - 1, out_only),
            Pending::Join(_) => (d.nodes().len() - 1, false),
        };
        let closing = press.pending == Pending::Close;
        let node = &mut d.nodes_mut()[index];
        // Closing drags the first node's handles the way the path runs on.
        let (forward, back) = (handle_or_none(h), handle_or_none(-h));
        if mods.alt || out_only {
            node.kind = NodeType::Corner;
            if closing {
                node.handle_in = back;
            } else {
                node.handle_out = forward;
            }
        } else {
            node.kind = NodeType::Symmetric;
            node.handle_out = forward;
            node.handle_in = back;
        }
        self.record(s, Vec::new())
    }

    /// The button was released: closes or joins when the press said so.
    ///
    /// # Errors
    /// A message for the user when the path is refused.
    pub fn release(&mut self, s: &mut Session) -> Result<(), String> {
        let Some(press) = self.press.take() else {
            return Ok(());
        };
        match press.pending {
            Pending::Node { .. } => Ok(()),
            Pending::Close => {
                if let Some(d) = &mut self.draft {
                    d.path.subpaths[d.subpath].closed = true;
                }
                let done = self.record(s, Vec::new());
                self.finish(s);
                done
            }
            Pending::Join(r) => {
                let done = self.join(s, r);
                self.finish(s);
                done
            }
        }
    }

    /// `⌫`: removes the last node drawn. Removing the only node of a new
    /// path takes the path back. Returns false when not drawing.
    ///
    /// # Errors
    /// A message for the user when the path is refused.
    pub fn backspace(&mut self, s: &mut Session) -> Result<bool, String> {
        self.sync(s);
        let Some(d) = &mut self.draft else {
            return Ok(false);
        };
        self.press = None;
        if d.nodes().len() <= d.kept {
            self.finish(s);
            return Ok(true);
        }
        d.nodes_mut().pop();
        if d.nodes().is_empty() {
            self.take_back(s);
            return Ok(true);
        }
        self.record(s, Vec::new())?;
        Ok(true)
    }

    /// Ends the path, left open (`Enter`, `Esc`, another tool…). A new
    /// path of a single node is taken back. The path stays selected.
    pub fn finish(&mut self, s: &mut Session) {
        self.press = None;
        self.sync(s);
        let Some(d) = &self.draft else {
            return;
        };
        if d.insert_at.is_some() && d.path.subpaths[d.subpath].nodes.len() < 2 {
            self.take_back(s);
            return;
        }
        let shape = d.shape;
        self.draft = None;
        if s.doc.nodes.contains_key(&shape) {
            s.select(vec![shape]);
        }
    }

    /// Drops the draft, taking back its step when it is still the top one.
    fn take_back(&mut self, s: &mut Session) {
        if let Some(d) = self.draft.take()
            && d.recorded
            && s.revision() == d.revision
        {
            s.undo();
        }
    }

    /// Ends the draft when something else changed the document since the
    /// tool's last change: its step is no longer the top one.
    pub fn sync(&mut self, s: &Session) {
        if self
            .draft
            .as_ref()
            .is_some_and(|d| s.revision() != d.revision)
        {
            self.draft = None;
            self.press = None;
        }
    }

    /// Starts a new path with one node at `p`.
    fn start(&mut self, s: &mut Session, p: Point) -> Result<(), String> {
        let insert_at = edit_ops::paste_target(s)
            .ok_or_else(|| "Draw failed: every layer is hidden or locked".to_owned())?;
        let node = PathNode::corner(p.x, p.y);
        self.draft = Some(Draft {
            shape: s.doc.alloc_node_id(),
            subpath: 0,
            path: Path {
                subpaths: vec![SubPath {
                    nodes: vec![node],
                    closed: false,
                }],
                fill_rule: vextor_core::FillRule::NonZero,
            },
            // The new shape's transform undoes its parent's: local space is
            // document space.
            to_local: Affine::IDENTITY,
            kept: 0,
            original: None,
            insert_at: Some(insert_at),
            style: new_style(s),
            revision: s.revision(),
            recorded: false,
        });
        self.record(s, Vec::new())
    }

    /// Continues the open subpath ending at `r`, drawing on from that end.
    fn continue_from(&mut self, s: &mut Session, r: PathNodeRef) {
        let Some(mut path) = s.path_of(r.shape).cloned() else {
            return;
        };
        let sp = &mut path.subpaths[r.subpath];
        if r.index == 0 && sp.nodes.len() > 1 {
            sp.reverse(); // draw on from the end that was clicked
        }
        let kept = sp.nodes.len();
        let original = s.path_of(r.shape).cloned();
        let style = match &s.doc.nodes[&r.shape].kind {
            NodeKind::Shape(shape) => shape.style.clone(),
            _ => Style::default(),
        };
        self.draft = Some(Draft {
            shape: r.shape,
            subpath: r.subpath,
            path,
            to_local: path_ops::world(s, r.shape).inverse(),
            kept,
            original,
            insert_at: None,
            style,
            revision: s.revision(),
            recorded: false,
        });
        s.select(vec![r.shape]);
    }

    /// Adds a corner node at `p` (document space).
    fn add_node(&mut self, s: &mut Session, p: Point) -> Result<(), String> {
        if let Some(d) = &mut self.draft {
            let at = d.local(p);
            d.nodes_mut().push(PathNode::corner(at.x, at.y));
        }
        self.record(s, Vec::new())
    }

    /// Joins the subpath ending at `r` onto the one drawn, whose last node
    /// was placed on `r`. Another path's subpath moves into this path.
    fn join(&mut self, s: &mut Session, r: PathNodeRef) -> Result<(), String> {
        let Some(d) = &mut self.draft else {
            return Ok(());
        };
        let mut extra = Vec::new();
        let mut other = if r.shape == d.shape {
            d.path.subpaths[r.subpath].clone()
        } else {
            let Some(path) = s.path_of(r.shape) else {
                return Ok(());
            };
            let into = d.to_local * path_ops::world(s, r.shape);
            if path.subpaths.len() == 1 {
                extra.push(Command::Delete { ids: vec![r.shape] });
            } else {
                let mut rest = path.clone();
                rest.subpaths.remove(r.subpath);
                extra.push(Command::SetGeometry {
                    items: vec![(r.shape, Geometry::Path(rest))],
                });
            }
            transform_subpath(&path.subpaths[r.subpath], into)
        };
        if r.index != 0 {
            other.reverse(); // the joined end comes first
        }
        let mut ours = d.path.subpaths[d.subpath].clone();
        if let Some(tail) = ours.nodes.last_mut() {
            tail.kind = NodeType::Corner; // keep the handle the drag gave it
        }
        d.path.subpaths[d.subpath] = join_open(ours, other);
        if r.shape == d.shape {
            d.path.subpaths.remove(r.subpath);
            if r.subpath < d.subpath {
                d.subpath -= 1;
            }
        }
        self.record(s, extra)
    }

    /// Records the draft as the path's one undo step (with `extra`
    /// commands), replacing the tool's previous step.
    fn record(&mut self, s: &mut Session, extra: Vec<Command>) -> Result<(), String> {
        let Some(d) = &mut self.draft else {
            return Ok(());
        };
        if d.recorded && s.revision() == d.revision {
            s.undo();
        }
        if extra.is_empty() && d.original.as_ref() == Some(&d.path) {
            // Back to the path as it was: no step at all.
            d.recorded = false;
            d.revision = s.revision();
            return Ok(());
        }
        let geometry = Geometry::Path(d.path.clone());
        let main = match d.insert_at {
            Some((parent, index)) if !s.doc.nodes.contains_key(&d.shape) => {
                let mut node = Node::new(
                    d.shape,
                    NodeKind::Shape(Shape {
                        geometry,
                        style: d.style.clone(),
                    }),
                );
                node.transform = s
                    .doc
                    .world_transform(parent)
                    .invert()
                    .unwrap_or(Transform::IDENTITY);
                Command::Insert {
                    parent: Some(parent),
                    index,
                    nodes: vec![node],
                }
            }
            _ => Command::SetGeometry {
                items: vec![(d.shape, geometry)],
            },
        };
        let mut commands = vec![main];
        commands.extend(extra);
        let result = s.apply(Command::Batch {
            label: LABEL,
            commands,
        });
        if let Err(e) = result {
            self.draft = None;
            self.press = None;
            return Err(format!("Draw failed: {e}"));
        }
        d.recorded = true;
        d.revision = s.revision();
        s.select(vec![d.shape]);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
