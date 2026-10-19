//! Structure, property and resource commands.

use std::collections::BTreeSet;

use super::Tx;
use crate::document::Parent;
use crate::error::CoreError;
use crate::ids::{NodeId, ResourceId};
use crate::math::Transform;
use crate::node::{Container, Geometry, Node, NodeKind};
use crate::resource::Resource;
use crate::style::Style;

/// Puts `ids` (already in the node map, without a parent) into `parent`'s
/// list at `index`. `None` = the top-level layer list.
pub(super) fn attach(
    tx: &mut Tx<'_>,
    parent: Option<NodeId>,
    index: usize,
    ids: &[NodeId],
) -> Result<(), CoreError> {
    for &id in ids {
        let is_layer = tx.node(id)?.is_layer();
        if is_layer && parent.is_some() {
            return Err(CoreError::InvalidCommand(
                "layers can only be at the top level",
            ));
        }
        if !is_layer && parent.is_none() {
            return Err(CoreError::InvalidCommand(
                "only layers can be at the top level",
            ));
        }
    }
    let list = match parent {
        None => tx.layers_mut(),
        Some(p) => {
            &mut tx
                .node_mut(p)?
                .container_mut()
                .ok_or(CoreError::WrongKind(p, "layer or group"))?
                .children
        }
    };
    if index > list.len() {
        return Err(CoreError::IndexOutOfRange {
            index,
            len: list.len(),
        });
    }
    list.splice(index..index, ids.iter().copied());
    Ok(())
}

/// Takes `id` out of its parent's list (clearing a clip that pointed at
/// it) and returns where it was.
pub(super) fn detach(tx: &mut Tx<'_>, id: NodeId) -> Result<(Option<NodeId>, usize), CoreError> {
    match tx.doc.parent_of(id) {
        Some(Parent::Root) => {
            let layers = tx.layers_mut();
            let index = layers
                .iter()
                .position(|&l| l == id)
                .expect("listed: parent_of found it");
            layers.remove(index);
            Ok((None, index))
        }
        Some(Parent::Node(p)) => {
            let c = tx
                .node_mut(p)?
                .container_mut()
                .expect("parent_of returns containers");
            let index = c
                .children
                .iter()
                .position(|&c| c == id)
                .expect("listed: parent_of found it");
            c.children.remove(index);
            if c.clip == Some(id) {
                c.clip = None;
            }
            Ok((Some(p), index))
        }
        Some(Parent::Pattern(_)) => Err(CoreError::InvalidCommand(
            "pattern content is edited with PutResource",
        )),
        None => Err(CoreError::NodeNotFound(id)),
    }
}

fn ensure_unique(ids: &[NodeId]) -> Result<(), CoreError> {
    let set: BTreeSet<_> = ids.iter().collect();
    if set.len() == ids.len() {
        Ok(())
    } else {
        Err(CoreError::InvalidCommand("an id is listed twice"))
    }
}

pub(super) fn insert(
    tx: &mut Tx<'_>,
    parent: Option<NodeId>,
    index: usize,
    nodes: Vec<Node>,
) -> Result<(), CoreError> {
    if nodes.is_empty() {
        return Err(CoreError::InvalidCommand("nothing to insert"));
    }
    let referenced: BTreeSet<NodeId> = nodes
        .iter()
        .filter_map(Node::container)
        .flat_map(|c| c.children.iter().copied())
        .collect();
    let roots: Vec<NodeId> = nodes
        .iter()
        .map(|n| n.id)
        .filter(|id| !referenced.contains(id))
        .collect();
    if roots.is_empty() {
        return Err(CoreError::InvalidCommand("inserted nodes form a cycle"));
    }
    for node in nodes {
        tx.add_node(node)?;
    }
    attach(tx, parent, index, &roots)
}

pub(super) fn delete(tx: &mut Tx<'_>, ids: &[NodeId]) -> Result<(), CoreError> {
    for &id in ids {
        if !tx.doc.nodes.contains_key(&id) {
            if tx.before.nodes.contains_key(&id) {
                continue; // already removed with an ancestor listed earlier
            }
            return Err(CoreError::NodeNotFound(id));
        }
        detach(tx, id)?;
        for sub in tx.doc.subtree(id) {
            tx.remove_node(sub)?;
        }
    }
    Ok(())
}

pub(super) fn reparent(
    tx: &mut Tx<'_>,
    ids: &[NodeId],
    new_parent: Option<NodeId>,
    index: usize,
) -> Result<(), CoreError> {
    ensure_unique(ids)?;
    for &id in ids {
        tx.node(id)?;
        if new_parent.is_some_and(|p| tx.doc.subtree(id).contains(&p)) {
            return Err(CoreError::InvalidCommand("cannot move a node into itself"));
        }
    }
    let mut clips = Vec::new();
    for &id in ids {
        let (old_parent, _) = detach(tx, id)?;
        if let Some(p) = old_parent
            && tx
                .before
                .nodes
                .get(&p)
                .and_then(|n| n.as_ref()?.container()?.clip)
                == Some(id)
        {
            clips.push((p, id));
        }
    }
    attach(tx, new_parent, index, ids)?;
    // A clip moved within its own container (a reorder) stays its clip.
    for (p, id) in clips {
        if new_parent == Some(p) {
            tx.node_mut(p)?
                .container_mut()
                .expect("held children: is a container")
                .clip = Some(id);
        }
    }
    Ok(())
}

pub(super) fn group(tx: &mut Tx<'_>, ids: &[NodeId]) -> Result<(), CoreError> {
    ensure_unique(ids)?;
    let first = *ids
        .first()
        .ok_or(CoreError::InvalidCommand("nothing to group"))?;
    let parent = match tx.doc.parent_of(first) {
        Some(Parent::Node(p)) => p,
        Some(Parent::Root) => return Err(CoreError::InvalidCommand("layers cannot be grouped")),
        Some(Parent::Pattern(_)) => {
            return Err(CoreError::InvalidCommand(
                "pattern content is edited with PutResource",
            ));
        }
        None => return Err(CoreError::NodeNotFound(first)),
    };
    let container = tx
        .node(parent)?
        .container()
        .expect("parent_of returns containers");
    if container.clip.is_some_and(|clip| ids.contains(&clip)) {
        return Err(CoreError::InvalidCommand("cannot group a clipping shape"));
    }
    let mut positions = Vec::with_capacity(ids.len());
    for &id in ids {
        let pos = container.children.iter().position(|&c| c == id);
        positions.push((
            pos.ok_or(CoreError::InvalidCommand("can only group siblings"))?,
            id,
        ));
    }
    positions.sort_unstable();
    let index = positions.last().expect("non-empty: first exists").0 + 1 - positions.len();
    let children: Vec<NodeId> = positions.iter().map(|&(_, id)| id).collect();

    let gid = tx.doc.alloc_node_id();
    tx.add_node(Node::new(
        gid,
        NodeKind::Group(Container {
            children: children.clone(),
            clip: None,
        }),
    ))?;
    let list = &mut tx
        .node_mut(parent)?
        .container_mut()
        .expect("checked above")
        .children;
    list.retain(|c| !children.contains(c));
    list.insert(index, gid);
    Ok(())
}

pub(super) fn ungroup(tx: &mut Tx<'_>, id: NodeId) -> Result<(), CoreError> {
    let group = tx.node(id)?.clone();
    let NodeKind::Group(c) = &group.kind else {
        return Err(CoreError::WrongKind(id, "group"));
    };
    if c.clip.is_some() {
        return Err(CoreError::InvalidCommand(
            "release the clip before ungrouping",
        ));
    }
    let (parent, index) = detach(tx, id)?;
    tx.remove_node(id)?;
    for &child in &c.children {
        let n = tx.node_mut(child)?;
        n.transform = n.transform.then(group.transform);
        n.opacity *= group.opacity;
        n.visible &= group.visible;
        n.locked |= group.locked;
    }
    attach(tx, parent, index, &c.children)
}

pub(super) fn set_transform(
    tx: &mut Tx<'_>,
    ids: &[NodeId],
    transform: Transform,
) -> Result<(), CoreError> {
    for &id in ids {
        tx.node_mut(id)?.transform = transform;
    }
    Ok(())
}

pub(super) fn set_transforms(
    tx: &mut Tx<'_>,
    items: Vec<(NodeId, Transform)>,
) -> Result<(), CoreError> {
    for (id, transform) in items {
        tx.node_mut(id)?.transform = transform;
    }
    Ok(())
}

pub(super) fn set_geometry(
    tx: &mut Tx<'_>,
    items: Vec<(NodeId, Geometry)>,
) -> Result<(), CoreError> {
    for (id, geometry) in items {
        match &mut tx.node_mut(id)?.kind {
            NodeKind::Shape(s) => s.geometry = geometry,
            _ => return Err(CoreError::WrongKind(id, "shape")),
        }
    }
    Ok(())
}

pub(super) fn set_style(tx: &mut Tx<'_>, ids: &[NodeId], style: &Style) -> Result<(), CoreError> {
    for &id in ids {
        match &mut tx.node_mut(id)?.kind {
            NodeKind::Shape(s) => s.style = style.clone(),
            _ => return Err(CoreError::WrongKind(id, "shape")),
        }
    }
    Ok(())
}

pub(super) fn put_resource(
    tx: &mut Tx<'_>,
    id: ResourceId,
    resource: Resource,
    nodes: Vec<Node>,
) -> Result<(), CoreError> {
    let old_content = tx
        .doc
        .resources
        .get(&id)
        .map(|r| r.content().to_vec())
        .unwrap_or_default();
    for node in nodes {
        tx.add_node(node)?;
    }
    for root in old_content {
        if !resource.content().contains(&root) {
            for sub in tx.doc.subtree(root) {
                tx.remove_node(sub)?;
            }
        }
    }
    tx.put_resource(id, resource);
    Ok(())
}

pub(super) fn remove_resource(tx: &mut Tx<'_>, id: ResourceId) -> Result<(), CoreError> {
    if tx.doc.resource_in_use(id) {
        return Err(CoreError::ResourceInUse(id));
    }
    let resource = tx.remove_resource(id)?;
    for &root in resource.content() {
        for sub in tx.doc.subtree(root) {
            tx.remove_node(sub)?;
        }
    }
    Ok(())
}
