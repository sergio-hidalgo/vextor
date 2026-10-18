//! Editor state: per-window, never saved, never undone (except the
//! selection, which undo restores).

use serde::{Deserialize, Serialize};

use crate::ids::NodeId;
use crate::math::Vec2;
use crate::node::Node;
use crate::path::PathNodeRef;

/// What the user is working with right now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorState {
    pub selection: Selection,
    pub tool: ToolId,
    pub view: View,
    pub clipboard: Option<ClipboardPayload>,
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

/// Copied nodes: whole subtrees, roots listed in z-order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipboardPayload {
    pub nodes: Vec<Node>,
}
