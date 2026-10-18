//! Path-editing commands.

use std::collections::{BTreeMap, BTreeSet};

use super::{Tx, structure};
use crate::error::CoreError;
use crate::ids::NodeId;
use crate::math::{EPS, Vec2};
use crate::node::{Geometry, NodeKind, Shape};
use crate::path::{NodeType, Path, PathNode, PathNodeRef, Side, SubPath};

fn path_mut<'t>(tx: &'t mut Tx<'_>, shape: NodeId) -> Result<&'t mut Path, CoreError> {
    match &mut tx.node_mut(shape)?.kind {
        NodeKind::Shape(Shape {
            geometry: Geometry::Path(p),
            ..
        }) => Ok(p),
        _ => Err(CoreError::WrongKind(shape, "path")),
    }
}

fn subpath_mut(path: &mut Path, index: usize) -> Result<&mut SubPath, CoreError> {
    let len = path.subpaths.len();
    path.subpaths
        .get_mut(index)
        .ok_or(CoreError::IndexOutOfRange { index, len })
}

fn node_mut<'t>(tx: &'t mut Tx<'_>, r: PathNodeRef) -> Result<&'t mut PathNode, CoreError> {
    let sp = subpath_mut(path_mut(tx, r.shape)?, r.subpath)?;
    let len = sp.nodes.len();
    sp.nodes.get_mut(r.index).ok_or(CoreError::IndexOutOfRange {
        index: r.index,
        len,
    })
}

fn ensure_open(sp: &SubPath) -> Result<(), CoreError> {
    if sp.closed {
        Err(CoreError::InvalidCommand("the subpath is closed"))
    } else {
        Ok(())
    }
}

pub(super) fn add_node(tx: &mut Tx<'_>, at: PathNodeRef, node: PathNode) -> Result<(), CoreError> {
    let path = path_mut(tx, at.shape)?;
    if at.subpath == path.subpaths.len() {
        if at.index != 0 {
            return Err(CoreError::IndexOutOfRange {
                index: at.index,
                len: 0,
            });
        }
        path.subpaths.push(SubPath {
            nodes: vec![node],
            closed: false,
        });
        return Ok(());
    }
    let sp = subpath_mut(path, at.subpath)?;
    if at.index > sp.nodes.len() {
        return Err(CoreError::IndexOutOfRange {
            index: at.index,
            len: sp.nodes.len(),
        });
    }
    sp.nodes.insert(at.index, node);
    Ok(())
}

pub(super) fn move_nodes(
    tx: &mut Tx<'_>,
    nodes: &[PathNodeRef],
    delta: Vec2,
) -> Result<(), CoreError> {
    let unique: BTreeSet<_> = nodes.iter().copied().collect();
    for r in unique {
        let n = node_mut(tx, r)?;
        n.anchor = n.anchor.offset(delta);
    }
    Ok(())
}

pub(super) fn set_handles(
    tx: &mut Tx<'_>,
    r: PathNodeRef,
    handle_in: Option<Vec2>,
    handle_out: Option<Vec2>,
) -> Result<(), CoreError> {
    let n = node_mut(tx, r)?;
    let keep = if n.handle_out == handle_out && n.handle_in != handle_in {
        Side::In
    } else {
        Side::Out
    };
    n.handle_in = handle_in;
    n.handle_out = handle_out;
    n.enforce(keep);
    Ok(())
}

pub(super) fn set_node_type(
    tx: &mut Tx<'_>,
    r: PathNodeRef,
    kind: NodeType,
) -> Result<(), CoreError> {
    let n = node_mut(tx, r)?;
    n.kind = kind;
    n.enforce_balanced();
    Ok(())
}

pub(super) fn toggle_segment(
    tx: &mut Tx<'_>,
    shape: NodeId,
    subpath: usize,
    index: usize,
) -> Result<(), CoreError> {
    let sp = subpath_mut(path_mut(tx, shape)?, subpath)?;
    let count = sp.segment_count();
    if index >= count {
        return Err(CoreError::IndexOutOfRange { index, len: count });
    }
    let next = (index + 1) % sp.nodes.len();
    let (a, b) = (sp.nodes[index].anchor, sp.nodes[next].anchor);
    if sp.nodes[index].handle_out.is_none() && sp.nodes[next].handle_in.is_none() {
        sp.nodes[index].handle_out = Some((b - a).scale(1.0 / 3.0));
        sp.nodes[next].handle_in = Some((a - b).scale(1.0 / 3.0));
        sp.nodes[index].enforce(Side::Out);
        sp.nodes[next].enforce(Side::In);
    } else {
        sp.nodes[index].handle_out = None;
        sp.nodes[next].handle_in = None;
    }
    Ok(())
}

pub(super) fn delete_nodes(tx: &mut Tx<'_>, nodes: &[PathNodeRef]) -> Result<(), CoreError> {
    let mut by_shape: BTreeMap<NodeId, BTreeSet<(usize, usize)>> = BTreeMap::new();
    for r in nodes {
        node_mut(tx, *r)?; // existence check
        by_shape
            .entry(r.shape)
            .or_default()
            .insert((r.subpath, r.index));
    }
    for (shape, refs) in by_shape {
        let path = path_mut(tx, shape)?;
        for &(s, i) in refs.iter().rev() {
            path.subpaths[s].nodes.remove(i);
        }
        path.subpaths.retain(|sp| !sp.nodes.is_empty());
        for sp in &mut path.subpaths {
            if sp.nodes.len() < 2 {
                sp.closed = false;
            }
        }
        if path.subpaths.is_empty() {
            structure::delete(tx, &[shape])?;
        }
    }
    Ok(())
}

pub(super) fn close(tx: &mut Tx<'_>, shape: NodeId, subpath: usize) -> Result<(), CoreError> {
    let sp = subpath_mut(path_mut(tx, shape)?, subpath)?;
    ensure_open(sp)?;
    if sp.nodes.len() < 2 {
        return Err(CoreError::InvalidCommand(
            "closing needs at least two nodes",
        ));
    }
    sp.closed = true;
    Ok(())
}

pub(super) fn open_at(tx: &mut Tx<'_>, r: PathNodeRef) -> Result<(), CoreError> {
    node_mut(tx, r)?; // existence check
    let sp = subpath_mut(path_mut(tx, r.shape)?, r.subpath)?;
    if !sp.closed {
        return Err(CoreError::InvalidCommand("the subpath is already open"));
    }
    sp.nodes.rotate_left(r.index);
    let mut end = sp.nodes[0];
    end.handle_out = None;
    sp.nodes[0].handle_in = None;
    sp.nodes.push(end);
    sp.closed = false;
    Ok(())
}

pub(super) fn split_at(tx: &mut Tx<'_>, r: PathNodeRef) -> Result<(), CoreError> {
    node_mut(tx, r)?; // existence check
    let path = path_mut(tx, r.shape)?;
    let sp = subpath_mut(path, r.subpath)?;
    ensure_open(sp)?;
    if r.index == 0 || r.index + 1 >= sp.nodes.len() {
        return Err(CoreError::InvalidCommand("split needs an interior node"));
    }
    let mut tail = sp.nodes.split_off(r.index);
    let mut end = tail[0];
    end.handle_out = None;
    sp.nodes.push(end);
    tail[0].handle_in = None;
    path.subpaths.insert(
        r.subpath + 1,
        SubPath {
            nodes: tail,
            closed: false,
        },
    );
    Ok(())
}

pub(super) fn reverse(tx: &mut Tx<'_>, shape: NodeId, subpath: usize) -> Result<(), CoreError> {
    subpath_mut(path_mut(tx, shape)?, subpath)?.reverse();
    Ok(())
}

pub(super) fn join(tx: &mut Tx<'_>, a: PathNodeRef, b: PathNodeRef) -> Result<(), CoreError> {
    if a.shape != b.shape {
        return Err(CoreError::InvalidCommand(
            "can only join endpoints of one shape",
        ));
    }
    for r in [a, b] {
        node_mut(tx, r)?; // existence check
    }
    let path = path_mut(tx, a.shape)?;
    for r in [a, b] {
        let sp = &path.subpaths[r.subpath];
        ensure_open(sp)?;
        if r.index != 0 && r.index + 1 != sp.nodes.len() {
            return Err(CoreError::InvalidCommand("can only join endpoints"));
        }
    }
    if a.subpath == b.subpath {
        if a.index == b.index {
            return Err(CoreError::InvalidCommand("pick two different endpoints"));
        }
        let sp = &mut path.subpaths[a.subpath];
        let last = sp.nodes.len() - 1;
        if sp.nodes.len() > 2 && coincide(&sp.nodes[0], &sp.nodes[last]) {
            let end = sp.nodes.pop().expect("len > 2");
            sp.nodes[0].handle_in = end.handle_in;
            sp.nodes[0].enforce_balanced();
        }
        sp.closed = true;
        return Ok(());
    }
    let mut first = path.subpaths[a.subpath].clone();
    if a.index == 0 {
        first.reverse(); // `a` becomes the last node
    }
    let mut second = path.subpaths[b.subpath].clone();
    if b.index != 0 {
        second.reverse(); // `b` becomes the first node
    }
    let tail = first.nodes.last_mut().expect("subpaths are non-empty");
    if coincide(tail, &second.nodes[0]) {
        let head = second.nodes.remove(0);
        tail.handle_out = head.handle_out;
        tail.enforce_balanced();
    }
    first.nodes.append(&mut second.nodes);
    path.subpaths[a.subpath] = first;
    path.subpaths.remove(b.subpath);
    Ok(())
}

fn coincide(p: &PathNode, q: &PathNode) -> bool {
    (p.anchor - q.anchor).length() <= EPS
}

pub(super) fn convert_to_path(
    tx: &mut Tx<'_>,
    items: Vec<(NodeId, Path)>,
) -> Result<(), CoreError> {
    for (id, path) in items {
        match &mut tx.node_mut(id)?.kind {
            NodeKind::Shape(s) => s.geometry = Geometry::Path(path),
            _ => return Err(CoreError::WrongKind(id, "shape")),
        }
    }
    Ok(())
}
