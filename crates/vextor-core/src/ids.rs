//! Stable identifiers. Allocated by [`crate::Document`], never reused.

use serde::{Deserialize, Serialize};

/// Identifies a node (layer, group, shape or image) in [`crate::Document::nodes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeId(pub u64);

/// Identifies a shared resource (gradient or pattern) in [`crate::Document::resources`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResourceId(pub u64);

/// Identifies an artboard in [`crate::Document::artboards`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ArtboardId(pub u64);
