//! Close, open, split, join and reverse: changes to how a path's
//! subpaths run.

use kurbo::Affine;
use vextor_core::{Command, Geometry, NodeId, Path, PathNodeRef, Selection};
use vextor_geom::{join_open, open_endpoints, to_kurbo, transform_subpath};

use super::{
    JOIN, REVERSE, apply, batch, edited, editing_nodes, select_nodes, selected_nodes, world,
};
use crate::session::Session;

/// How two endpoints would be joined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    /// Two selected endpoints of one path.
    Endpoints(PathNodeRef, PathNodeRef),
    /// Two selected paths, at their nearest open ends.
    Paths(NodeId, NodeId),
}

/// The subpath Close would close: the selected nodes' (or, with none
/// selected, the edited path's first) open subpath of two or more nodes.
#[must_use]
pub fn close_target(s: &Session) -> Option<(NodeId, usize)> {
    let shape = edited(s)?;
    let path = s.path_of(shape)?;
    let can = |i: usize| {
        path.subpaths
            .get(i)
            .is_some_and(|sp| !sp.closed && sp.nodes.len() >= 2)
    };
    match selected_nodes(s).first() {
        Some(r) => can(r.subpath).then_some((shape, r.subpath)),
        None => (0..path.subpaths.len())
            .find(|&i| can(i))
            .map(|i| (shape, i)),
    }
}

/// The one selected node, when it sits on a closed subpath (Open).
#[must_use]
pub fn open_target(s: &Session) -> Option<PathNodeRef> {
    let [r] = selected_nodes(s)[..] else {
        return None;
    };
    s.path_of(r.shape)?
        .subpaths
        .get(r.subpath)?
        .closed
        .then_some(r)
}

/// The one selected node, when it is inside an open subpath (Split).
#[must_use]
pub fn split_target(s: &Session) -> Option<PathNodeRef> {
    let [r] = selected_nodes(s)[..] else {
        return None;
    };
    let sp = s.path_of(r.shape)?.subpaths.get(r.subpath)?;
    (!sp.closed && r.index > 0 && r.index + 1 < sp.nodes.len()).then_some(r)
}

/// What Join would join: two selected open ends of one path, or two
/// selected paths that each have an open end.
#[must_use]
pub fn join_target(s: &Session) -> Option<Join> {
    if let [a, b] = selected_nodes(s)[..] {
        let path = s.path_of(a.shape)?;
        let ends = open_endpoints(path);
        let end = |r: PathNodeRef| ends.contains(&(r.subpath, r.index));
        return (end(a) && end(b)).then_some(Join::Endpoints(a, b));
    }
    if let Selection::Objects(_) = s.editor.selection
        && let [a, b] = s.selected()[..]
    {
        let open = |id| s.path_of(id).is_some_and(|p| !open_endpoints(p).is_empty());
        return (open(a) && open(b)).then_some(Join::Paths(a, b));
    }
    None
}

/// The paths Reverse would turn round: every selected path.
#[must_use]
pub fn reverse_targets(s: &Session) -> Vec<NodeId> {
    s.selected()
        .into_iter()
        .filter(|&id| s.path_of(id).is_some())
        .collect()
}

/// The selected segment, as `(shape, subpath, index)`.
#[must_use]
pub fn segment_target(s: &Session) -> Option<(NodeId, usize, usize)> {
    match s.editor.selection {
        Selection::Segment {
            shape,
            subpath,
            index,
        } => Some((shape, subpath, index)),
        _ => None,
    }
}

/// Closes the target subpath of [`close_target`].
///
/// # Errors
/// A message for the user when the command is refused.
pub fn close(s: &mut Session) -> Result<(), String> {
    let Some((shape, subpath)) = close_target(s) else {
        return Ok(());
    };
    apply(s, Command::CloseSubPath { shape, subpath })?;
    select_nodes(s, shape, Vec::new());
    Ok(())
}

/// Opens a closed subpath at the selected node (it becomes both ends).
///
/// # Errors
/// A message for the user when the command is refused.
pub fn open(s: &mut Session) -> Result<(), String> {
    let Some(node) = open_target(s) else {
        return Ok(());
    };
    apply(s, Command::OpenAt { node })?;
    select_nodes(s, node.shape, Vec::new());
    Ok(())
}

/// Splits an open subpath in two at the selected node.
///
/// # Errors
/// A message for the user when the command is refused.
pub fn split(s: &mut Session) -> Result<(), String> {
    let Some(node) = split_target(s) else {
        return Ok(());
    };
    apply(s, Command::SplitAt { node })?;
    select_nodes(s, node.shape, Vec::new());
    Ok(())
}

/// Turns every subpath of the selected paths round.
///
/// # Errors
/// A message for the user when the command is refused.
pub fn reverse(s: &mut Session) -> Result<(), String> {
    let mut commands = Vec::new();
    for shape in reverse_targets(s) {
        let count = s.path_of(shape).map_or(0, |p| p.subpaths.len());
        commands.extend((0..count).map(|subpath| Command::Reverse { shape, subpath }));
    }
    if commands.is_empty() {
        return Ok(());
    }
    let shape = edited(s);
    batch(s, REVERSE, commands)?;
    // Node indices changed: keep the path, not stale nodes.
    if let Some(shape) = shape
        && editing_nodes(s)
    {
        select_nodes(s, shape, Vec::new());
    }
    Ok(())
}

/// Joins what [`join_target`] names. Two paths become the lower one,
/// keeping its style; the other's subpaths move into it.
///
/// # Errors
/// A message for the user when the command is refused.
pub fn join(s: &mut Session) -> Result<(), String> {
    match join_target(s) {
        Some(Join::Endpoints(a, b)) => {
            apply(s, Command::JoinEndpoints { a, b })?;
            select_nodes(s, a.shape, Vec::new());
            Ok(())
        }
        Some(Join::Paths(a, b)) => {
            let path = joined_paths(s, a, b).ok_or("Join failed: no open ends")?;
            let commands = vec![
                Command::SetGeometry {
                    items: vec![(a, Geometry::Path(path))],
                },
                Command::Delete { ids: vec![b] },
            ];
            batch(s, JOIN, commands)?;
            s.select(vec![a]);
            Ok(())
        }
        None => Ok(()),
    }
}

/// An end of `a`, an end of `b`, each as (subpath, index), and their
/// distance.
type Ends = ((usize, usize), (usize, usize), f64);

/// Path `a` with path `b` joined to it at their nearest open ends, in
/// `a`'s local space.
fn joined_paths(s: &Session, a: NodeId, b: NodeId) -> Option<Path> {
    let (pa, pb) = (s.path_of(a)?, s.path_of(b)?);
    let (wa, wb) = (world(s, a), world(s, b));
    let doc_point =
        |p: &Path, w: Affine, (sp, i): (usize, usize)| w * to_kurbo(p.subpaths[sp].nodes[i].anchor);
    let mut best: Option<Ends> = None;
    for ea in open_endpoints(pa) {
        for eb in open_endpoints(pb) {
            let d = (doc_point(pa, wa, ea) - doc_point(pb, wb, eb)).hypot();
            if best.is_none_or(|(_, _, bd)| d < bd) {
                best = Some((ea, eb, d));
            }
        }
    }
    let ((sa, ia), (sb, ib), _) = best?;
    let into_a = wa.inverse() * wb;
    let mut first = pa.subpaths[sa].clone();
    if ia == 0 && first.nodes.len() > 1 {
        first.reverse(); // the joined end comes last
    }
    let mut second = transform_subpath(&pb.subpaths[sb], into_a);
    if ib != 0 {
        second.reverse(); // the joined end comes first
    }
    let mut path = pa.clone();
    path.subpaths[sa] = join_open(first, second);
    let others = pb.subpaths.iter().enumerate().filter(|&(i, _)| i != sb);
    path.subpaths
        .extend(others.map(|(_, sp)| transform_subpath(sp, into_a)));
    Some(path)
}
