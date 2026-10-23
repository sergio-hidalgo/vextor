//! Paint edits (spec 09) as commands: the fill or stroke of shapes and
//! text glyphs, a text's background and border, object opacity and the
//! document's swatches. The inspector and the canvas only call these; the
//! rules are tested here without a window.

use vextor_core::{
    Command, CornerRadii, Document, LineCap, LineJoin, Node, NodeId, NodeKind, Paint, Resource,
    ResourceId, Stroke, Style, SwatchKind, TextFrame,
};
use vextor_geom::PaintPart;

mod gradient;
mod resources;

pub use gradient::{
    DashPreset, GradientPoint, GradientShape, add_stop, dash_preset, format_dash, gradient_points,
    move_point, move_stop, new_gradient, parse_dash, preset_dash, remove_stop, tile_parts,
    tile_transform,
};
pub use resources::{MAX_RESOURCE_NAME_CHARS, delete_resource, rename_resource};

/// Which paint of an object an edit is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// A shape's fill, or a text's glyph fill.
    Fill,
    /// A shape's stroke, or a text's glyph stroke.
    Stroke,
    /// A text's frame fill.
    Background,
    /// A text's frame stroke.
    Border,
}

impl Target {
    /// The undo step's name.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Fill => "Change fill",
            Self::Stroke => "Change stroke",
            Self::Background => "Change background",
            Self::Border => "Change border",
        }
    }

    /// The part of the object it paints.
    #[must_use]
    pub fn part(self) -> PaintPart {
        match self {
            Self::Fill | Self::Stroke => PaintPart::Body,
            Self::Background | Self::Border => PaintPart::Frame,
        }
    }

    /// The swatch list its colour editor shows: strokes and borders share
    /// one, fills and backgrounds the other.
    #[must_use]
    pub fn swatch_kind(self) -> SwatchKind {
        if self.is_stroke() {
            SwatchKind::Stroke
        } else {
            SwatchKind::Fill
        }
    }

    /// True for a stroke or a border.
    #[must_use]
    pub fn is_stroke(self) -> bool {
        matches!(self, Self::Stroke | Self::Border)
    }
}

/// One change to a paint.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    /// The paint itself; `Paint::None` on a stroke removes the stroke.
    Paint(Paint),
    Width(f64),
    Cap(LineCap),
    Join(LineJoin),
    Miter(f64),
    /// Dash and gap lengths, and the cap a preset needs (dots are round).
    Dash(Vec<f64>, Option<LineCap>),
}

/// The frame a text gets when a background or border is first turned on.
#[must_use]
pub fn empty_frame() -> TextFrame {
    TextFrame {
        padding: [8.0, 16.0, 8.0, 16.0],
        radius: CornerRadii {
            tl: 6.0,
            tr: 6.0,
            br: 6.0,
            bl: 6.0,
        },
        fill: Paint::None,
        stroke: None,
    }
}

/// `node`'s fill and stroke for `target`: a shape's or text's style, or a
/// text's frame as a style. `None` when the node has no such paint.
fn style_of(node: &Node, target: Target) -> Option<Style> {
    match (&node.kind, target.part()) {
        (NodeKind::Shape(s), PaintPart::Body) => Some(s.style.clone()),
        (NodeKind::Text(t), PaintPart::Body) => Some(t.style.clone()),
        (NodeKind::Text(t), PaintPart::Frame) => Some(t.frame.as_ref().map_or(
            Style {
                fill: Paint::None,
                stroke: None,
            },
            |f| Style {
                fill: f.fill,
                stroke: f.stroke.clone(),
            },
        )),
        _ => None,
    }
}

/// The paint `target` shows on `node`; `Paint::None` without a stroke.
#[must_use]
pub fn paint(node: &Node, target: Target) -> Option<Paint> {
    let style = style_of(node, target)?;
    Some(if target.is_stroke() {
        style.stroke.map_or(Paint::None, |s| s.paint)
    } else {
        style.fill
    })
}

/// The stroke `target` (a stroke or border) has on `node`.
#[must_use]
pub fn stroke(node: &Node, target: Target) -> Option<Stroke> {
    style_of(node, target)?
        .stroke
        .filter(|_| target.is_stroke())
}

/// The objects among `ids` that `target` applies to: shapes and texts,
/// texts only for a background or border.
#[must_use]
pub fn targets(doc: &Document, ids: &[NodeId], target: Target) -> Vec<NodeId> {
    ids.iter()
        .copied()
        .filter(|id| {
            doc.nodes
                .get(id)
                .is_some_and(|n| style_of(n, target).is_some())
        })
        .collect()
}

fn apply(style: &mut Style, target: Target, edit: &Edit) {
    if !target.is_stroke() {
        if let Edit::Paint(p) = edit {
            style.fill = *p;
        }
        return;
    }
    if let Edit::Paint(p) = edit {
        if *p == Paint::None {
            style.stroke = None;
        } else {
            style.stroke.get_or_insert_with(Stroke::default).paint = *p;
        }
        return;
    }
    let Some(s) = &mut style.stroke else {
        return;
    };
    match edit {
        Edit::Width(w) => s.width = w.max(0.0),
        Edit::Cap(c) => s.cap = *c,
        Edit::Join(j) => s.join = *j,
        Edit::Miter(m) => s.miter_limit = m.max(1.0),
        Edit::Dash(dash, cap) => {
            s.dash.clone_from(dash);
            if let Some(c) = cap {
                s.cap = *c;
            }
        }
        Edit::Paint(_) => {}
    }
}

/// The commands giving `node` the edited `style` for `target`.
fn restyle(id: NodeId, node: &Node, target: Target, style: Style) -> Command {
    let ids = vec![id];
    match (&node.kind, target.part()) {
        (NodeKind::Text(t), PaintPart::Frame) => {
            let mut frame = t.frame.clone().unwrap_or_else(empty_frame);
            frame.fill = style.fill;
            frame.stroke = style.stroke;
            // Neither background nor border: no frame at all.
            let frame = (frame.fill != Paint::None || frame.stroke.is_some()).then_some(frame);
            Command::SetTextFrame { ids, frame }
        }
        _ => Command::SetStyle { ids, style },
    }
}

/// The commands applying `edit` to `target` of every object among `ids`
/// it applies to, each keeping its other values.
fn edit_commands(doc: &Document, ids: &[NodeId], target: Target, edit: &Edit) -> Vec<Command> {
    targets(doc, ids, target)
        .into_iter()
        .filter_map(|id| {
            let node = doc.nodes.get(&id)?;
            let before = style_of(node, target)?;
            let mut after = before.clone();
            apply(&mut after, target, edit);
            (after != before).then(|| restyle(id, node, target, after))
        })
        .collect()
}

/// One undo step applying `edit` to `target` of the objects among `ids`;
/// `None` when nothing would change.
#[must_use]
pub fn command(doc: &Document, ids: &[NodeId], target: Target, edit: &Edit) -> Option<Command> {
    let commands = edit_commands(doc, ids, target, edit);
    (!commands.is_empty()).then(|| Command::Batch {
        label: target.label(),
        commands,
    })
}

/// One undo step adding `resource` under a fresh id and painting `target`
/// of the objects among `ids` with it (`paint` makes the paint from the
/// id).
pub fn command_with_resource(
    doc: &mut Document,
    ids: &[NodeId],
    target: Target,
    resource: Resource,
    paint: impl FnOnce(ResourceId) -> Paint,
) -> Command {
    let id = doc.alloc_resource_id();
    let mut commands = vec![Command::PutResource {
        id,
        resource,
        nodes: Vec::new(),
    }];
    commands.extend(edit_commands(doc, ids, target, &Edit::Paint(paint(id))));
    Command::Batch {
        label: target.label(),
        commands,
    }
}

/// One undo step replacing resource `id` (a gradient or a pattern whose
/// content stays the same) by `resource`.
#[must_use]
pub fn edit_resource(id: ResourceId, resource: Resource) -> Command {
    let label = match resource {
        Resource::Gradient(_) => "Edit gradient",
        Resource::Pattern(_) => "Edit pattern",
    };
    Command::Batch {
        label,
        commands: vec![Command::PutResource {
            id,
            resource,
            nodes: Vec::new(),
        }],
    }
}

/// How many objects paint with resource `id`.
#[must_use]
pub fn users(doc: &Document, id: ResourceId) -> usize {
    doc.nodes
        .values()
        .filter(|n| {
            n.paints()
                .into_iter()
                .any(|p| matches!(p, Paint::Gradient(r) | Paint::Pattern(r) if r == id))
        })
        .count()
}

/// One undo step setting the opacity of every object among `ids`
/// (0..=1); `None` when nothing would change.
#[must_use]
pub fn opacity_command(doc: &Document, ids: &[NodeId], opacity: f64) -> Option<Command> {
    let opacity = opacity.clamp(0.0, 1.0);
    let commands: Vec<Command> = ids
        .iter()
        .filter_map(|id| doc.nodes.get(id))
        .filter(|n| (n.opacity - opacity).abs() > vextor_core::EPS)
        .map(|n| Command::SetProps {
            id: n.id,
            name: n.name.clone(),
            visible: n.visible,
            locked: n.locked,
            opacity,
        })
        .collect();
    (!commands.is_empty()).then_some(Command::Batch {
        label: "Change opacity",
        commands,
    })
}

/// Object ▸ Make Pattern: copies of the selected objects become a new
/// pattern whose tile is their painted bounds (strokes included), in one
/// undo step. The objects stay; the pattern is then picked in Fill or
/// Stroke ▸ Pattern.
///
/// # Errors
/// A message for the user when nothing suitable is selected or the change
/// is refused.
pub fn make_pattern(s: &mut crate::session::Session) -> Result<(), String> {
    let ids = s.selected();
    let tile = ids
        .iter()
        .filter_map(|id| vextor_geom::visual_bounds(&s.doc, &s.fonts, *id))
        .reduce(|a, b| a.union(b))
        .ok_or("Make Pattern: select the objects to repeat")?;
    let tile = vextor_core::Rect::new(tile.x0, tile.y0, tile.width(), tile.height());
    let (cmd, _) = s
        .doc
        .make_pattern(&ids, tile)
        .map_err(|e| format!("Make Pattern: {e}"))?;
    s.apply(cmd)
        .map_err(|e| format!("Make Pattern failed: {e}"))
}

#[cfg(test)]
mod tests;
