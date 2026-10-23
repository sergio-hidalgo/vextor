//! Layer and tree edits for the Layers panel (spec 10): new, duplicate
//! and delete a layer, and moving rows to another place in the tree.

use crate::command::Command;
use crate::document::{Document, Parent};
use crate::error::CoreError;
use crate::ids::NodeId;
use crate::math::Transform;
use crate::node::{Container, MAX_NODE_NAME_CHARS, Node, NodeKind};

impl Document {
    /// The command that adds an empty layer named "Layer N" (the first
    /// number no layer uses) above `above`, else at the top; and its id.
    pub fn new_layer(&mut self, above: Option<NodeId>) -> (Command, NodeId) {
        let index = above
            .and_then(|id| self.layers.iter().position(|&l| l == id))
            .map_or(self.layers.len(), |i| i + 1);
        let taken = |name: &str| {
            self.layers
                .iter()
                .any(|l| self.nodes.get(l).and_then(|n| n.name.as_deref()) == Some(name))
        };
        // With n layers, one of the n + 1 numbers from n + 1 on is free.
        let first = self.layers.len() + 1;
        let name = (first..=2 * first)
            .map(|n| format!("Layer {n}"))
            .find(|name| !taken(name))
            .unwrap_or_else(|| format!("Layer {first}"));
        let id = self.alloc_node_id();
        let mut layer = Node::new(id, NodeKind::Layer(Container::default()));
        layer.name = Some(name);
        let cmd = Command::Batch {
            label: "New layer",
            commands: vec![Command::Insert {
                parent: None,
                index,
                nodes: vec![layer],
            }],
        };
        (cmd, id)
    }

    /// The command that copies layer `id` with everything in it, placed
    /// just above it and named "… copy"; and the copy's id.
    ///
    /// # Errors
    /// [`CoreError::WrongKind`] when `id` is not a layer.
    pub fn duplicate_layer(&mut self, id: NodeId) -> Result<(Command, NodeId), CoreError> {
        let Some(index) = self.layers.iter().position(|&l| l == id) else {
            return Err(CoreError::WrongKind(id, "layer"));
        };
        let nodes = self.copy_subtrees(&[id])?;
        let (mut nodes, map) = self.fresh_copies_mapped(&nodes);
        let copy = map[&id];
        if let Some(layer) = nodes.iter_mut().find(|n| n.id == copy) {
            let base = layer.name.as_deref().unwrap_or("Layer");
            let name: String = format!("{base} copy");
            layer.name = Some(name.chars().take(MAX_NODE_NAME_CHARS).collect());
        }
        let cmd = Command::Batch {
            label: "Duplicate layer",
            commands: vec![Command::Insert {
                parent: None,
                index: index + 1,
                nodes,
            }],
        };
        Ok((cmd, copy))
    }

    /// The command that deletes layer `id` with its contents.
    ///
    /// # Errors
    /// [`CoreError::WrongKind`] when `id` is not a layer;
    /// [`CoreError::InvalidCommand`] for the last layer, which stays.
    pub fn delete_layer(&self, id: NodeId) -> Result<Command, CoreError> {
        if !self.layers.contains(&id) {
            return Err(CoreError::WrongKind(id, "layer"));
        }
        if self.layers.len() == 1 {
            return Err(CoreError::InvalidCommand(
                "the last layer cannot be deleted",
            ));
        }
        Ok(Command::Batch {
            label: "Delete layer",
            commands: vec![Command::Delete { ids: vec![id] }],
        })
    }

    /// The command that moves `ids` into `parent` (`None`: the layer list)
    /// before the child now at `before` (its length: on top). Objects keep
    /// their place on the canvas: their transforms are rebased onto the
    /// new parent.
    ///
    /// # Errors
    /// [`CoreError::NodeNotFound`] for a missing node or parent;
    /// [`CoreError::InvalidCommand`] when nothing is moved or the new
    /// parent's transform cannot be inverted. The move itself is checked
    /// when applied (layers only at the top level, never into itself).
    pub fn move_to(
        &self,
        ids: &[NodeId],
        parent: Option<NodeId>,
        before: usize,
    ) -> Result<Command, CoreError> {
        if ids.is_empty() {
            return Err(CoreError::InvalidCommand("nothing to move"));
        }
        for &id in ids {
            self.node(id)?;
        }
        let ids = self.sorted_by_paint_order(ids);
        let siblings = match parent {
            None => &self.layers,
            Some(p) => {
                &self
                    .node(p)?
                    .container()
                    .ok_or(CoreError::WrongKind(p, "layer or group"))?
                    .children
            }
        };
        // `Reparent` counts its index after the moved nodes are taken out.
        let index = before
            - siblings[..before.min(siblings.len())]
                .iter()
                .filter(|id| ids.contains(id))
                .count();
        let mut commands = vec![Command::Reparent {
            ids: ids.clone(),
            new_parent: parent,
            index,
        }];
        if let Some(p) = parent {
            let to_parent = self
                .world_transform(p)
                .invert()
                .ok_or(CoreError::InvalidCommand("the target cannot hold content"))?;
            let items: Vec<(NodeId, Transform)> = ids
                .iter()
                .filter(|&&id| self.parent_of(id) != Some(Parent::Node(p)))
                .map(|&id| (id, self.world_transform(id).then(to_parent)))
                .collect();
            if !items.is_empty() {
                commands.push(Command::SetTransforms { items });
            }
        }
        let label = if parent.is_none() {
            "Move layer"
        } else {
            "Move in layers"
        };
        Ok(Command::Batch { label, commands })
    }
}
