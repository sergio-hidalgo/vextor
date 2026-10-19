//! The document: everything saved in a `.vextor` file.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize, Serializer};

use crate::color::{ColorFormat, FormatScope, Rgba};
use crate::error::CoreError;
use crate::ids::{ArtboardId, NodeId, ResourceId};
use crate::math::Rect;
use crate::node::{Container, Node, NodeKind};
use crate::resource::Resource;
use crate::settings::{GridSettings, SvgExportOptions};
use crate::style::Swatch;

/// Current `.vextor` file format version.
pub const FORMAT_VERSION: u32 = 2;

/// A vextor document. Edited only through [`crate::Command`]s
/// ([`Document::apply`]) so every change is undoable.
///
/// Equality ignores the id allocator: undo restores content, not the
/// allocator, because ids are never reused.
#[derive(Clone, Debug, Deserialize)]
pub struct Document {
    /// `.vextor` file format, [`FORMAT_VERSION`].
    pub format_version: u32,
    pub name: String,
    /// How colours are written when saved; hex unless the user asks for
    /// rgba. Files without it load as hex.
    #[serde(default)]
    pub color_format: ColorFormat,
    /// Ordered export targets.
    pub artboards: Vec<Artboard>,
    /// Top-level layers, bottom → top.
    pub layers: Vec<NodeId>,
    /// Every layer, group, shape and image, including pattern content.
    pub nodes: BTreeMap<NodeId, Node>,
    pub resources: BTreeMap<ResourceId, Resource>,
    pub swatches: Vec<Swatch>,
    pub grid: GridSettings,
    pub export: SvgExportOptions,
    /// Next free id, shared by nodes, resources and artboards.
    pub(crate) next_id: u64,
}

/// A rectangle of the canvas exported as one SVG.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Artboard {
    pub id: ArtboardId,
    /// E.g. `"icon-24"`, `"button-primary"`.
    pub name: String,
    /// In document space.
    pub rect: Rect,
    /// `None` = transparent.
    pub background: Option<Rgba>,
}

/// Who holds a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parent {
    /// Listed in [`Document::layers`].
    Root,
    /// A child of this layer or group.
    Node(NodeId),
    /// Content of this pattern resource.
    Pattern(ResourceId),
}

impl Serialize for Document {
    /// Serialises every field, writing colours in [`Document::color_format`].
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Fields<'a> {
            format_version: u32,
            name: &'a str,
            color_format: ColorFormat,
            artboards: &'a [Artboard],
            layers: &'a [NodeId],
            nodes: &'a BTreeMap<NodeId, Node>,
            resources: &'a BTreeMap<ResourceId, Resource>,
            swatches: &'a [Swatch],
            grid: &'a GridSettings,
            export: &'a SvgExportOptions,
            next_id: u64,
        }
        let _scope = FormatScope::set(self.color_format);
        Fields {
            format_version: self.format_version,
            name: &self.name,
            color_format: self.color_format,
            artboards: &self.artboards,
            layers: &self.layers,
            nodes: &self.nodes,
            resources: &self.resources,
            swatches: &self.swatches,
            grid: &self.grid,
            export: &self.export,
            next_id: self.next_id,
        }
        .serialize(s)
    }
}

impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        self.format_version == other.format_version
            && self.name == other.name
            && self.color_format == other.color_format
            && self.artboards == other.artboards
            && self.layers == other.layers
            && self.nodes == other.nodes
            && self.resources == other.resources
            && self.swatches == other.swatches
            && self.grid == other.grid
            && self.export == other.export
    }
}

impl Document {
    /// A new document with one empty layer named "Layer 1", no artboard and
    /// default grid and export settings.
    #[must_use]
    pub fn new(name: &str) -> Self {
        let mut doc = Self::empty(name);
        let id = doc.alloc_node_id();
        let mut layer = Node::new(id, NodeKind::Layer(Container::default()));
        layer.name = Some("Layer 1".to_owned());
        doc.nodes.insert(id, layer);
        doc.layers.push(id);
        doc
    }

    fn empty(name: &str) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            name: name.to_owned(),
            color_format: ColorFormat::Hex,
            artboards: Vec::new(),
            layers: Vec::new(),
            nodes: BTreeMap::new(),
            resources: BTreeMap::new(),
            swatches: Vec::new(),
            grid: GridSettings::default(),
            export: SvgExportOptions::default(),
            next_id: 1,
        }
    }

    /// Reserves a fresh node id.
    pub fn alloc_node_id(&mut self) -> NodeId {
        NodeId(self.alloc())
    }

    /// Reserves a fresh resource id.
    pub fn alloc_resource_id(&mut self) -> ResourceId {
        ResourceId(self.alloc())
    }

    /// Reserves a fresh artboard id.
    pub fn alloc_artboard_id(&mut self) -> ArtboardId {
        ArtboardId(self.alloc())
    }

    fn alloc(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Makes sure `id` will never be handed out by the allocator.
    pub(crate) fn reserve(&mut self, id: u64) {
        self.next_id = self.next_id.max(id.saturating_add(1));
    }

    /// The next id the allocator will hand out.
    #[must_use]
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    /// The node with this id.
    ///
    /// # Errors
    /// [`CoreError::NodeNotFound`].
    pub fn node(&self, id: NodeId) -> Result<&Node, CoreError> {
        self.nodes.get(&id).ok_or(CoreError::NodeNotFound(id))
    }

    /// Who holds `id`, or `None` when nothing does (or it does not exist).
    #[must_use]
    pub fn parent_of(&self, id: NodeId) -> Option<Parent> {
        if self.layers.contains(&id) {
            return Some(Parent::Root);
        }
        let in_node = self
            .nodes
            .values()
            .find(|n| n.container().is_some_and(|c| c.children.contains(&id)));
        if let Some(p) = in_node {
            return Some(Parent::Node(p.id));
        }
        self.resources
            .iter()
            .find(|(_, r)| r.content().contains(&id))
            .map(|(rid, _)| Parent::Pattern(*rid))
    }

    /// `root` and all its descendants, parents before children.
    #[must_use]
    pub fn subtree(&self, root: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue; // a cycle; validate() reports it
            }
            out.push(id);
            if let Some(c) = self.nodes.get(&id).and_then(Node::container) {
                stack.extend(c.children.iter().rev());
            }
        }
        out
    }

    /// Copies of `roots` and their descendants, ready for a clipboard
    /// ([`Document::fresh_copies`] before inserting them again).
    ///
    /// # Errors
    /// [`CoreError::NodeNotFound`] when a root does not exist.
    pub fn copy_subtrees(&self, roots: &[NodeId]) -> Result<Vec<Node>, CoreError> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for &root in roots {
            self.node(root)?;
            for id in self.subtree(root) {
                if seen.insert(id) {
                    out.push(self.node(id)?.clone());
                }
            }
        }
        Ok(out)
    }

    /// The same nodes with fresh ids from this document's allocator;
    /// references between them (children, clip) are remapped. Used for
    /// paste and duplicate so inserted ids never collide.
    pub fn fresh_copies(&mut self, nodes: &[Node]) -> Vec<Node> {
        self.fresh_copies_mapped(nodes).0
    }

    /// [`Document::fresh_copies`], plus the old → new id map.
    pub(crate) fn fresh_copies_mapped(
        &mut self,
        nodes: &[Node],
    ) -> (Vec<Node>, BTreeMap<NodeId, NodeId>) {
        let map: BTreeMap<NodeId, NodeId> =
            nodes.iter().map(|n| (n.id, self.alloc_node_id())).collect();
        let remap = |id: NodeId| map.get(&id).copied().unwrap_or(id);
        let copies = nodes
            .iter()
            .map(|n| {
                let mut copy = n.clone();
                copy.id = remap(n.id);
                if let Some(c) = copy.container_mut() {
                    for child in &mut c.children {
                        *child = remap(*child);
                    }
                    c.clip = c.clip.map(remap);
                }
                copy
            })
            .collect();
        (copies, map)
    }

    /// True when any paint in the document refers to `id`.
    #[must_use]
    pub fn resource_in_use(&self, id: ResourceId) -> bool {
        use crate::style::Paint;
        self.nodes.values().any(|n| {
            n.paints()
                .into_iter()
                .any(|p| matches!(p, Paint::Gradient(r) | Paint::Pattern(r) if r == id))
        })
    }
}
