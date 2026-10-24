//! Offset Path, Simplify and Outline Stroke on each selected shape, in its
//! own space, one undo step each time.

use vextor_core::{
    Command, Container, FillRule, Geometry, LineJoin, NodeId, NodeKind, Paint, Shape, Style,
};
use vextor_geom::{offset, outline_stroke, path_to_bezpath, result_path, simplify, to_bezpath};

use super::operand::{Place, ids};
use super::{OFFSET, OUTLINE, SIMPLIFY};
use crate::session::Session;

/// What Offset Path asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OffsetSettings {
    /// Outward in px; negative insets.
    pub distance: f64,
    pub join: LineJoin,
    /// Miter length over the distance at which a miter becomes a bevel.
    pub miter_limit: f64,
}

impl Default for OffsetSettings {
    fn default() -> Self {
        Self {
            distance: 4.0,
            join: LineJoin::Miter,
            miter_limit: 4.0,
        }
    }
}

/// The selected shapes.
pub(super) fn shapes(s: &Session) -> Vec<(NodeId, &Shape)> {
    s.selected()
        .into_iter()
        .filter_map(|id| match &s.doc.nodes.get(&id)?.kind {
            NodeKind::Shape(shape) => Some((id, shape)),
            _ => None,
        })
        .collect()
}

/// True when `shape` paints a stroke.
pub(super) fn stroked(shape: &Shape) -> bool {
    shape
        .style
        .stroke
        .as_ref()
        .is_some_and(|st| st.width > 0.0 && st.paint != Paint::None)
}

fn fill_rule(g: &Geometry) -> FillRule {
    match g {
        Geometry::Path(p) => p.fill_rule,
        _ => FillRule::NonZero,
    }
}

/// Grows (or insets) every selected shape by `o`; each becomes a path.
///
/// # Errors
/// A message when an inset leaves nothing of a shape, or the change is
/// refused.
pub fn offset_selection(s: &mut Session, o: OffsetSettings) -> Result<(), String> {
    let mut items = Vec::new();
    for (id, shape) in shapes(s) {
        let outline = to_bezpath(&shape.geometry);
        let grown = offset(
            &outline,
            fill_rule(&shape.geometry),
            o.distance,
            o.join,
            o.miter_limit,
        );
        let path = result_path(&grown, FillRule::NonZero)
            .ok_or_else(|| format!("{OFFSET}: an inset this deep leaves nothing"))?;
        items.push((id, Geometry::Path(path)));
    }
    apply_geometry(s, OFFSET, items)
}

/// Simplifies every selected path within `tolerance` px. Returns the node
/// count before and after; a path with nothing to save is left alone.
///
/// # Errors
/// A message when the change is refused.
pub fn simplify_selection(s: &mut Session, tolerance: f64) -> Result<(usize, usize), String> {
    let (mut before, mut after) = (0, 0);
    let mut items = Vec::new();
    for (id, shape) in shapes(s) {
        let Geometry::Path(p) = &shape.geometry else {
            continue;
        };
        let nodes =
            |p: &vextor_core::Path| p.subpaths.iter().map(|sp| sp.nodes.len()).sum::<usize>();
        before += nodes(p);
        let outline = path_to_bezpath(p);
        let fewer = simplify(&outline, tolerance);
        match result_path(&fewer, p.fill_rule) {
            Some(q) if fewer.segments().count() < outline.segments().count() => {
                after += nodes(&q);
                items.push((id, Geometry::Path(q)));
            }
            _ => after += nodes(p),
        }
    }
    apply_geometry(s, SIMPLIFY, items)?;
    Ok((before, after))
}

fn apply_geometry(
    s: &mut Session,
    label: &'static str,
    items: Vec<(NodeId, Geometry)>,
) -> Result<(), String> {
    if items.is_empty() {
        return Ok(());
    }
    s.apply(Command::Batch {
        label,
        commands: vec![Command::SetGeometry { items }],
    })
    .map_err(|e| format!("{label} failed: {e}"))
}

/// Turns the stroke of every selected stroked shape into a filled path in
/// the stroke's paint. A shape that is also filled becomes a group of its
/// fill (no stroke) and the outlined stroke above it. The results are
/// selected.
///
/// # Errors
/// A message when a stroke paints nothing or the change is refused.
pub(super) fn outline_selection(s: &mut Session) -> Result<(), String> {
    let mut alloc = ids(s);
    let mut commands = Vec::new();
    let mut results = Vec::new();
    for (id, shape) in shapes(s) {
        let Some(stroke) = shape.style.stroke.as_ref().filter(|_| stroked(shape)) else {
            continue;
        };
        let place =
            Place::of(&s.doc, id).ok_or_else(|| format!("{OUTLINE}: nothing to replace"))?;
        let outline = outline_stroke(&to_bezpath(&shape.geometry), stroke);
        let path = result_path(&outline, FillRule::NonZero)
            .ok_or_else(|| format!("{OUTLINE}: the stroke paints nothing"))?;
        let outlined = Shape {
            geometry: Geometry::Path(path),
            style: Style {
                fill: stroke.paint,
                stroke: None,
            },
        };
        let top = alloc();
        let inserted = if shape.style.fill == Paint::None {
            vec![place.node(top, NodeKind::Shape(outlined))]
        } else {
            let (fill_id, stroke_id) = (alloc(), alloc());
            let fill = Shape {
                geometry: shape.geometry.clone(),
                style: Style {
                    fill: shape.style.fill,
                    stroke: None,
                },
            };
            let named = |id, name: &str, shape| vextor_core::Node {
                name: Some(name.to_owned()),
                ..vextor_core::Node::new(id, NodeKind::Shape(shape))
            };
            let group = place.node(
                top,
                NodeKind::Group(Container {
                    children: vec![fill_id, stroke_id],
                    clip: None,
                }),
            );
            vec![
                group,
                named(fill_id, "Fill", fill),
                named(stroke_id, "Stroke", outlined),
            ]
        };
        commands.push(Command::ReplaceWith {
            removed: vec![id],
            inserted,
            parent: Some(place.parent),
            index: place.index,
        });
        results.push(top);
    }
    if commands.is_empty() {
        return Err(format!("{OUTLINE}: select a shape with a stroke"));
    }
    s.apply(Command::Batch {
        label: OUTLINE,
        commands,
    })
    .map_err(|e| format!("{OUTLINE} failed: {e}"))?;
    s.select(results);
    Ok(())
}
