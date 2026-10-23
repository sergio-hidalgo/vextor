//! Running commands: [`Document::apply`].
//!
//! Every mutation goes through [`Tx`], which snapshots each part the first
//! time it is touched. The snapshot is the undo patch; on failure it is
//! written back so the document is unchanged.

mod paths;
mod structure;
mod text;

use crate::change::{Change, Patch};
use crate::color::ColorFormat;
use crate::command::Command;
use crate::document::{Artboard, Document};
use crate::editor::Selection;
use crate::error::CoreError;
use crate::ids::{NodeId, ResourceId};
use crate::node::Node;
use crate::resource::Resource;
use crate::settings::GridSettings;

impl Document {
    /// Applies one command and returns its undo record.
    ///
    /// The command is all-or-nothing: on error the document is unchanged.
    /// After the edit the whole document is validated
    /// ([`Document::validate`]), so a command can never leave it invalid.
    ///
    /// # Errors
    /// [`CoreError`] describing why the command does not fit this document.
    pub fn apply(&mut self, cmd: Command) -> Result<Change, CoreError> {
        let label = cmd.label();
        let mut tx = Tx {
            doc: self,
            before: Patch::default(),
        };
        let ran = run(&mut tx, cmd);
        let before = tx.before;
        match ran.and_then(|()| self.validate().map_err(CoreError::Invalid)) {
            Ok(()) => {
                let after = self.snapshot(&before);
                Ok(Change {
                    label,
                    before,
                    after,
                    selection_before: Selection::None,
                })
            }
            Err(e) => {
                self.apply_patch(&before);
                Err(e)
            }
        }
    }

    /// The current state of every part `targets` covers.
    #[must_use]
    pub fn snapshot(&self, targets: &Patch) -> Patch {
        Patch {
            nodes: targets
                .nodes
                .keys()
                .map(|id| (*id, self.nodes.get(id).cloned()))
                .collect(),
            resources: targets
                .resources
                .keys()
                .map(|id| (*id, self.resources.get(id).cloned()))
                .collect(),
            layers: targets.layers.as_ref().map(|_| self.layers.clone()),
            artboards: targets.artboards.as_ref().map(|_| self.artboards.clone()),
            grid: targets.grid.as_ref().map(|_| self.grid.clone()),
            color_format: targets.color_format.map(|_| self.color_format),
            name: targets.name.as_ref().map(|_| self.name.clone()),
        }
    }
}

fn run(tx: &mut Tx<'_>, cmd: Command) -> Result<(), CoreError> {
    use Command as C;
    match cmd {
        C::Insert {
            parent,
            index,
            nodes,
        } => structure::insert(tx, parent, index, nodes),
        C::Delete { ids } => structure::delete(tx, &ids),
        C::Reparent {
            ids,
            new_parent,
            index,
        } => structure::reparent(tx, &ids, new_parent, index),
        C::Group { ids } => structure::group(tx, &ids),
        C::Ungroup { id } => structure::ungroup(tx, id),
        C::MakeClip { ids } => structure::make_clip(tx, &ids),
        C::ReleaseClip { id } => structure::release_clip(tx, id),
        C::SetTransform { ids, transform } => structure::set_transform(tx, &ids, transform),
        C::SetTransforms { items } => structure::set_transforms(tx, items),
        C::SetStyle { ids, style } => structure::set_style(tx, &ids, &style),
        C::SetGeometry { items } => structure::set_geometry(tx, items),
        cmd @ (C::SetText { .. } | C::SetTextStyle { .. } | C::SetTextFrame { .. }) => {
            text::run(tx, cmd)
        }
        C::SetProps {
            id,
            name,
            visible,
            locked,
            opacity,
        } => {
            let node = tx.node_mut(id)?;
            node.name = name;
            node.visible = visible;
            node.locked = locked;
            node.opacity = opacity;
            Ok(())
        }
        C::AddPathNode { at, node } => paths::add_node(tx, at, node),
        C::MovePathNodes { nodes, delta } => paths::move_nodes(tx, &nodes, delta),
        C::SetHandles {
            node,
            handle_in,
            handle_out,
        } => paths::set_handles(tx, node, handle_in, handle_out),
        C::SetNodeType { node, kind } => paths::set_node_type(tx, node, kind),
        C::ToggleSegment {
            shape,
            subpath,
            index,
        } => paths::toggle_segment(tx, shape, subpath, index),
        C::DeletePathNodes { nodes } => paths::delete_nodes(tx, &nodes),
        C::CloseSubPath { shape, subpath } => paths::close(tx, shape, subpath),
        C::OpenAt { node } => paths::open_at(tx, node),
        C::JoinEndpoints { a, b } => paths::join(tx, a, b),
        C::SplitAt { node } => paths::split_at(tx, node),
        C::Reverse { shape, subpath } => paths::reverse(tx, shape, subpath),
        C::ConvertToPath { items } => paths::convert_to_path(tx, items),
        C::ReplaceWith {
            removed,
            inserted,
            parent,
            index,
        } => {
            structure::delete(tx, &removed)?;
            structure::insert(tx, parent, index, inserted)
        }
        C::PutResource {
            id,
            resource,
            nodes,
        } => structure::put_resource(tx, id, resource, nodes),
        C::RemoveResource { id } => structure::remove_resource(tx, id),
        C::SetArtboards(artboards) => {
            for a in &artboards {
                tx.doc.reserve(a.id.0);
            }
            *tx.artboards_mut() = artboards;
            Ok(())
        }
        C::SetGrid(grid) => {
            *tx.grid_mut() = grid;
            Ok(())
        }
        C::Batch { commands, .. } => {
            // One transaction: the patch covers every step and validation
            // runs once at the end, so steps may pass through states that
            // are only valid together.
            for cmd in commands {
                run(tx, cmd)?;
            }
            Ok(())
        }
        C::SetName(name) => set_name(tx, name),
        C::SetColorFormat(format) => {
            set_color_format(tx, format);
            Ok(())
        }
    }
}

/// [`Command::SetColorFormat`].
fn set_color_format(tx: &mut Tx<'_>, format: ColorFormat) {
    if tx.before.color_format.is_none() {
        tx.before.color_format = Some(tx.doc.color_format);
    }
    tx.doc.color_format = format;
}

/// [`Command::SetName`].
fn set_name(tx: &mut Tx<'_>, name: String) -> Result<(), CoreError> {
    if name.trim().is_empty() {
        return Err(CoreError::InvalidCommand("a document name cannot be empty"));
    }
    if tx.before.name.is_none() {
        tx.before.name = Some(tx.doc.name.clone());
    }
    tx.doc.name = name;
    Ok(())
}

/// A document being edited by one command, recording the undo patch.
pub(crate) struct Tx<'a> {
    pub(crate) doc: &'a mut Document,
    before: Patch,
}

impl Tx<'_> {
    fn touch_node(&mut self, id: NodeId) {
        if !self.before.nodes.contains_key(&id) {
            self.before
                .nodes
                .insert(id, self.doc.nodes.get(&id).cloned());
        }
    }

    fn touch_resource(&mut self, id: ResourceId) {
        if !self.before.resources.contains_key(&id) {
            self.before
                .resources
                .insert(id, self.doc.resources.get(&id).cloned());
        }
    }

    /// Read access to a node.
    pub(crate) fn node(&self, id: NodeId) -> Result<&Node, CoreError> {
        self.doc.node(id)
    }

    /// Write access to an existing node.
    pub(crate) fn node_mut(&mut self, id: NodeId) -> Result<&mut Node, CoreError> {
        if !self.doc.nodes.contains_key(&id) {
            return Err(CoreError::NodeNotFound(id));
        }
        self.touch_node(id);
        Ok(self
            .doc
            .nodes
            .get_mut(&id)
            .expect("node exists: checked above"))
    }

    /// Adds a node whose id is not in use.
    pub(crate) fn add_node(&mut self, node: Node) -> Result<(), CoreError> {
        if self.doc.nodes.contains_key(&node.id) {
            return Err(CoreError::DuplicateId(node.id.0));
        }
        self.touch_node(node.id);
        self.doc.reserve(node.id.0);
        self.doc.nodes.insert(node.id, node);
        Ok(())
    }

    /// Removes a node from the map (not from its parent's list).
    pub(crate) fn remove_node(&mut self, id: NodeId) -> Result<Node, CoreError> {
        self.touch_node(id);
        self.doc
            .nodes
            .remove(&id)
            .ok_or(CoreError::NodeNotFound(id))
    }

    pub(crate) fn layers_mut(&mut self) -> &mut Vec<NodeId> {
        if self.before.layers.is_none() {
            self.before.layers = Some(self.doc.layers.clone());
        }
        &mut self.doc.layers
    }

    pub(crate) fn put_resource(&mut self, id: ResourceId, resource: Resource) {
        self.touch_resource(id);
        self.doc.reserve(id.0);
        self.doc.resources.insert(id, resource);
    }

    pub(crate) fn remove_resource(&mut self, id: ResourceId) -> Result<Resource, CoreError> {
        self.touch_resource(id);
        self.doc
            .resources
            .remove(&id)
            .ok_or(CoreError::ResourceNotFound(id))
    }

    fn artboards_mut(&mut self) -> &mut Vec<Artboard> {
        if self.before.artboards.is_none() {
            self.before.artboards = Some(self.doc.artboards.clone());
        }
        &mut self.doc.artboards
    }

    fn grid_mut(&mut self) -> &mut GridSettings {
        if self.before.grid.is_none() {
            self.before.grid = Some(self.doc.grid.clone());
        }
        &mut self.doc.grid
    }
}
