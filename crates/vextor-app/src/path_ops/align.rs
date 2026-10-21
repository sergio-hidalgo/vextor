//! Align and distribute selected path nodes (spec 07), with the same
//! options as objects: each node is a point, so Align lines up anchors
//! (left / centre / right, top / middle / bottom) and Distribute spaces
//! three or more evenly. Nodes line up with each other, or with the
//! artboard when only one is selected or Align to Artboard is on. The
//! nodes' handles move with them. One undo step.

use kurbo::{Point, Rect};
use vextor_core::{Command, PathNodeRef};
use vextor_geom::Align;

use super::{anchor_in_doc, batch, local_vector, selected_nodes};
use crate::edit_ops::artboard_at;
use crate::session::Session;

/// Undo label of aligning nodes.
pub const ALIGN_NODES: &str = "Align nodes";
/// Undo label of distributing nodes.
pub const DISTRIBUTE_NODES: &str = "Distribute nodes";

/// The selected nodes and their anchors in document space.
fn anchors(s: &Session) -> Vec<(PathNodeRef, Point)> {
    selected_nodes(s)
        .into_iter()
        .filter_map(|r| Some((r, anchor_in_doc(s, r)?)))
        .collect()
}

/// Lines up the selected nodes as `how` says: with each other, or with
/// the artboard (one node, or `to_artboard`).
///
/// # Errors
/// A message for the user when the move is refused.
pub fn align_nodes(s: &mut Session, how: Align, to_artboard: bool) -> Result<(), String> {
    let nodes = anchors(s);
    let boxes: Vec<Rect> = nodes
        .iter()
        .map(|&(_, p)| Rect::from_points(p, p))
        .collect();
    let Some(selection) = boxes.iter().copied().reduce(|a, b| a.union(b)) else {
        return Ok(());
    };
    let target = if to_artboard || nodes.len() == 1 {
        match artboard_at(&s.doc, selection.center()) {
            Some(a) => a,
            None => return Ok(()),
        }
    } else {
        selection
    };
    let moves = vextor_geom::align(&boxes, target, how);
    move_each(s, &nodes, &moves, ALIGN_NODES)
}

/// Spaces three or more selected nodes evenly, left to right when
/// `horizontal`, otherwise top to bottom; the outermost two stay.
///
/// # Errors
/// A message for the user when the move is refused.
pub fn distribute_nodes(s: &mut Session, horizontal: bool) -> Result<(), String> {
    let nodes = anchors(s);
    let boxes: Vec<Rect> = nodes
        .iter()
        .map(|&(_, p)| Rect::from_points(p, p))
        .collect();
    let moves = vextor_geom::distribute(&boxes, horizontal);
    move_each(s, &nodes, &moves, DISTRIBUTE_NODES)
}

/// Moves each node by its own document-space vector, as one step.
fn move_each(
    s: &mut Session,
    nodes: &[(PathNodeRef, Point)],
    moves: &[kurbo::Vec2],
    label: &'static str,
) -> Result<(), String> {
    let commands: Vec<Command> = nodes
        .iter()
        .zip(moves)
        .filter(|(_, m)| m.hypot() > vextor_core::EPS)
        .map(|(&(r, _), &m)| Command::MovePathNodes {
            nodes: vec![r],
            delta: local_vector(s, r.shape, m),
        })
        .collect();
    if commands.is_empty() {
        return Ok(());
    }
    batch(s, label, commands)
}
