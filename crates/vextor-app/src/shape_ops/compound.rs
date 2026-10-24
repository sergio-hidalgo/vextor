//! Make / Release Compound Path and the fill rule: a compound path is one
//! path of several subpaths, where overlaps make holes (even-odd).

use vextor_core::{Command, FillRule, Geometry, NodeKind, Path, Shape};
use vextor_geom::{geometry_to_path, to_affine, transform_subpath};

use super::modify::shapes;
use super::operand::{Place, ids};
use super::{MAKE_COMPOUND, RELEASE_COMPOUND};
use crate::session::Session;

/// Undo label of a fill rule change.
pub const FILL_RULE: &str = "Change fill rule";

/// The selected shapes as one even-odd path with the bottom one's style
/// and properties, selected.
///
/// # Errors
/// A message when fewer than two shapes are selected or the change is
/// refused.
pub(super) fn make(s: &mut Session) -> Result<(), String> {
    let selected = shapes(s);
    if selected.len() < 2 || selected.len() != s.selected().len() {
        return Err(format!("{MAKE_COMPOUND}: select two or more shapes"));
    }
    let bottom = selected[0].0;
    let place =
        Place::of(&s.doc, bottom).ok_or_else(|| format!("{MAKE_COMPOUND}: nothing to replace"))?;
    let mut subpaths = Vec::new();
    for (id, shape) in &selected {
        let into_bottom = place.to_local * to_affine(s.doc.world_transform(*id));
        let path = match &shape.geometry {
            Geometry::Path(p) => p.clone(),
            g => geometry_to_path(g),
        };
        subpaths.extend(
            path.subpaths
                .iter()
                .map(|sp| transform_subpath(sp, into_bottom)),
        );
    }
    let shape = Shape {
        geometry: Geometry::Path(Path {
            subpaths,
            fill_rule: FillRule::EvenOdd,
        }),
        style: selected[0].1.style.clone(),
    };
    let id = ids(s)();
    let removed = selected.iter().map(|(id, _)| *id).collect();
    s.apply(Command::Batch {
        label: MAKE_COMPOUND,
        commands: vec![Command::ReplaceWith {
            removed,
            inserted: vec![place.node(id, NodeKind::Shape(shape))],
            parent: Some(place.parent),
            index: place.index,
        }],
    })
    .map_err(|e| format!("{MAKE_COMPOUND} failed: {e}"))?;
    s.select(vec![id]);
    Ok(())
}

/// True when `shape` is a path of more than one subpath.
pub(super) fn is_compound(shape: &Shape) -> bool {
    matches!(&shape.geometry, Geometry::Path(p) if p.subpaths.len() > 1)
}

/// Each selected compound path becomes one path per subpath, in its place
/// and with its style (the first keeps its name); all are selected.
///
/// # Errors
/// A message when no compound path is selected or the change is refused.
pub(super) fn release(s: &mut Session) -> Result<(), String> {
    let mut alloc = ids(s);
    let mut commands = Vec::new();
    let mut results = Vec::new();
    for (id, shape) in shapes(s) {
        let Geometry::Path(p) = &shape.geometry else {
            continue;
        };
        if p.subpaths.len() < 2 {
            continue;
        }
        let mut place = Place::of(&s.doc, id)
            .ok_or_else(|| format!("{RELEASE_COMPOUND}: nothing to replace"))?;
        let mut inserted = Vec::new();
        for sp in &p.subpaths {
            let piece = Shape {
                geometry: Geometry::Path(Path {
                    subpaths: vec![sp.clone()],
                    fill_rule: p.fill_rule,
                }),
                style: shape.style.clone(),
            };
            let new = alloc();
            inserted.push(place.node(new, NodeKind::Shape(piece)));
            results.push(new);
            place.name = None; // one name, on the bottom piece
        }
        commands.push(Command::ReplaceWith {
            removed: vec![id],
            inserted,
            parent: Some(place.parent),
            index: place.index,
        });
    }
    if commands.is_empty() {
        return Err(format!(
            "{RELEASE_COMPOUND}: select a path with several parts"
        ));
    }
    s.apply(Command::Batch {
        label: RELEASE_COMPOUND,
        commands,
    })
    .map_err(|e| format!("{RELEASE_COMPOUND} failed: {e}"))?;
    s.select(results);
    Ok(())
}

/// Sets the fill rule of path `id`.
///
/// # Errors
/// A message when `id` is not a path or the change is refused.
pub fn set_fill_rule(
    s: &mut Session,
    id: vextor_core::NodeId,
    rule: FillRule,
) -> Result<(), String> {
    let mut path = s
        .path_of(id)
        .cloned()
        .ok_or_else(|| format!("{FILL_RULE}: select a path"))?;
    if path.fill_rule == rule {
        return Ok(());
    }
    path.fill_rule = rule;
    s.apply(Command::Batch {
        label: FILL_RULE,
        commands: vec![Command::SetGeometry {
            items: vec![(id, Geometry::Path(path))],
        }],
    })
    .map_err(|e| format!("{FILL_RULE} failed: {e}"))
}
