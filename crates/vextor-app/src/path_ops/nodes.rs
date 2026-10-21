//! Node-level edits: segments, inserting and deleting nodes, node types,
//! handles and corner radii.

use kurbo::Point;
use vextor_core::{Command, Geometry, NodeId, NodeType, PathNodeRef, Side, Vec2};
use vextor_geom::{from_point, from_vec, smooth_handles, split_segment, to_kurbo};

use super::{
    ADD_NODE, BEND, MOVE_HANDLE, NODE_TYPE, ROUND, apply, batch, edited, local_vector,
    segment_target, select_nodes, selected_nodes, world,
};
use crate::session::Session;

/// Straight ↔ curve for the selected segment.
///
/// # Errors
/// A message for the user when the command is refused.
pub fn toggle_segment(s: &mut Session) -> Result<(), String> {
    let Some((shape, subpath, index)) = segment_target(s) else {
        return Ok(());
    };
    apply(
        s,
        Command::ToggleSegment {
            shape,
            subpath,
            index,
        },
    )
}

/// Inserts a node at `t` on segment `index` of `shape`'s `subpath`
/// without changing the outline, and selects it.
///
/// # Errors
/// A message for the user when the command is refused.
pub fn insert_node(
    s: &mut Session,
    shape: NodeId,
    subpath: usize,
    index: usize,
    t: f64,
) -> Result<(), String> {
    let Some(mut path) = s.path_of(shape).cloned() else {
        return Ok(());
    };
    let Some(new) = path
        .subpaths
        .get_mut(subpath)
        .and_then(|sp| split_segment(sp, index, t))
    else {
        return Ok(());
    };
    let set = Command::SetGeometry {
        items: vec![(shape, Geometry::Path(path))],
    };
    batch(s, ADD_NODE, vec![set])?;
    let at = PathNodeRef {
        shape,
        subpath,
        index: new,
    };
    select_nodes(s, shape, vec![at]);
    Ok(())
}

/// Inserts a node in the middle of the selected segment (`+`).
///
/// # Errors
/// A message for the user when the command is refused.
pub fn insert_in_segment(s: &mut Session) -> Result<(), String> {
    match segment_target(s) {
        Some((shape, subpath, index)) => insert_node(s, shape, subpath, index, 0.5),
        None => Ok(()),
    }
}

/// Deletes the selected nodes; the path stays continuous around them.
/// A subpath left with one node goes too, and a path left with none
/// removes its shape.
///
/// # Errors
/// A message for the user when the command is refused.
pub fn delete_nodes(s: &mut Session) -> Result<(), String> {
    let mut nodes = selected_nodes(s);
    let Some(shape) = nodes.first().map(|r| r.shape) else {
        return Ok(());
    };
    let Some(path) = s.path_of(shape) else {
        return Ok(());
    };
    for (subpath, sp) in path.subpaths.iter().enumerate() {
        let gone = nodes.iter().filter(|r| r.subpath == subpath).count();
        if gone > 0 && sp.nodes.len() == gone + 1 {
            let index = (0..sp.nodes.len())
                .find(|&i| !nodes.iter().any(|r| r.subpath == subpath && r.index == i))
                .expect("one node is left");
            nodes.push(PathNodeRef {
                shape,
                subpath,
                index,
            });
        }
    }
    apply(s, Command::DeletePathNodes { nodes })?;
    let left = s.path_of(shape).is_some();
    s.select(if left { vec![shape] } else { Vec::new() });
    Ok(())
}

/// Moves the selected nodes by (`dx`, `dy`) document px.
///
/// # Errors
/// A message for the user when the command is refused.
pub fn nudge_nodes(s: &mut Session, dx: f64, dy: f64) -> Result<(), String> {
    let nodes = selected_nodes(s);
    let Some(shape) = nodes.first().map(|r| r.shape) else {
        return Ok(());
    };
    let delta = local_vector(s, shape, kurbo::Vec2::new(dx, dy));
    apply(s, Command::MovePathNodes { nodes, delta })
}

/// Selects every node of the edited path (`⌘A` in node editing).
pub fn select_all_nodes(s: &mut Session) {
    let Some(shape) = edited(s) else {
        return;
    };
    let Some(path) = s.path_of(shape) else {
        return;
    };
    let refs = path
        .subpaths
        .iter()
        .enumerate()
        .flat_map(|(subpath, sp)| {
            (0..sp.nodes.len()).map(move |index| PathNodeRef {
                shape,
                subpath,
                index,
            })
        })
        .collect();
    select_nodes(s, shape, refs);
}

/// The type after `kind` when cycling with a double-click.
#[must_use]
pub fn next_type(kind: NodeType) -> NodeType {
    match kind {
        NodeType::Corner => NodeType::Smooth,
        NodeType::Smooth => NodeType::Symmetric,
        NodeType::Symmetric => NodeType::Corner,
    }
}

/// Gives node `r` type `kind`. Smooth or Symmetric without handles gets
/// handles along its neighbours; Corner drops its handles (a sharp
/// corner: `⌥`-drag a handle for a corner with free handles).
///
/// # Errors
/// A message for the user when the command is refused.
pub fn set_node_type(s: &mut Session, r: PathNodeRef, kind: NodeType) -> Result<(), String> {
    let Some(sp) = s.path_of(r.shape).and_then(|p| p.subpaths.get(r.subpath)) else {
        return Ok(());
    };
    let n = sp.nodes[r.index];
    let mut commands = Vec::new();
    if kind == NodeType::Corner {
        commands.push(Command::SetNodeType { node: r, kind });
        commands.push(handles(r, None, None));
    } else {
        if n.handle_in.is_none() && n.handle_out.is_none() {
            let (h_in, h_out) = smooth_handles(sp, r.index);
            commands.push(handles(r, h_in, h_out));
        }
        commands.push(Command::SetNodeType { node: r, kind });
    }
    batch(s, NODE_TYPE, commands)
}

fn handles(node: PathNodeRef, handle_in: Option<Vec2>, handle_out: Option<Vec2>) -> Command {
    Command::SetHandles {
        node,
        handle_in,
        handle_out,
    }
}

/// Moves node `r`'s anchor to `to` (document space); its handles follow.
///
/// # Errors
/// A message for the user when the command is refused.
pub fn move_node_to(s: &mut Session, r: PathNodeRef, to: Point) -> Result<(), String> {
    let Some(n) = s.path_node(r) else {
        return Ok(());
    };
    let local = world(s, r.shape).inverse() * to;
    let delta = from_vec(local - to_kurbo(n.anchor));
    apply(
        s,
        Command::MovePathNodes {
            nodes: vec![r],
            delta,
        },
    )
}

/// Sets node `r`'s handle on `side` to `value` (local space). The other
/// handle follows the node type, unless `free`: then the node becomes a
/// Corner first, so only this handle moves.
///
/// # Errors
/// A message for the user when the command is refused.
pub fn set_handle(
    s: &mut Session,
    r: PathNodeRef,
    side: Side,
    value: Option<Vec2>,
    free: bool,
) -> Result<(), String> {
    let Some(n) = s.path_node(r).copied() else {
        return Ok(());
    };
    let mut commands = Vec::new();
    if free && n.kind != NodeType::Corner {
        commands.push(Command::SetNodeType {
            node: r,
            kind: NodeType::Corner,
        });
    }
    commands.push(match side {
        Side::In => handles(r, value, n.handle_out),
        Side::Out => handles(r, n.handle_in, value),
    });
    batch(s, MOVE_HANDLE, commands)
}

/// Gives segment `index` of `shape`'s `subpath` the handles `out` (on its
/// start node) and `inn` (on its end node), local space.
///
/// # Errors
/// A message for the user when the command is refused.
pub fn set_segment_handles(
    s: &mut Session,
    (shape, subpath, index): (NodeId, usize, usize),
    out: Vec2,
    inn: Vec2,
) -> Result<(), String> {
    let Some(sp) = s.path_of(shape).and_then(|p| p.subpaths.get(subpath)) else {
        return Ok(());
    };
    let j = (index + 1) % sp.nodes.len();
    let (a, b) = (sp.nodes[index], sp.nodes[j]);
    let at = |index| PathNodeRef {
        shape,
        subpath,
        index,
    };
    let commands = vec![
        handles(at(index), a.handle_in, Some(out)),
        handles(at(j), Some(inn), b.handle_out),
    ];
    batch(s, BEND, commands)
}

/// Sets the corner radius of nodes `refs` of one path (local px, 0 =
/// sharp).
///
/// # Errors
/// A message for the user when the command is refused.
pub fn set_corner_radius(s: &mut Session, refs: &[PathNodeRef], radius: f64) -> Result<(), String> {
    let Some(shape) = refs.first().map(|r| r.shape) else {
        return Ok(());
    };
    let Some(mut path) = s.path_of(shape).cloned() else {
        return Ok(());
    };
    for r in refs.iter().filter(|r| r.shape == shape) {
        if let Some(n) = path
            .subpaths
            .get_mut(r.subpath)
            .and_then(|sp| sp.nodes.get_mut(r.index))
        {
            n.corner_radius = radius.max(0.0);
        }
    }
    let set = Command::SetGeometry {
        items: vec![(shape, Geometry::Path(path))],
    };
    batch(s, ROUND, vec![set])
}

/// Node `r`'s anchor in document space.
#[must_use]
pub fn anchor_in_doc(s: &Session, r: PathNodeRef) -> Option<Point> {
    let n = s.path_node(r)?;
    Some(world(s, r.shape) * to_kurbo(n.anchor))
}

/// `p` (document space) in shape `id`'s local space, as a model point.
#[must_use]
pub fn local_point(s: &Session, id: NodeId, p: Point) -> vextor_core::Point {
    from_point(world(s, id).inverse() * p)
}
