//! Node tool drags: what each kind of drag does as the pointer moves, and
//! what its release leaves selected.

use kurbo::{Point, Rect, Vec2};
use vextor_core::{Command, NodeId, PathNodeRef, Side};
use vextor_geom::{NodeCorner, bend, constrain_vector, handle_or_none};

use super::shown::nodes_in;
use super::{DRAG_THRESHOLD, Drag, NodeTool};
use crate::path_ops::{self, select_nodes, selected_nodes};
use crate::select::Mods;
use crate::session::Session;

/// One pointer move: where it is, the modifiers, the zoom and the
/// document's angle step.
#[derive(Clone, Copy)]
struct Move {
    p: Point,
    mods: Mods,
    zoom: f64,
    step: f64,
}

impl Move {
    /// True once the drag from `start` has passed the threshold (and
    /// remembers it in `moved`).
    fn past(self, start: Point, moved: &mut bool) -> bool {
        *moved = *moved || (self.p - start).hypot() * self.zoom >= DRAG_THRESHOLD;
        *moved
    }

    /// `v`, on the angle step when `⇧` is held.
    fn constrain(self, v: Vec2) -> Vec2 {
        if self.mods.shift {
            constrain_vector(v, self.step)
        } else {
            v
        }
    }
}

impl NodeTool {
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
        let m = Move {
            p,
            mods,
            zoom,
            step: s.doc.grid.snap.angle_step_deg,
        };
        if let Drag::Marquee { now, .. } = &mut self.drag {
            *now = p;
        }
        if !self.passed(m) {
            return Ok(());
        }
        match &mut self.drag {
            Drag::Idle => Ok(()),
            Drag::Marquee {
                start, base, shape, ..
            } => {
                if let Some(shape) = *shape {
                    marquee(s, shape, base, Rect::from_points(*start, p));
                }
                Ok(())
            }
            Drag::Nodes { start, applied, .. } => {
                let delta = m.constrain(p - *start);
                let step_by = delta - *applied;
                *applied = delta;
                move_nodes(s, step_by)
            }
            Drag::Handle { node, side, .. } => move_handle(s, *node, *side, m),
            Drag::Bend {
                segment,
                t,
                start,
                before,
                ..
            } => bend_segment(s, *segment, *t, before, p - *start),
            Drag::Radius {
                nodes,
                corner,
                to_local,
                inset,
            } => round(s, nodes, corner, *to_local * p - corner.bisector * *inset),
        }
    }

    /// True once the drag under way has passed the threshold (a round
    /// widget drag acts at once).
    fn passed(&mut self, m: Move) -> bool {
        match &mut self.drag {
            Drag::Marquee { start, moved, .. }
            | Drag::Nodes { start, moved, .. }
            | Drag::Handle { start, moved, .. }
            | Drag::Bend { start, moved, .. } => m.past(*start, moved),
            Drag::Idle | Drag::Radius { .. } => true,
        }
    }

    /// Primary button released.
    pub fn release(&mut self, s: &mut Session) {
        match std::mem::take(&mut self.drag) {
            Drag::Idle => {}
            Drag::Marquee { moved, base, .. } => {
                // A click on empty canvas leaves the path.
                if !moved && base.is_empty() {
                    s.select(Vec::new());
                }
            }
            Drag::Nodes { moved, clicked, .. } => {
                s.end_gesture();
                if !moved && let Some(r) = clicked {
                    select_nodes(s, r.shape, vec![r]);
                }
            }
            Drag::Handle { .. } | Drag::Bend { .. } | Drag::Radius { .. } => s.end_gesture(),
        }
        self.before = None;
    }
}

/// Selects `base` plus the nodes of `shape` inside `r`.
fn marquee(s: &mut Session, shape: NodeId, base: &[PathNodeRef], r: Rect) {
    let mut refs = base.to_vec();
    for n in nodes_in(s, shape, r) {
        if !refs.contains(&n) {
            refs.push(n);
        }
    }
    select_nodes(s, shape, refs);
}

/// Moves the selected nodes by `by` (document space).
fn move_nodes(s: &mut Session, by: Vec2) -> Result<(), String> {
    let nodes = selected_nodes(s);
    let Some(shape) = nodes.first().map(|r| r.shape) else {
        return Ok(());
    };
    let delta = path_ops::local_vector(s, shape, by);
    path_ops::apply(s, Command::MovePathNodes { nodes, delta })
}

/// Puts node `node`'s handle on `side` at the pointer; `⌥` moves it alone.
fn move_handle(s: &mut Session, node: PathNodeRef, side: Side, m: Move) -> Result<(), String> {
    let Some(n) = s.path_node(node) else {
        return Ok(());
    };
    let tip = path_ops::local_point(s, node.shape, m.p);
    let v = m.constrain(Vec2::new(tip.x - n.anchor.x, tip.y - n.anchor.y));
    path_ops::set_handle(s, node, side, handle_or_none(v), m.mods.alt)
}

/// Bends `segment` (as it was at the press, `before`) so its point at `t`
/// follows the pointer, moved `by` (document space) since the press.
fn bend_segment(
    s: &mut Session,
    segment: (NodeId, usize, usize),
    t: f64,
    before: &vextor_core::SubPath,
    by: Vec2,
) -> Result<(), String> {
    let delta = path_ops::local_vector(s, segment.0, by);
    match bend(before, segment.2, t, Vec2::new(delta.x, delta.y)) {
        Some((out, inn)) => path_ops::set_segment_handles(s, segment, out, inn),
        None => Ok(()),
    }
}

/// Rounds `nodes` to the radius whose arc middle is nearest `local`.
fn round(
    s: &mut Session,
    nodes: &[PathNodeRef],
    corner: &NodeCorner,
    local: Point,
) -> Result<(), String> {
    path_ops::set_corner_radius(s, nodes, corner.radius_at(local))
}
