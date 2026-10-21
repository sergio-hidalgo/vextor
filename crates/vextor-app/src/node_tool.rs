//! The node tool (`A`, or double-click a path, spec 07) as a state machine
//! fed with document-space points, so it is tested without a window.
//!
//! It edits one path: the selected one ([`path_ops::edited`]). A click
//! selects a node (`⇧` adds or removes), a drag on empty canvas selects
//! the nodes in its marquee, a drag moves the selected nodes (`⇧` keeps
//! the move on the angle step). Dragging a handle moves it, its partner
//! following the node type (`⌥` moves it alone, making a corner).
//! Clicking a segment selects it; dragging one bends it through the
//! pointer. The round widget inside a selected corner node rounds it
//! (every selected corner at once). A double-click on a node cycles its
//! type. Each drag is one undo step.

use kurbo::{Affine, Point, Rect, Vec2};
use vextor_core::{Document, NodeId, PathNodeRef, Selection, Side, SubPath};
use vextor_geom::{NodeCorner, SegmentHit, hit_node, hit_segment, node_corner, pick};

use crate::path_ops::{self, edited, select_nodes, selected_nodes, world};
use crate::select::{Mods, PICK_TOLERANCE};
use crate::session::Session;

/// How near a node, handle or widget the pointer must be to grab it, in
/// screen points (an 8 pt area at least, the `ui-editor` minimum).
pub const HIT: f64 = 5.0;
/// How far inside a corner its round widget sits beyond the rounding
/// arc, in screen points (clear of the node itself).
pub const WIDGET_INSET: f64 = 12.0;
/// Movement before a press becomes a drag, in screen points.
const DRAG_THRESHOLD: f64 = 3.0;

/// What the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NodeTarget {
    /// A shown handle of the edited path.
    Handle(PathNodeRef, Side),
    /// The round widget of a selected corner node.
    Radius(PathNodeRef),
    Node(PathNodeRef),
    Segment(NodeId, SegmentHit),
    /// Another object: a click selects it (a path's nodes then show).
    Object(NodeId),
    Empty,
}

#[derive(Debug, Default)]
enum Drag {
    #[default]
    Idle,
    Marquee {
        start: Point,
        now: Point,
        /// Nodes already selected that the marquee adds to (`⇧`).
        base: Vec<PathNodeRef>,
        shape: Option<NodeId>,
        moved: bool,
    },
    Nodes {
        start: Point,
        /// The pressed node's anchor at press (what snaps, spec 08).
        anchor: Point,
        /// The document-space move applied so far.
        applied: Vec2,
        moved: bool,
        /// The selected node pressed on: a click without a drag selects
        /// it alone.
        clicked: Option<PathNodeRef>,
    },
    Handle {
        node: PathNodeRef,
        side: Side,
        start: Point,
        /// The handle's tip at press (what snaps, spec 08).
        anchor: Point,
        moved: bool,
    },
    Bend {
        segment: (NodeId, usize, usize),
        t: f64,
        start: Point,
        /// The subpath when the press began.
        before: SubPath,
        moved: bool,
    },
    Radius {
        nodes: Vec<PathNodeRef>,
        corner: NodeCorner,
        /// Document space → the path's local space.
        to_local: Affine,
        /// [`WIDGET_INSET`] in local px.
        inset: f64,
    },
}

/// The node tool's state between events.
#[derive(Debug, Default)]
pub struct NodeTool {
    drag: Drag,
    /// The document when a drag that edits the path began (fast drag).
    before: Option<Box<Document>>,
}

impl NodeTool {
    /// True while a drag is in progress.
    #[must_use]
    pub fn is_dragging(&self) -> bool {
        !matches!(self.drag, Drag::Idle)
    }

    /// While the edited path is dragged: the press-time document and the
    /// path, for the canvas's fast path.
    #[must_use]
    pub fn preview(&self, s: &Session) -> Option<(&Document, NodeId)> {
        Some((self.before.as_deref()?, edited(s)?))
    }

    /// While nodes or a handle are dragged: where the press was and the
    /// grabbed point then, and the edited path (what snaps, spec 08).
    #[must_use]
    pub fn snap_drag(&self, s: &Session) -> Option<(Point, Point, NodeId)> {
        let shape = edited(s)?;
        match self.drag {
            Drag::Nodes { start, anchor, .. } | Drag::Handle { start, anchor, .. } => {
                Some((start, anchor, shape))
            }
            _ => None,
        }
    }

    /// The marquee rectangle (document space) while one is drawn.
    #[must_use]
    pub fn marquee(&self) -> Option<Rect> {
        match self.drag {
            Drag::Marquee {
                start,
                now,
                moved: true,
                ..
            } => Some(Rect::from_points(start, now)),
            _ => None,
        }
    }

    /// What is under `p` at `zoom`.
    #[must_use]
    pub fn target(s: &Session, p: Point, zoom: f64) -> NodeTarget {
        let reach = HIT / zoom;
        if let Some(shape) = edited(s)
            && let Some(path) = s.path_of(shape)
        {
            let w = world(s, shape);
            let near = |q: Point| (q - p).hypot();
            // Where handles overlap, a selected node's own handle wins
            // over a neighbour's; then the nearest.
            let handle = handles(s)
                .into_iter()
                .filter(|h| near(h.tip) <= reach)
                .min_by(|a, b| {
                    b.own
                        .cmp(&a.own)
                        .then_with(|| near(a.tip).total_cmp(&near(b.tip)))
                });
            if let Some(h) = handle {
                return NodeTarget::Handle(h.node, h.side);
            }
            if let Some((r, _)) = radius_widgets(s, zoom)
                .into_iter()
                .find(|&(_, at)| near(at) <= reach)
            {
                return NodeTarget::Radius(r);
            }
            if let Some((subpath, index)) = hit_node(path, w, p, reach) {
                return NodeTarget::Node(PathNodeRef {
                    shape,
                    subpath,
                    index,
                });
            }
            if let Some(hit) = hit_segment(path, w, p, PICK_TOLERANCE / zoom) {
                return NodeTarget::Segment(shape, hit);
            }
        }
        pick(&s.doc, &s.fonts, p, PICK_TOLERANCE / zoom)
            .map_or(NodeTarget::Empty, NodeTarget::Object)
    }

    /// Primary button pressed at `p`.
    pub fn press(&mut self, s: &mut Session, p: Point, mods: Mods, zoom: f64) {
        self.drag = Drag::Idle;
        self.before = None;
        match Self::target(s, p, zoom) {
            NodeTarget::Handle(node, side) => {
                let anchor = handles(s)
                    .into_iter()
                    .find(|h| h.node == node && h.side == side)
                    .map_or(p, |h| h.tip);
                self.begin(s);
                self.drag = Drag::Handle {
                    node,
                    side,
                    start: p,
                    anchor,
                    moved: false,
                };
            }
            NodeTarget::Radius(r) => self.press_radius(s, r, zoom),
            NodeTarget::Node(r) => self.press_node(s, r, p, mods),
            NodeTarget::Segment(shape, hit) => self.press_segment(s, shape, hit, p),
            NodeTarget::Object(id) => s.select(vec![id]),
            NodeTarget::Empty => {
                let shape = edited(s);
                let base = if mods.shift {
                    selected_nodes(s)
                } else {
                    Vec::new()
                };
                if !mods.shift
                    && let Some(shape) = shape
                {
                    select_nodes(s, shape, Vec::new());
                }
                self.drag = Drag::Marquee {
                    start: p,
                    now: p,
                    base,
                    shape,
                    moved: false,
                };
            }
        }
    }

    /// A press on node `r`: `⇧` adds or removes it; otherwise it is
    /// selected (alone, unless already selected) and a move may start.
    fn press_node(&mut self, s: &mut Session, r: PathNodeRef, p: Point, mods: Mods) {
        let mut sel = selected_nodes(s);
        if mods.shift {
            match sel.iter().position(|&x| x == r) {
                Some(i) => {
                    sel.remove(i);
                }
                None => sel.push(r),
            }
            select_nodes(s, r.shape, sel);
            return;
        }
        let clicked = if sel.contains(&r) {
            Some(r)
        } else {
            select_nodes(s, r.shape, vec![r]);
            None
        };
        let anchor = path_ops::anchor_in_doc(s, r).unwrap_or(p);
        self.begin(s);
        self.drag = Drag::Nodes {
            start: p,
            anchor,
            applied: Vec2::ZERO,
            moved: false,
            clicked,
        };
    }

    /// A press on a segment: selects it; a drag may bend it.
    fn press_segment(&mut self, s: &mut Session, shape: NodeId, hit: SegmentHit, p: Point) {
        s.editor.selection = Selection::Segment {
            shape,
            subpath: hit.subpath,
            index: hit.index,
        };
        let Some(before) = s.path_of(shape).map(|p| p.subpaths[hit.subpath].clone()) else {
            return;
        };
        self.begin(s);
        self.drag = Drag::Bend {
            segment: (shape, hit.subpath, hit.index),
            t: hit.t,
            start: p,
            before,
            moved: false,
        };
    }

    /// Opens the gesture of a drag that edits the path.
    fn begin(&mut self, s: &mut Session) {
        s.begin_gesture();
        self.before = Some(Box::new(s.doc.clone()));
    }

    /// A press on node `r`'s round widget: rounds the selected corners.
    fn press_radius(&mut self, s: &mut Session, r: PathNodeRef, zoom: f64) {
        let Some(corner) = s
            .path_of(r.shape)
            .and_then(|p| node_corner(&p.subpaths[r.subpath], r.index))
        else {
            return;
        };
        let w = world(s, r.shape);
        let nodes = selected_nodes(s)
            .into_iter()
            .filter(|n| {
                s.path_of(n.shape)
                    .is_some_and(|p| node_corner(&p.subpaths[n.subpath], n.index).is_some())
            })
            .collect();
        self.begin(s);
        self.drag = Drag::Radius {
            nodes,
            corner,
            to_local: w.inverse(),
            inset: WIDGET_INSET / (zoom * scale(w)),
        };
    }

    /// A double-click at `p`: on a node cycles its type (Corner → Smooth →
    /// Symmetric); on empty canvas returns to the selection tool.
    ///
    /// # Errors
    /// A message for the user when the change is refused.
    pub fn double_click(s: &mut Session, p: Point, zoom: f64) -> Result<(), String> {
        match Self::target(s, p, zoom) {
            NodeTarget::Node(r) => {
                let Some(kind) = s.path_node(r).map(|n| n.kind) else {
                    return Ok(());
                };
                path_ops::set_node_type(s, r, path_ops::next_type(kind))?;
                select_nodes(s, r.shape, vec![r]);
                Ok(())
            }
            NodeTarget::Empty => {
                s.editor.tool = vextor_core::ToolId::Select;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

mod drag;
mod shown;

use shown::scale;
pub use shown::{handles, radius_widgets};

#[cfg(test)]
mod tests;
