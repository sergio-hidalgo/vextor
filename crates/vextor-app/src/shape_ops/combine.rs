//! Union, Subtract, Intersect, Exclude and Divide: the selected objects
//! are replaced, in one undo step, by the result at the bottom object's
//! place.

use vextor_core::{Command, Container, FillRule, Geometry, NodeKind, Shape};
use vextor_geom::{Area, BooleanOp, divide, result_path};

use super::operand::{Place, collect, ids};
use crate::session::Session;

/// `op` on the selected objects: one path with the bottom object's style
/// and properties, selected.
///
/// # Errors
/// A message when the selection cannot take part or nothing is left.
pub(super) fn boolean(s: &mut Session, op: BooleanOp, label: &'static str) -> Result<(), String> {
    let selected = s.selected();
    let operands = collect(s, &selected, label)?;
    let place =
        Place::of(&s.doc, selected[0]).ok_or_else(|| format!("{label}: nothing to replace"))?;
    let areas: Vec<Area> = operands.iter().map(|o| o.area.clone()).collect();
    let result = Area::combine(op, &areas);
    let outline = place.to_local * result.to_bezpath();
    let path = result_path(&outline, FillRule::NonZero)
        .ok_or_else(|| format!("{label}: the result is empty, so nothing changed"))?;
    let id = ids(s)();
    let shape = Shape {
        geometry: Geometry::Path(path),
        style: operands[0].style.clone(),
    };
    let node = place.node(id, NodeKind::Shape(shape));
    s.apply(Command::Batch {
        label,
        commands: vec![Command::ReplaceWith {
            removed: selected,
            inserted: vec![node],
            parent: Some(place.parent),
            index: place.index,
        }],
    })
    .map_err(|e| format!("{label} failed: {e}"))?;
    s.select(vec![id]);
    Ok(())
}

/// Divide: every region where the selected objects overlap differently
/// becomes its own path, painted like the topmost object over it, in a
/// new group at the bottom object's place (selected).
///
/// # Errors
/// A message when the selection cannot take part.
pub(super) fn divide_selection(s: &mut Session, label: &'static str) -> Result<(), String> {
    let selected = s.selected();
    let operands = collect(s, &selected, label)?;
    let place =
        Place::of(&s.doc, selected[0]).ok_or_else(|| format!("{label}: nothing to replace"))?;
    let areas: Vec<Area> = operands.iter().map(|o| o.area.clone()).collect();
    let mut alloc = ids(s);
    let group_id = alloc();
    let mut children = Vec::new();
    let mut inserted = Vec::new();
    for (piece, top) in divide(&areas) {
        let outline = place.to_local * piece.to_bezpath();
        let Some(path) = result_path(&outline, FillRule::NonZero) else {
            continue;
        };
        let id = alloc();
        children.push(id);
        inserted.push(vextor_core::Node::new(
            id,
            NodeKind::Shape(Shape {
                geometry: Geometry::Path(path),
                style: operands[top].style.clone(),
            }),
        ));
    }
    if children.is_empty() {
        return Err(format!("{label}: the objects cover nothing"));
    }
    let group = vextor_core::Node {
        transform: place.transform,
        ..vextor_core::Node::new(
            group_id,
            NodeKind::Group(Container {
                children,
                clip: None,
            }),
        )
    };
    inserted.insert(0, group);
    s.apply(Command::Batch {
        label,
        commands: vec![Command::ReplaceWith {
            removed: selected,
            inserted,
            parent: Some(place.parent),
            index: place.index,
        }],
    })
    .map_err(|e| format!("{label} failed: {e}"))?;
    s.select(vec![group_id]);
    Ok(())
}
