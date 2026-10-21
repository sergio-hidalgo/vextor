//! Editor state: per-window, never saved, never undone (except the
//! selection, which undo restores).

use serde::{Deserialize, Serialize};

use crate::ids::{NodeId, ResourceId};
use crate::math::Vec2;
use crate::node::Node;
use crate::path::PathNodeRef;
use crate::resource::Resource;
use crate::style::Style;

/// What the user is working with right now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorState {
    pub selection: Selection,
    pub tool: ToolId,
    pub view: View,
    pub clipboard: Option<ClipboardPayload>,
    /// The fill and stroke new shapes get: the last style given to a shape
    /// (spec 06).
    pub default_style: Style,
}

/// The current selection.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Selection {
    #[default]
    None,
    /// Object mode.
    Objects(Vec<NodeId>),
    /// Node-edit mode.
    PathNodes(Vec<PathNodeRef>),
    /// The segment from node `index` to the next one.
    Segment {
        shape: NodeId,
        subpath: usize,
        index: usize,
    },
}

/// The active tool. Later specs add their tools here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolId {
    #[default]
    Select,
    /// Selects and edits the nodes of one path (spec 07).
    Node,
    /// Draws paths node by node (spec 07).
    Pen,
    /// Places and edits text (spec 05c).
    Text,
    /// Draws a rectangle (spec 06).
    Rect,
    /// Draws an ellipse (spec 06).
    Ellipse,
    /// Draws a regular polygon (spec 06).
    Polygon,
    /// Draws a star (spec 06).
    Star,
    /// Draws a straight line (spec 06).
    Line,
    /// Zooms in on a click, out on an `⌥`-click, to a dragged rectangle
    /// (spec 08b).
    Zoom,
}

impl ToolId {
    /// The shape tools, in tool-strip order.
    pub const SHAPES: [Self; 5] = [
        Self::Rect,
        Self::Ellipse,
        Self::Polygon,
        Self::Star,
        Self::Line,
    ];

    /// True for the tools that draw a shape.
    #[must_use]
    pub fn is_shape(self) -> bool {
        Self::SHAPES.contains(&self)
    }
}

/// Canvas zoom and pan.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    /// Screen px per document px, > 0.
    pub zoom: f64,
    /// Screen offset of the document origin.
    pub pan: Vec2,
}

impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::default(),
        }
    }
}

/// Copied nodes: whole subtrees, roots listed bottom → top. Each root's
/// transform is in document (world) space, so it can be pasted under any
/// parent ([`crate::Document::copy`], [`crate::Document::paste`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipboardPayload {
    pub nodes: Vec<Node>,
    /// The gradients and patterns the nodes paint with.
    pub resources: Vec<ClipResource>,
}

/// A resource copied with the nodes that use it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipResource {
    /// Its id in the document it was copied from; reused on paste when the
    /// target holds the same resource under this id.
    pub id: ResourceId,
    pub resource: Resource,
    /// A pattern's content subtrees (empty for a gradient).
    pub content: Vec<Node>,
}
