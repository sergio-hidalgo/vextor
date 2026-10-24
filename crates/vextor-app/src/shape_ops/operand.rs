//! What the path operations work on: each selected object as one area in
//! document space (a shape; a group as the union of what it shows; a text
//! as its outlines), and where a result takes the bottom object's place.

use kurbo::Affine;
use vextor_core::{
    Document, FillRule, Geometry, Node, NodeId, NodeKind, Paint, Parent, Shape, Style, Transform,
};
use vextor_geom::{Area, BooleanOp, Fonts, text_outlines, to_affine, to_bezpath};

use crate::session::Session;

/// One selected object as an operand: what it covers and how it paints.
pub(super) struct Operand {
    pub area: Area,
    pub style: Style,
}

/// True when `id` can take part in a boolean: a shape, a text, or a group
/// of those (no images anywhere inside).
pub(super) fn eligible(doc: &Document, id: NodeId) -> bool {
    match doc.nodes.get(&id).map(|n| &n.kind) {
        Some(NodeKind::Shape(_) | NodeKind::Text(_)) => true,
        Some(NodeKind::Group(c)) => {
            !c.children.is_empty() && c.children.iter().all(|&child| eligible(doc, child))
        }
        _ => false,
    }
}

/// True when `id` is or holds a text.
pub(super) fn has_text(doc: &Document, id: NodeId) -> bool {
    match doc.nodes.get(&id).map(|n| &n.kind) {
        Some(NodeKind::Text(_)) => true,
        Some(NodeKind::Group(c)) => c.children.iter().any(|&child| has_text(doc, child)),
        _ => false,
    }
}

/// The operands of `ids` (bottom → top), texts as their outlines.
///
/// # Errors
/// A message when an object cannot take part, or texts wait on fonts.
pub(super) fn collect(s: &Session, ids: &[NodeId], label: &str) -> Result<Vec<Operand>, String> {
    if ids.iter().any(|&id| has_text(&s.doc, id)) && s.fonts.db().is_empty() {
        return Err(format!(
            "{label}: the fonts are still loading, try again in a moment"
        ));
    }
    ids.iter()
        .map(|&id| {
            let world = to_affine(s.doc.world_transform(id));
            let node = s
                .doc
                .nodes
                .get(&id)
                .ok_or_else(|| format!("{label}: object is gone"))?;
            let (area, style) = covered(&s.doc, &s.fonts, node, world)
                .ok_or_else(|| format!("{label}: images can't take part, only shapes and text"))?;
            Ok(Operand {
                area,
                style: style.unwrap_or_else(default_style),
            })
        })
        .collect()
}

/// What `node` (drawn with `world`) covers, and the style of its bottom
/// shape; `None` for an image. Hidden children cover nothing.
fn covered(
    doc: &Document,
    fonts: &Fonts,
    node: &Node,
    world: Affine,
) -> Option<(Area, Option<Style>)> {
    match &node.kind {
        NodeKind::Shape(shape) => Some((shape_area(shape, world), Some(shape.style.clone()))),
        NodeKind::Text(text) => {
            let layout = fonts.layout(node.id, text);
            let (frame, glyphs) = text_outlines(text, &layout);
            let areas: Vec<Area> = frame
                .iter()
                .chain(glyphs.iter().map(|(_, g)| g))
                .map(|shape| shape_area(shape, world))
                .collect();
            Some((Area::union_all(&areas), Some(text.style.clone())))
        }
        NodeKind::Group(c) | NodeKind::Layer(c) => {
            let mut areas = Vec::new();
            let mut style = None;
            let mut clip = None;
            for &child in &c.children {
                let n = doc.nodes.get(&child)?;
                let inner = world * to_affine(n.transform);
                let (area, s) = covered(doc, fonts, n, inner)?;
                if c.clip == Some(child) {
                    clip = Some(area);
                } else if n.visible {
                    style = style.or(s);
                    areas.push(area);
                }
            }
            let shown = Area::union_all(&areas);
            let shown = match clip {
                Some(clip) => Area::combine(BooleanOp::Intersect, &[shown, clip]),
                None => shown,
            };
            Some((shown, style))
        }
        NodeKind::Image(_) => None,
    }
}

/// The fill of `shape` in document space.
fn shape_area(shape: &Shape, world: Affine) -> Area {
    let rule = match &shape.geometry {
        Geometry::Path(p) => p.fill_rule,
        _ => FillRule::NonZero,
    };
    Area::new(&(world * to_bezpath(&shape.geometry)), rule)
}

fn default_style() -> Style {
    Style {
        fill: Paint::Solid(vextor_core::Rgba::rgb(0, 0, 0)),
        stroke: None,
    }
}

/// Where a result goes: the place and node properties of the object it
/// replaces, and the map from document space into that object's space.
pub(super) struct Place {
    pub parent: NodeId,
    pub index: usize,
    pub name: Option<String>,
    pub opacity: f64,
    pub transform: Transform,
    pub to_local: Affine,
}

impl Place {
    /// The place of `id` (a layer or group child).
    pub(super) fn of(doc: &Document, id: NodeId) -> Option<Self> {
        let Some(Parent::Node(parent)) = doc.parent_of(id) else {
            return None;
        };
        let index = doc
            .nodes
            .get(&parent)?
            .container()?
            .children
            .iter()
            .position(|&c| c == id)?;
        let node = doc.nodes.get(&id)?;
        Some(Self {
            parent,
            index,
            name: node.name.clone(),
            opacity: node.opacity,
            transform: node.transform,
            to_local: to_affine(doc.world_transform(id)).inverse(),
        })
    }

    /// A new node `id` of `kind` with the replaced object's properties.
    pub(super) fn node(&self, id: NodeId, kind: NodeKind) -> Node {
        Node {
            name: self.name.clone(),
            opacity: self.opacity,
            transform: self.transform,
            ..Node::new(id, kind)
        }
    }
}

/// Fresh node ids for `s`, for nodes inserted in one command.
pub(super) fn ids(s: &Session) -> impl FnMut() -> NodeId + use<> {
    let mut next = s.doc.next_id();
    move || {
        let id = NodeId(next);
        next += 1;
        id
    }
}
