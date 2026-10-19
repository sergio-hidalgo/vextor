//! Every edit to a document is a [`Command`].

use crate::color::ColorFormat;
use crate::document::Artboard;
use crate::ids::{NodeId, ResourceId};
use crate::math::{Transform, Vec2};
use crate::node::{Geometry, Node};
use crate::path::{NodeType, Path, PathNode, PathNodeRef};
use crate::resource::Resource;
use crate::settings::GridSettings;
use crate::style::Style;
use crate::text::{FontSpec, TextAlign, TextFrame};

/// One undoable edit. Applied with [`crate::Document::apply`]; either it
/// succeeds completely or the document is left unchanged.
///
/// `parent: None` means the top level ([`crate::Document::layers`]), which
/// holds layers only; layers can live nowhere else.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    // ----- structure -----
    /// Inserts whole subtrees. Roots are the nodes no other node in `nodes`
    /// lists as a child; they go at `index` in their given order.
    Insert {
        parent: Option<NodeId>,
        index: usize,
        nodes: Vec<Node>,
    },
    /// Deletes nodes and their descendants. Pattern content is removed with
    /// its resource instead.
    Delete {
        ids: Vec<NodeId>,
    },
    /// Moves nodes (in the given order) to `new_parent` at `index`, counted
    /// after they were taken out of their old place.
    Reparent {
        ids: Vec<NodeId>,
        new_parent: Option<NodeId>,
        index: usize,
    },
    /// Wraps siblings in a new group placed where the topmost of them was.
    Group {
        ids: Vec<NodeId>,
    },
    /// Replaces a group by its children; the group's transform, opacity,
    /// visibility and lock are folded into each child.
    Ungroup {
        id: NodeId,
    },

    // ----- properties -----
    SetTransform {
        ids: Vec<NodeId>,
        transform: Transform,
    },
    /// Gives each node its own transform (moving, aligning or scaling
    /// several objects at once).
    SetTransforms {
        items: Vec<(NodeId, Transform)>,
    },
    /// Fill and stroke of shapes, or glyph paint of text objects.
    SetStyle {
        ids: Vec<NodeId>,
        style: Style,
    },
    /// Replaces a text object's characters.
    SetText {
        id: NodeId,
        content: String,
    },
    /// Font and metrics of text objects (glyph paint goes through
    /// [`Command::SetStyle`]).
    SetTextStyle {
        ids: Vec<NodeId>,
        font: FontSpec,
        size: f64,
        line_height: f64,
        letter_spacing: f64,
        align: TextAlign,
    },
    /// Background and border of text objects; `None` removes them.
    SetTextFrame {
        ids: Vec<NodeId>,
        frame: Option<TextFrame>,
    },
    /// Replaces shape geometries (a resize baked into the shape, so its
    /// stroke keeps its width).
    SetGeometry {
        items: Vec<(NodeId, Geometry)>,
    },
    SetProps {
        id: NodeId,
        name: Option<String>,
        visible: bool,
        locked: bool,
        opacity: f64,
    },

    // ----- paths -----
    /// Inserts a node before `at.index`. `at.subpath == subpaths.len()`
    /// (with `index` 0) starts a new subpath.
    AddPathNode {
        at: PathNodeRef,
        node: PathNode,
    },
    MovePathNodes {
        nodes: Vec<PathNodeRef>,
        delta: Vec2,
    },
    /// Sets both handles; the node type is then restored keeping the
    /// handle that changed (`handle_out` when both did).
    SetHandles {
        node: PathNodeRef,
        handle_in: Option<Vec2>,
        handle_out: Option<Vec2>,
    },
    /// Changes the node type, aligning existing handles to their average.
    SetNodeType {
        node: PathNodeRef,
        kind: NodeType,
    },
    /// Straight ↔ curve for the segment from `index` to the next node.
    /// A new curve gets handles a third of the way along the segment.
    ToggleSegment {
        shape: NodeId,
        subpath: usize,
        index: usize,
    },
    /// Empty subpaths are removed; a path left empty removes its shape.
    DeletePathNodes {
        nodes: Vec<PathNodeRef>,
    },
    CloseSubPath {
        shape: NodeId,
        subpath: usize,
    },
    /// Opens a closed subpath at `node`, which becomes both ends.
    OpenAt {
        node: PathNodeRef,
    },
    /// Joins two open endpoints of the same shape: closes the subpath when
    /// both are its ends, otherwise concatenates the subpaths (`a`'s end
    /// to `b`'s start), merging the two anchors when they coincide.
    JoinEndpoints {
        a: PathNodeRef,
        b: PathNodeRef,
    },
    /// Splits an open subpath at an interior node into two subpaths.
    SplitAt {
        node: PathNodeRef,
    },
    Reverse {
        shape: NodeId,
        subpath: usize,
    },
    /// Replaces shape geometries by paths computed in `vextor-geom`.
    ConvertToPath {
        items: Vec<(NodeId, Path)>,
    },

    // ----- path operations (vextor-geom computes, core records) -----
    /// Deletes `removed`, then inserts `inserted` like [`Command::Insert`].
    ReplaceWith {
        removed: Vec<NodeId>,
        inserted: Vec<Node>,
        parent: Option<NodeId>,
        index: usize,
    },

    // ----- resources -----
    /// Adds or replaces a resource. `nodes` are new nodes for a pattern's
    /// content; content of a replaced pattern no longer listed is removed.
    PutResource {
        id: ResourceId,
        resource: Resource,
        nodes: Vec<Node>,
    },
    /// Removes an unused resource and its pattern content.
    RemoveResource {
        id: ResourceId,
    },

    // ----- document -----
    SetArtboards(Vec<Artboard>),
    /// Renames the document (the name an unsaved document is saved under).
    /// Refused when empty or only whitespace.
    SetName(String),
    SetGrid(GridSettings),
    /// How colours are written when the document is saved.
    SetColorFormat(ColorFormat),

    // ----- composite -----
    /// Runs `commands` in order as one undo step named `label`; either all
    /// of them apply or none does.
    Batch {
        label: &'static str,
        commands: Vec<Command>,
    },
}

impl Command {
    /// Short name shown in the Edit menu ("Undo Move nodes").
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Insert { .. } => "Insert",
            Self::Delete { .. } => "Delete",
            Self::Reparent { .. } => "Move to layer",
            Self::Group { .. } => "Group",
            Self::Ungroup { .. } => "Ungroup",
            Self::SetTransform { .. } | Self::SetTransforms { .. } => "Transform",
            Self::SetStyle { .. } => "Change style",
            Self::SetText { .. } => "Edit text",
            Self::SetTextStyle { .. } => "Change text style",
            Self::SetTextFrame { .. } => "Change text frame",
            Self::SetGeometry { .. } => "Change shape",
            Self::SetProps { .. } => "Change properties",
            Self::AddPathNode { .. } => "Add node",
            Self::MovePathNodes { .. } => "Move nodes",
            Self::SetHandles { .. } => "Move handle",
            Self::SetNodeType { .. } => "Change node type",
            Self::ToggleSegment { .. } => "Toggle segment",
            Self::DeletePathNodes { .. } => "Delete nodes",
            Self::CloseSubPath { .. } => "Close path",
            Self::OpenAt { .. } => "Open path",
            Self::JoinEndpoints { .. } => "Join",
            Self::SplitAt { .. } => "Split",
            Self::Reverse { .. } => "Reverse path",
            Self::ConvertToPath { .. } => "Convert to path",
            Self::ReplaceWith { .. } => "Path operation",
            Self::PutResource { .. } => "Edit resource",
            Self::RemoveResource { .. } => "Remove resource",
            Self::SetArtboards(_) => "Edit artboards",
            Self::SetName(_) => "Rename document",
            Self::SetGrid(_) => "Edit grid",
            Self::SetColorFormat(_) => "Change colour format",
            Self::Batch { label, .. } => label,
        }
    }
}
