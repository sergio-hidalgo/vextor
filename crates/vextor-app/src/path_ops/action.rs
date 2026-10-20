//! The path commands offered by Object ▸ Path, the canvas's context menu
//! and the inspector's Path section: one table, so all three agree on
//! what is possible and why not.

use super::{
    close, close_target, delete_nodes, editing_nodes, insert_in_segment, join, join_target, open,
    open_target, reverse, reverse_targets, segment_target, split, split_target, toggle_segment,
};
use crate::session::Session;

/// A path command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathAction {
    Close,
    Open,
    Join,
    Split,
    Reverse,
    ToggleSegment,
    AddNode,
    DeleteNodes,
}

impl PathAction {
    /// Close, open, join, split and reverse: the commands that change how
    /// subpaths run.
    pub const STRUCTURE: [Self; 5] = [
        Self::Close,
        Self::Open,
        Self::Join,
        Self::Split,
        Self::Reverse,
    ];
    /// The commands on a selected segment or nodes.
    pub const EDIT: [Self; 3] = [Self::ToggleSegment, Self::AddNode, Self::DeleteNodes];

    /// Its menu text.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Close => "Close Path",
            Self::Open => "Open at Node",
            Self::Join => "Join",
            Self::Split => "Split at Node",
            Self::Reverse => "Reverse Direction",
            Self::ToggleSegment => "Straight ↔ Curve",
            Self::AddNode => "Add Node",
            Self::DeleteNodes => "Delete Nodes",
        }
    }

    /// What it needs, shown while it is disabled.
    #[must_use]
    pub fn why(self) -> &'static str {
        match self {
            Self::Close => "Select an open path, or a node of one",
            Self::Open => "Select one node of a closed path",
            Self::Join => "Select two open end nodes, or two open paths",
            Self::Split => "Select one node inside an open path",
            Self::Reverse => "Select a path",
            Self::ToggleSegment | Self::AddNode => "Select a segment (click it with A)",
            Self::DeleteNodes => "Select nodes (click them with A)",
        }
    }

    /// True when it would do something now.
    #[must_use]
    pub fn can(self, s: &Session) -> bool {
        match self {
            Self::Close => close_target(s).is_some(),
            Self::Open => open_target(s).is_some(),
            Self::Join => join_target(s).is_some(),
            Self::Split => split_target(s).is_some(),
            Self::Reverse => !reverse_targets(s).is_empty(),
            Self::ToggleSegment | Self::AddNode => segment_target(s).is_some(),
            Self::DeleteNodes => editing_nodes(s) && segment_target(s).is_none(),
        }
    }

    /// Does it, as one undo step.
    ///
    /// # Errors
    /// A message for the user when the change is refused.
    pub fn run(self, s: &mut Session) -> Result<(), String> {
        match self {
            Self::Close => close(s),
            Self::Open => open(s),
            Self::Join => join(s),
            Self::Split => split(s),
            Self::Reverse => reverse(s),
            Self::ToggleSegment => toggle_segment(s),
            Self::AddNode => insert_in_segment(s),
            Self::DeleteNodes => delete_nodes(s),
        }
    }
}
