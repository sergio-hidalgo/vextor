//! Dragging rows (spec 10): where a drop lands and the move it makes.

use vextor_core::{Command, Document, NodeId, Parent};

use super::{Row, apply};
use crate::session::Session;

/// Where a dragged row would land, relative to the row under the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drop {
    /// Just above the row (higher in the stack).
    Above(NodeId),
    /// Just below it.
    Below(NodeId),
    /// Inside a layer or group, on top of its contents.
    Into(NodeId),
}

/// The drop `at` a pointer `frac` of the way down `row` (0 top, 1
/// bottom): a layer or group takes the middle half as Into.
#[must_use]
pub fn drop_at(row: &Row, frac: f32) -> Drop {
    match frac {
        f if row.container && (0.25..0.75).contains(&f) => Drop::Into(row.id),
        f if f < 0.5 => Drop::Above(row.id),
        _ => Drop::Below(row.id),
    }
}

/// The rows a drag that starts on `id` carries: the selection when `id`
/// is part of it, else `id` alone.
#[must_use]
pub fn dragged(s: &Session, id: NodeId) -> Vec<NodeId> {
    let selected = s.selected();
    if selected.contains(&id) {
        selected
    } else {
        vec![id]
    }
}

/// The move that drops `ids` `at` a row, or `None` where they cannot go:
/// layers only among layers, objects only into layers and groups, never
/// into themselves. Objects keep their place on the canvas.
#[must_use]
pub fn drop_command(doc: &Document, ids: &[NodeId], at: Drop) -> Option<Command> {
    let is_layer = |id: &NodeId| doc.layers.contains(id);
    let layers = ids.iter().filter(|id| is_layer(id)).count();
    if ids.is_empty() || (layers != 0 && layers != ids.len()) {
        return None;
    }
    let target = match at {
        Drop::Above(t) | Drop::Below(t) | Drop::Into(t) => t,
    };
    if ids.iter().any(|&id| doc.subtree(id).contains(&target)) {
        return None;
    }
    let (parent, before) = if layers > 0 {
        let i = doc.layers.iter().position(|&l| l == target)?;
        match at {
            Drop::Above(_) => (None, i + 1),
            Drop::Below(_) => (None, i),
            Drop::Into(_) => return None,
        }
    } else if is_layer(&target) || matches!(at, Drop::Into(_)) {
        let len = doc.nodes.get(&target)?.container()?.children.len();
        (Some(target), len)
    } else {
        let Some(Parent::Node(p)) = doc.parent_of(target) else {
            return None;
        };
        let siblings = &doc.nodes.get(&p)?.container()?.children;
        let i = siblings.iter().position(|&c| c == target)?;
        match at {
            Drop::Above(_) => (Some(p), i + 1),
            _ => (Some(p), i),
        }
    };
    doc.move_to(ids, parent, before).ok()
}

/// Drops the dragged rows; the moved objects stay selected.
///
/// # Errors
/// A message for the user when the move is refused.
pub fn drop(s: &mut Session, ids: &[NodeId], at: Drop) -> Result<(), String> {
    apply(s, drop_command(&s.doc, ids, at))
}
