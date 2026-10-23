//! The Layers panel's rules (spec 10), as functions on a [`Session`] so
//! they are tested without a window: the rows and their labels, clicking
//! a row, renaming, visibility, lock, layer opacity, the layer buttons and
//! drag and drop. The panel itself is `panels/layers.rs`.

use std::collections::BTreeSet;

use vextor_core::{Command, Document, Geometry, Node, NodeId, NodeKind, Parent};

use crate::icons::Icon;
use crate::session::Session;

/// Longest default label taken from a text's first line, in characters.
const TEXT_LABEL_CHARS: usize = 40;

/// What the panel remembers per document. Not saved, not undone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PanelState {
    /// Rows the user opened or closed against the default (layers open,
    /// groups closed).
    toggled: BTreeSet<NodeId>,
    /// The selection the rows last revealed.
    revealed: Vec<NodeId>,
    /// The row to scroll to on the next frame.
    pub scroll_to: Option<NodeId>,
    /// The row being renamed and the name typed so far.
    pub renaming: Option<(NodeId, String)>,
}

impl PanelState {
    /// Whether `id`'s children are listed.
    #[must_use]
    pub fn is_open(&self, doc: &Document, id: NodeId) -> bool {
        doc.layers.contains(&id) != self.toggled.contains(&id)
    }

    /// Opens or closes `id`'s row.
    pub fn toggle(&mut self, id: NodeId) {
        if !self.toggled.remove(&id) {
            self.toggled.insert(id);
        }
    }

    fn open(&mut self, doc: &Document, id: NodeId) {
        if !self.is_open(doc, id) {
            self.toggle(id);
        }
    }
}

/// One line of the panel.
#[allow(clippy::struct_excessive_bools)] // one flag per thing a row shows
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: NodeId,
    /// 0 for a layer, 1 for its children, …
    pub depth: usize,
    pub icon: Icon,
    pub label: String,
    /// The label is the user's name, not a default one.
    pub named: bool,
    pub visible: bool,
    pub locked: bool,
    /// A layer or group above it is hidden or locked: drawn dimmed.
    pub inherited: bool,
    /// The clip shape of its group.
    pub clip: bool,
    /// A layer or group: it can be opened and dropped into.
    pub container: bool,
    pub open: bool,
    pub selected: bool,
    /// The current layer: where new objects go.
    pub current: bool,
}

/// The panel's rows, top → bottom as stacked (the top layer first), the
/// children of open rows under them.
#[must_use]
pub fn rows(s: &Session) -> Vec<Row> {
    let selected: BTreeSet<NodeId> = s.selected().into_iter().collect();
    let current = current_layer(s);
    let mut out = Vec::new();
    for &layer in s.doc.layers.iter().rev() {
        add_rows(s, layer, 0, false, None, &selected, current, &mut out);
    }
    out
}

#[allow(clippy::too_many_arguments)] // the walk's state, passed down
fn add_rows(
    s: &Session,
    id: NodeId,
    depth: usize,
    inherited: bool,
    clip: Option<NodeId>,
    selected: &BTreeSet<NodeId>,
    current: Option<NodeId>,
    out: &mut Vec<Row>,
) {
    let Some(node) = s.doc.nodes.get(&id) else {
        return;
    };
    let open = s.layers_panel.is_open(&s.doc, id);
    out.push(Row {
        id,
        depth,
        icon: icon(node),
        label: label(node),
        named: node.name.is_some(),
        visible: node.visible,
        locked: node.locked,
        inherited,
        clip: clip == Some(id),
        container: node.container().is_some(),
        open,
        selected: selected.contains(&id),
        current: current == Some(id),
    });
    if let Some(c) = node.container()
        && open
        && depth < 64
    {
        let inherited = inherited || !node.visible || node.locked;
        for &child in c.children.iter().rev() {
            add_rows(
                s,
                child,
                depth + 1,
                inherited,
                c.clip,
                selected,
                current,
                out,
            );
        }
    }
}

/// A row's label: the name; else a text's first line ("T" icon), else
/// what the object is.
#[must_use]
pub fn label(node: &Node) -> String {
    if let Some(name) = &node.name {
        return name.clone();
    }
    match &node.kind {
        NodeKind::Layer(_) => "Layer".to_owned(),
        NodeKind::Group(c) if c.clip.is_some() => "Clip group".to_owned(),
        NodeKind::Group(_) => "Group".to_owned(),
        NodeKind::Image(_) => "Image".to_owned(),
        NodeKind::Text(t) => {
            let line = t.content.lines().next().unwrap_or_default().trim();
            if line.is_empty() {
                "Text".to_owned()
            } else if line.chars().count() > TEXT_LABEL_CHARS {
                let cut: String = line.chars().take(TEXT_LABEL_CHARS - 1).collect();
                format!("{}…", cut.trim_end())
            } else {
                line.to_owned()
            }
        }
        NodeKind::Shape(shape) => match shape.geometry {
            Geometry::Rect { .. } => "Rectangle",
            Geometry::Ellipse { .. } => "Ellipse",
            Geometry::Polygon { .. } => "Polygon",
            Geometry::Star { .. } => "Star",
            Geometry::Line { .. } => "Line",
            Geometry::Path(_) => "Path",
        }
        .to_owned(),
    }
}

fn icon(node: &Node) -> Icon {
    match &node.kind {
        NodeKind::Layer(_) => Icon::Layer,
        NodeKind::Group(_) => Icon::Group,
        NodeKind::Image(_) => Icon::Image,
        NodeKind::Text(_) => Icon::ToolText,
        NodeKind::Shape(shape) => match shape.geometry {
            Geometry::Rect { .. } => Icon::ToolRect,
            Geometry::Ellipse { .. } => Icon::ToolEllipse,
            Geometry::Polygon { .. } => Icon::ToolPolygon,
            Geometry::Star { .. } => Icon::ToolStar,
            Geometry::Line { .. } => Icon::ToolLine,
            Geometry::Path(_) => Icon::ToolPen,
        },
    }
}

/// The layer `id` is in (itself for a layer).
#[must_use]
pub fn layer_of(doc: &Document, id: NodeId) -> Option<NodeId> {
    let mut at = id;
    for _ in 0..=64 {
        match doc.parent_of(at)? {
            Parent::Root => return Some(at),
            Parent::Node(p) => at = p,
            Parent::Pattern(_) => return None,
        }
    }
    None
}

/// The ancestors of `id`, nearest first.
fn ancestors(doc: &Document, id: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut at = id;
    while let Some(Parent::Node(p)) = doc.parent_of(at) {
        out.push(p);
        at = p;
        if out.len() > 64 {
            break;
        }
    }
    out
}

/// Whether `id` can be selected and edited: it and every layer or group
/// above it are visible and unlocked.
#[must_use]
pub fn editable(doc: &Document, id: NodeId) -> bool {
    std::iter::once(id)
        .chain(ancestors(doc, id))
        .chain(layer_of(doc, id))
        .all(|n| doc.nodes.get(&n).is_some_and(|n| n.visible && !n.locked))
}

/// The current layer: the one picked in the panel, or the top one.
#[must_use]
pub fn current_layer(s: &Session) -> Option<NodeId> {
    s.layer
        .filter(|l| s.doc.layers.contains(l))
        .or_else(|| s.doc.layers.last().copied())
}

/// Follows a selection made elsewhere (the canvas): opens the rows above
/// the topmost selected object, scrolls to it and makes its layer current.
pub fn reveal(s: &mut Session) {
    let selected = s.selected();
    if selected == s.layers_panel.revealed {
        return;
    }
    s.layers_panel.revealed.clone_from(&selected);
    let Some(&top) = selected.last() else {
        return;
    };
    for id in ancestors(&s.doc, top) {
        s.layers_panel.open(&s.doc, id);
    }
    s.layer = layer_of(&s.doc, top);
    s.layers_panel.scroll_to = Some(top);
}

/// A click on row `id`; `add` (⇧) adds an object to the selection or
/// takes it out. A layer row makes that layer current and clears the
/// selection; a hidden or locked object is not selected.
pub fn click(s: &mut Session, id: NodeId, add: bool) {
    if s.doc.layers.contains(&id) {
        s.layer = Some(id);
        if !add {
            s.select(Vec::new());
        }
        return;
    }
    s.layer = layer_of(&s.doc, id);
    if !editable(&s.doc, id) {
        return;
    }
    let mut ids = if add { s.selected() } else { Vec::new() };
    if let Some(i) = ids.iter().position(|&x| x == id) {
        ids.remove(i);
    } else {
        ids.push(id);
    }
    s.select(ids);
}

/// One undo step changing `id`'s name, visibility, lock or opacity, or
/// `None` when nothing would change.
fn props(
    doc: &Document,
    id: NodeId,
    label: &'static str,
    edit: impl FnOnce(&mut Node),
) -> Option<Command> {
    let old = doc.nodes.get(&id)?;
    let mut n = old.clone();
    edit(&mut n);
    if n == *old {
        return None;
    }
    Some(Command::Batch {
        label,
        commands: vec![Command::SetProps {
            id,
            name: n.name,
            visible: n.visible,
            locked: n.locked,
            opacity: n.opacity,
        }],
    })
}

/// Starts renaming row `id` in place, from its current name.
pub fn start_rename(s: &mut Session, id: NodeId) {
    let name = s.doc.nodes.get(&id).map(label).unwrap_or_default();
    s.layers_panel.renaming = Some((id, name));
}

/// Renames `id` to `typed`, kept as typed (through
/// [`vextor_core::clean_name`]: trimmed, at most 64 characters, no control
/// characters; the SVG id is made from it on export). Blank removes the name, so the row shows its
/// default label again.
#[must_use]
pub fn rename(doc: &Document, id: NodeId, typed: &str) -> Option<Command> {
    let name = vextor_core::clean_name(typed);
    props(doc, id, "Rename", |n| n.name = name)
}

/// Shows or hides `id`.
#[must_use]
pub fn toggle_visible(doc: &Document, id: NodeId) -> Option<Command> {
    let visible = doc.nodes.get(&id)?.visible;
    props(doc, id, if visible { "Hide" } else { "Show" }, |n| {
        n.visible = !visible;
    })
}

/// Locks or unlocks `id`.
#[must_use]
pub fn toggle_locked(doc: &Document, id: NodeId) -> Option<Command> {
    let locked = doc.nodes.get(&id)?.locked;
    props(doc, id, if locked { "Unlock" } else { "Lock" }, |n| {
        n.locked = !locked;
    })
}

/// Sets `id`'s opacity (clamped to 0..=1). Only that node changes, so
/// every other layer keeps its own.
#[must_use]
pub fn set_opacity(doc: &Document, id: NodeId, opacity: f64) -> Option<Command> {
    if !opacity.is_finite() {
        return None;
    }
    props(doc, id, "Change opacity", |n| {
        n.opacity = opacity.clamp(0.0, 1.0);
    })
}

/// Applies a row edit; objects it hid or locked leave the selection.
///
/// # Errors
/// A message for the user when the edit is refused.
pub fn apply(s: &mut Session, cmd: Option<Command>) -> Result<(), String> {
    let Some(cmd) = cmd else {
        return Ok(());
    };
    let label = cmd.label();
    s.apply(cmd).map_err(|e| format!("{label} failed: {e}"))?;
    let kept: Vec<NodeId> = s
        .selected()
        .into_iter()
        .filter(|&id| editable(&s.doc, id))
        .collect();
    if kept.len() != s.selected().len() {
        s.select(kept);
    }
    Ok(())
}

mod buttons;
mod drop;

pub use buttons::{delete_layer, duplicate_layer, new_layer};
pub use drop::{Drop, dragged, drop, drop_at, drop_command};

#[cfg(test)]
mod tests;
