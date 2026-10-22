//! Edit operations that build [`Command`]s from the current document:
//! copy / paste, stacking order, and the world transforms they rely on.

use std::collections::{BTreeMap, BTreeSet};

use crate::command::Command;
use crate::document::{Document, Parent};
use crate::editor::{ClipResource, ClipboardPayload};
use crate::error::CoreError;
use crate::ids::{NodeId, ResourceId};
use crate::math::Transform;
use crate::node::Node;
use crate::resource::Resource;
use crate::style::Paint;

/// A change of stacking order among siblings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrange {
    /// Up over the nearest unselected sibling above that overlaps, else
    /// one step (spec 08c).
    Forward,
    /// Down under the nearest unselected sibling below that overlaps, else
    /// one step.
    Backward,
    /// Above all siblings.
    Front,
    /// Below all siblings.
    Back,
}

impl Document {
    /// The transform from `id`'s own space to document space: its
    /// transform, then every ancestor's. Identity for a missing node.
    #[must_use]
    pub fn world_transform(&self, id: NodeId) -> Transform {
        let own = self
            .nodes
            .get(&id)
            .map_or(Transform::IDENTITY, |n| n.transform);
        own.then(self.parent_transform(id))
    }

    /// The transform from the space `id` is placed in (its parent's) to
    /// document space. Identity for a layer or pattern content.
    #[must_use]
    pub fn parent_transform(&self, id: NodeId) -> Transform {
        match self.parent_of(id) {
            Some(Parent::Node(p)) => self.world_transform(p),
            _ => Transform::IDENTITY,
        }
    }

    /// Every node held by a layer, in paint order: bottom layer first,
    /// each container before its children, children bottom → top.
    #[must_use]
    pub fn paint_order(&self) -> Vec<NodeId> {
        self.layers.iter().flat_map(|&l| self.subtree(l)).collect()
    }

    /// `ids` sorted bottom → top (paint order); unknown ids are dropped.
    #[must_use]
    pub fn sorted_by_paint_order(&self, ids: &[NodeId]) -> Vec<NodeId> {
        let wanted: BTreeSet<NodeId> = ids.iter().copied().collect();
        self.paint_order()
            .into_iter()
            .filter(|id| wanted.contains(id))
            .collect()
    }

    /// Copies `roots` (listed bottom → top) with their descendants and the
    /// resources they paint with. Each root's transform is made world-space.
    ///
    /// # Errors
    /// [`CoreError::NodeNotFound`] when a root does not exist.
    pub fn copy(&self, roots: &[NodeId]) -> Result<ClipboardPayload, CoreError> {
        let mut nodes = self.copy_subtrees(roots)?;
        for node in &mut nodes {
            if roots.contains(&node.id) {
                node.transform = node.transform.then(self.parent_transform(node.id));
            }
        }
        // Resources used by the nodes, and by pattern content, transitively.
        let mut used: BTreeSet<ResourceId> = nodes.iter().flat_map(paint_refs).collect();
        let mut resources = Vec::new();
        let mut done = BTreeSet::new();
        while let Some(id) = used.iter().copied().find(|id| !done.contains(id)) {
            done.insert(id);
            let resource = self
                .resources
                .get(&id)
                .ok_or(CoreError::ResourceNotFound(id))?;
            let content = self.copy_subtrees(resource.content())?;
            used.extend(content.iter().flat_map(paint_refs));
            resources.push(ClipResource {
                id,
                resource: resource.clone(),
                content,
            });
        }
        Ok(ClipboardPayload { nodes, resources })
    }

    /// The command that pastes `payload` into `parent` at `index`, moved by
    /// `offset` (in document space), as one undo step named `label`; and
    /// the new ids of the pasted roots, bottom → top.
    ///
    /// Every node gets a fresh id. A resource is reused when this document
    /// holds an identical one under the same id; otherwise it is added
    /// under a fresh id and the pasted paints point to it.
    ///
    /// # Errors
    /// [`CoreError::NodeNotFound`] for a missing `parent`;
    /// [`CoreError::InvalidCommand`] when the payload is empty or `parent`'s
    /// transform cannot be inverted.
    pub fn paste(
        &mut self,
        payload: &ClipboardPayload,
        parent: NodeId,
        index: usize,
        offset: Transform,
        label: &'static str,
    ) -> Result<(Command, Vec<NodeId>), CoreError> {
        self.node(parent)?;
        if payload.nodes.is_empty() {
            return Err(CoreError::InvalidCommand("nothing to paste"));
        }
        let to_parent = self
            .world_transform(parent)
            .invert()
            .ok_or(CoreError::InvalidCommand("the target cannot hold content"))?;

        let mut resource_map = BTreeMap::new();
        let mut added = Vec::new();
        for r in &payload.resources {
            if self.resources.get(&r.id) == Some(&r.resource) {
                resource_map.insert(r.id, r.id);
            } else {
                // A fresh id can equal the copied one (another document),
                // so "added" is tracked, not inferred from the ids.
                resource_map.insert(r.id, self.alloc_resource_id());
                added.push(r);
            }
        }
        let mut commands = Vec::new();
        for r in added {
            let (mut content, map) = self.fresh_copies_mapped(&r.content);
            for n in &mut content {
                remap_paints(n, &resource_map);
            }
            let mut resource = r.resource.clone();
            if let Resource::Pattern(p) = &mut resource {
                for id in &mut p.content {
                    *id = map.get(id).copied().unwrap_or(*id);
                }
            }
            commands.push(Command::PutResource {
                id: resource_map[&r.id],
                resource,
                nodes: content,
            });
        }

        let children: BTreeSet<NodeId> = payload
            .nodes
            .iter()
            .filter_map(Node::container)
            .flat_map(|c| c.children.iter().copied())
            .collect();
        let (mut nodes, map) = self.fresh_copies_mapped(&payload.nodes);
        let mut roots = Vec::new();
        for (node, old) in nodes.iter_mut().zip(&payload.nodes) {
            remap_paints(node, &resource_map);
            if !children.contains(&old.id) {
                node.transform = node.transform.then(offset).then(to_parent);
                roots.push(map[&old.id]);
            }
        }
        commands.push(Command::Insert {
            parent: Some(parent),
            index,
            nodes,
        });
        Ok((Command::Batch { label, commands }, roots))
    }

    /// The command that restacks `ids` among their siblings, or `None` when
    /// nothing would move. Siblings are handled per parent; layers and
    /// clipping shapes keep their place. `overlap(a, b)` says whether two
    /// siblings cover each other on screen: Forward and Backward pass over
    /// the ones that don't (the geometry lives in `vextor-geom`).
    #[must_use]
    pub fn arrange(
        &self,
        ids: &[NodeId],
        how: Arrange,
        overlap: impl Fn(NodeId, NodeId) -> bool,
    ) -> Option<Command> {
        let mut by_parent: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
        for &id in ids {
            if let Some(Parent::Node(p)) = self.parent_of(id) {
                by_parent.entry(p).or_default().insert(id);
            }
        }
        let mut commands = Vec::new();
        for (parent, selected) in by_parent {
            let Some(c) = self.nodes.get(&parent).and_then(Node::container) else {
                continue;
            };
            let moving = |id: &NodeId| selected.contains(id) && c.clip != Some(*id);
            let mut order = c.children.clone();
            match how {
                Arrange::Front => order.sort_by_key(|id| moving(id)),
                Arrange::Back => order.sort_by_key(|id| !moving(id)),
                Arrange::Forward => {
                    // Top-down, so a moved object is the limit of the one
                    // below it and the selection keeps its relative order.
                    for i in (0..order.len()).rev() {
                        if !moving(&order[i]) {
                            continue;
                        }
                        let limit = (i + 1..order.len())
                            .find(|&j| moving(&order[j]))
                            .unwrap_or(order.len());
                        if i + 1 == limit {
                            continue;
                        }
                        let to = (i + 1..limit)
                            .find(|&j| overlap(order[i], order[j]))
                            .unwrap_or(i + 1);
                        let id = order.remove(i);
                        order.insert(to, id);
                    }
                }
                Arrange::Backward => {
                    for i in 0..order.len() {
                        if !moving(&order[i]) {
                            continue;
                        }
                        let limit = (0..i)
                            .rev()
                            .find(|&j| moving(&order[j]))
                            .map_or(0, |j| j + 1);
                        if limit == i {
                            continue;
                        }
                        let to = (limit..i)
                            .rev()
                            .find(|&j| overlap(order[i], order[j]))
                            .unwrap_or(i - 1);
                        let id = order.remove(i);
                        order.insert(to, id);
                    }
                }
            }
            if order != c.children {
                // Re-listing every child in the new order (a clip stays a
                // clip when it stays in its container).
                commands.push(Command::Reparent {
                    ids: order,
                    new_parent: Some(parent),
                    index: 0,
                });
            }
        }
        if commands.is_empty() {
            return None;
        }
        let label = match how {
            Arrange::Forward => "Bring forward",
            Arrange::Backward => "Send backward",
            Arrange::Front => "Bring to front",
            Arrange::Back => "Send to back",
        };
        Some(Command::Batch { label, commands })
    }

    /// Whether [`Document::arrange`] would move anything: some selected
    /// object has an unselected sibling above it (Forward, Front) or below
    /// it (Backward, Back). Needs no geometry.
    #[must_use]
    pub fn can_arrange(&self, ids: &[NodeId], how: Arrange) -> bool {
        self.arrange(ids, how, |_, _| false).is_some()
    }
}

/// The resources a node's paints refer to.
fn paint_refs(node: &Node) -> Vec<ResourceId> {
    node.paints()
        .into_iter()
        .filter_map(|p| match p {
            Paint::Gradient(r) | Paint::Pattern(r) => Some(r),
            _ => None,
        })
        .collect()
}

/// Points a node's paints at the resources' new ids.
fn remap_paints(node: &mut Node, map: &BTreeMap<ResourceId, ResourceId>) {
    for p in node.paints_mut() {
        if let Paint::Gradient(r) | Paint::Pattern(r) = p {
            *r = map.get(r).copied().unwrap_or(*r);
        }
    }
}
