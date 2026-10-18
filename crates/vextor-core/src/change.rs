//! Undo records: per-node before/after snapshots, not document clones.

use std::collections::BTreeMap;

use crate::color::ColorFormat;
use crate::document::{Artboard, Document};
use crate::editor::Selection;
use crate::ids::{NodeId, ResourceId};
use crate::node::Node;
use crate::resource::Resource;
use crate::settings::GridSettings;

/// One undo step: what every touched part looked like before and after.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    /// "Move nodes", shown in the Edit menu.
    pub label: &'static str,
    pub before: Patch,
    pub after: Patch,
    /// Restored by undo. Set by [`crate::History`]; `None` from `apply`.
    pub selection_before: Selection,
}

/// The state of some parts of a document. Writing it back restores them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Patch {
    /// `None` = absent.
    pub nodes: BTreeMap<NodeId, Option<Node>>,
    pub resources: BTreeMap<ResourceId, Option<Resource>>,
    pub layers: Option<Vec<NodeId>>,
    pub artboards: Option<Vec<Artboard>>,
    pub grid: Option<GridSettings>,
    pub color_format: Option<ColorFormat>,
}

impl Patch {
    /// True when both patches touch exactly the same parts.
    #[must_use]
    pub fn same_targets(&self, other: &Self) -> bool {
        self.nodes.keys().eq(other.nodes.keys())
            && self.resources.keys().eq(other.resources.keys())
            && self.layers.is_some() == other.layers.is_some()
            && self.artboards.is_some() == other.artboards.is_some()
            && self.grid.is_some() == other.grid.is_some()
            && self.color_format.is_some() == other.color_format.is_some()
    }

    /// Adds `other`'s entries for parts this patch does not cover yet.
    pub(crate) fn fill_from(&mut self, other: Self) {
        for (k, v) in other.nodes {
            self.nodes.entry(k).or_insert(v);
        }
        for (k, v) in other.resources {
            self.resources.entry(k).or_insert(v);
        }
        self.layers = self.layers.take().or(other.layers);
        self.artboards = self.artboards.take().or(other.artboards);
        self.grid = self.grid.take().or(other.grid);
        self.color_format = self.color_format.or(other.color_format);
    }

    /// Replaces this patch's entries by `other`'s where both exist.
    pub(crate) fn overwrite_with(&mut self, other: Self) {
        self.nodes.extend(other.nodes);
        self.resources.extend(other.resources);
        self.layers = other.layers.or(self.layers.take());
        self.artboards = other.artboards.or(self.artboards.take());
        self.grid = other.grid.or(self.grid.take());
        self.color_format = other.color_format.or(self.color_format);
    }
}

impl Document {
    /// Writes a patch back into the document.
    pub fn apply_patch(&mut self, patch: &Patch) {
        for (id, node) in &patch.nodes {
            match node {
                Some(n) => {
                    self.nodes.insert(*id, n.clone());
                }
                None => {
                    self.nodes.remove(id);
                }
            }
        }
        for (id, res) in &patch.resources {
            match res {
                Some(r) => {
                    self.resources.insert(*id, r.clone());
                }
                None => {
                    self.resources.remove(id);
                }
            }
        }
        if let Some(layers) = &patch.layers {
            self.layers.clone_from(layers);
        }
        if let Some(artboards) = &patch.artboards {
            self.artboards.clone_from(artboards);
        }
        if let Some(grid) = &patch.grid {
            self.grid = grid.clone();
        }
        if let Some(format) = patch.color_format {
            self.color_format = format;
        }
    }
}
