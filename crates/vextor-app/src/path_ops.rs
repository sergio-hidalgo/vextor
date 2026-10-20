//! Path editing actions (spec 07) on a session: what the node tool, the
//! pen, the keys, the Object ▸ Path menu, the canvas's context menu and
//! the inspector's Path section do to paths. Each action is one undo
//! step.
//!
//! In node editing the selection is the path's selected nodes, or one of
//! its segments; a path with no node selected is simply the one selected
//! object ([`edited`]).

use kurbo::{Affine, Point};
use vextor_core::{Command, NodeId, PathNodeRef, Selection, Vec2};
use vextor_geom::{from_vec, to_affine};

use crate::session::Session;

mod action;
mod nodes;
mod structure;

pub use action::PathAction;

pub use nodes::{
    anchor_in_doc, delete_nodes, insert_in_segment, insert_node, local_point, move_node_to,
    next_type, nudge_nodes, select_all_nodes, set_corner_radius, set_handle, set_node_type,
    set_segment_handles, toggle_segment,
};
pub use structure::{
    close, close_target, join, join_target, open, open_target, reverse, reverse_targets,
    segment_target, split, split_target,
};

/// Undo label of joining two paths into one.
pub const JOIN: &str = "Join";
/// Undo label of reversing paths.
pub const REVERSE: &str = "Reverse path";
/// Undo label of inserting a node on a segment.
pub const ADD_NODE: &str = "Add node";
/// Undo label of a node type change.
pub const NODE_TYPE: &str = "Change node type";
/// Undo label of dragging or typing a handle.
pub const MOVE_HANDLE: &str = "Move handle";
/// Undo label of bending a segment.
pub const BEND: &str = "Bend segment";
/// Undo label of rounding a node's corner.
pub const ROUND: &str = "Round corner";

/// The path being node-edited: the shape of the selected nodes or
/// segment, or the one selected object when it is a path.
#[must_use]
pub fn edited(s: &Session) -> Option<NodeId> {
    let id = match &s.editor.selection {
        Selection::PathNodes(refs) => refs.first().map(|r| r.shape),
        Selection::Segment { shape, .. } => Some(*shape),
        Selection::Objects(ids) if ids.len() == 1 => ids.first().copied(),
        _ => None,
    }?;
    s.path_of(id).is_some().then_some(id)
}

/// The selected path nodes, in path order.
#[must_use]
pub fn selected_nodes(s: &Session) -> Vec<PathNodeRef> {
    let mut refs = match &s.editor.selection {
        Selection::PathNodes(refs) => refs.clone(),
        _ => Vec::new(),
    };
    refs.sort();
    refs.dedup();
    refs
}

/// Selects `refs` of path `shape`; none leaves the path itself selected.
pub fn select_nodes(s: &mut Session, shape: NodeId, refs: Vec<PathNodeRef>) {
    s.editor.selection = if refs.is_empty() {
        Selection::Objects(vec![shape])
    } else {
        Selection::PathNodes(refs)
    };
}

/// True while path nodes or a segment are selected: Delete, the arrows
/// and Esc act on them.
#[must_use]
pub fn editing_nodes(s: &Session) -> bool {
    matches!(
        s.editor.selection,
        Selection::PathNodes(_) | Selection::Segment { .. }
    )
}

/// Shape `id`'s local space → document space.
#[must_use]
pub fn world(s: &Session, id: NodeId) -> Affine {
    to_affine(s.doc.world_transform(id))
}

/// A document-space vector in shape `id`'s local space.
#[must_use]
pub fn local_vector(s: &Session, id: NodeId, v: kurbo::Vec2) -> Vec2 {
    let inv = world(s, id).inverse();
    from_vec(inv * (Point::ZERO + v) - inv * Point::ZERO)
}

pub(crate) fn apply(s: &mut Session, cmd: Command) -> Result<(), String> {
    let label = cmd.label();
    s.apply(cmd).map_err(|e| format!("{label} failed: {e}"))
}

pub(crate) fn batch(
    s: &mut Session,
    label: &'static str,
    commands: Vec<Command>,
) -> Result<(), String> {
    apply(s, Command::Batch { label, commands })
}

#[cfg(test)]
mod tests;
