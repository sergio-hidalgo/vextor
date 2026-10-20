//! Object ▸ Align, Distribute, Flip, nudging, and the inspector's
//! position, size and rotation: operations that move or resize the
//! selection. Each is one undo step.

use kurbo::{Affine, Rect as KRect, Vec2 as KVec2};
use vextor_core::{Command, Document, NodeId, NodeKind, Rect, Transform};
use vextor_geom::{Align, Fonts, retarget, union_bounds};

use super::apply;
use crate::session::Session;

/// Aligns the selection: to its own bounds when two or more objects are
/// selected, to the artboard when one is or `to_artboard` is set.
///
/// # Errors
/// A message for the user when the move is refused.
pub fn align(s: &mut Session, how: Align, to_artboard: bool) -> Result<(), String> {
    let ids = s.selected();
    let boxes = boxes(&s.doc, &s.fonts, &ids);
    let Some(selection) = union(&boxes) else {
        return Ok(());
    };
    let target = if to_artboard || ids.len() == 1 {
        match artboard_at(&s.doc, selection.center()) {
            Some(a) => a,
            None => return Ok(()),
        }
    } else {
        selection
    };
    let moves = vextor_geom::align(&boxes.iter().map(|b| b.1).collect::<Vec<_>>(), target, how);
    move_each(s, &boxes, &moves, "Align")
}

/// Spaces three or more objects evenly, left to right when `horizontal`,
/// otherwise top to bottom.
///
/// # Errors
/// A message for the user when the move is refused.
pub fn distribute(s: &mut Session, horizontal: bool) -> Result<(), String> {
    let boxes = boxes(&s.doc, &s.fonts, &s.selected());
    let moves = vextor_geom::distribute(&boxes.iter().map(|b| b.1).collect::<Vec<_>>(), horizontal);
    move_each(s, &boxes, &moves, "Distribute")
}

/// Mirrors the selection about its centre.
///
/// # Errors
/// A message for the user when the change is refused.
pub fn flip(s: &mut Session, horizontal: bool) -> Result<(), String> {
    let ids = s.selected();
    let Some(b) = union_bounds(&s.doc, &s.fonts, &ids) else {
        return Ok(());
    };
    transform(s, vextor_geom::flip(b, horizontal), "Flip")
}

/// Moves the selection by `(dx, dy)` document px.
///
/// # Errors
/// A message for the user when the move is refused.
pub fn nudge(s: &mut Session, dx: f64, dy: f64) -> Result<(), String> {
    transform(s, Affine::translate((dx, dy)), "Nudge")
}

/// Resizes the selection by `change` (document space, a scale), baking the
/// stretch into the shapes so strokes keep their width.
///
/// # Errors
/// A message for the user when the change is refused.
pub fn resize(s: &mut Session, change: Affine) -> Result<(), String> {
    let ids = s.selected();
    if ids.is_empty() {
        return Ok(());
    }
    let commands = resize_commands(&s.doc, &ids, change);
    apply(
        s,
        Command::Batch {
            label: "Resize",
            commands,
        },
    )
}

/// The commands that resize `ids` in `doc` by `change`: new transforms and
/// new geometries ([`vextor_geom::resize`]).
#[must_use]
pub fn resize_commands(doc: &Document, ids: &[NodeId], change: Affine) -> Vec<Command> {
    let r = vextor_geom::resize(doc, ids, change);
    let mut commands = vec![Command::SetTransforms {
        items: r.transforms,
    }];
    if !r.geometries.is_empty() {
        commands.push(Command::SetGeometry {
            items: r.geometries,
        });
    }
    for (id, t) in r.texts {
        commands.push(Command::SetTextStyle {
            ids: vec![id],
            font: t.font,
            size: t.size,
            line_height: t.line_height,
            letter_spacing: t.letter_spacing,
            align: t.align,
        });
    }
    commands
}

/// True when any of `ids`, or anything inside them, is a text object:
/// resizing then keeps proportions (text scales, it does not stretch).
#[must_use]
pub fn has_text(doc: &Document, ids: &[NodeId]) -> bool {
    ids.iter().any(|&id| {
        doc.subtree(id)
            .iter()
            .any(|n| matches!(doc.nodes.get(n).map(|n| &n.kind), Some(NodeKind::Text(_))))
    })
}

/// Applies `change` (document space) to every selected object, as one
/// step named `label`.
///
/// # Errors
/// A message for the user when the change is refused.
pub fn transform(s: &mut Session, change: Affine, label: &'static str) -> Result<(), String> {
    let ids = s.selected();
    if ids.is_empty() {
        return Ok(());
    }
    let items = retarget(&s.doc, &ids, change);
    apply(
        s,
        Command::Batch {
            label,
            commands: vec![Command::SetTransforms { items }],
        },
    )
}

fn boxes(doc: &Document, fonts: &Fonts, ids: &[NodeId]) -> Vec<(NodeId, KRect)> {
    ids.iter()
        .filter_map(|&id| Some((id, vextor_geom::bounds(doc, fonts, id)?)))
        .collect()
}

fn union(boxes: &[(NodeId, KRect)]) -> Option<KRect> {
    boxes.iter().map(|b| b.1).reduce(|a, b| a.union(b))
}

/// The artboard holding `p`, else the first one.
fn artboard_at(doc: &Document, p: kurbo::Point) -> Option<KRect> {
    let rect = |r: Rect| KRect::new(r.x, r.y, r.x + r.width, r.y + r.height);
    doc.artboards
        .iter()
        .map(|a| rect(a.rect))
        .find(|r| r.contains(p))
        .or_else(|| doc.artboards.first().map(|a| rect(a.rect)))
}

/// Moves each boxed object by its own vector, as one step.
fn move_each(
    s: &mut Session,
    boxes: &[(NodeId, KRect)],
    moves: &[KVec2],
    label: &'static str,
) -> Result<(), String> {
    let items: Vec<(NodeId, Transform)> = boxes
        .iter()
        .zip(moves)
        .filter(|(_, m)| m.hypot() > vextor_core::EPS)
        .flat_map(|((id, _), m)| retarget(&s.doc, &[*id], Affine::translate(*m)))
        .collect();
    if items.is_empty() {
        return Ok(());
    }
    apply(
        s,
        Command::Batch {
            label,
            commands: vec![Command::SetTransforms { items }],
        },
    )
}
