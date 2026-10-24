//! Object ▸ Image to Vector… (spec 13b): one selected image becomes a
//! group of filled paths, one per colour (or one stroked path, Outlines
//! only), in the image's place, as one undo step. Tracing itself runs on a
//! worker thread (`trace_dialog`); this module holds what the worker runs
//! and the command that puts the result in the document.

use kurbo::Affine;
use vextor_core::{
    Command, Container, Document, Geometry, ImageMime, ImageSource, LineCap, LineJoin, Node,
    NodeId, NodeKind, Paint, Parent, Rect, Rgba, Shape, Stroke, Style,
};
use vextor_geom::{Pixels, TraceMode, TracedColor, bezpath_to_path, trace};

use crate::session::Session;

/// Undo label of the conversion.
pub const LABEL: &str = "Image to vector";
/// Why the command is disabled.
pub const SELECT_ONE_IMAGE: &str = "Select one image";
/// Outlines only: the line width as a share of the image's longer side, so
/// lines look the same whatever the image's size.
const OUTLINE_SHARE: f64 = 0.003;
/// ... and never thinner than this, in px.
const MIN_OUTLINE_WIDTH: f64 = 0.5;
/// The group's name when the image has none.
const UNNAMED: &str = "Traced image";

/// The dialog's choices, in order: mode, name, tooltip.
pub const MODES: [(TraceMode, &str, &str); 4] = [
    (
        TraceMode::BlackAndWhite,
        "Black and white",
        "Black shapes where the image is dark",
    ),
    (
        TraceMode::Colors3,
        "3 colours",
        "The image's 3 main colours",
    ),
    (
        TraceMode::Colors6,
        "6 colours",
        "The image's 6 main colours",
    ),
    (
        TraceMode::Outlines,
        "Outlines only",
        "Black lines along the edges between the image's 6 main colours",
    ),
];

/// What a trace needs from the image, copied so the worker owns it.
#[derive(Clone, Debug)]
pub struct Source {
    pub image: NodeId,
    pub mime: ImageMime,
    pub bytes: Vec<u8>,
    pub rect: Rect,
}

/// The one selected image, when it can be traced: visible, unlocked, and
/// not inside a hidden or locked group or layer. Cheap: menus and the
/// inspector ask every frame.
///
/// # Errors
/// [`SELECT_ONE_IMAGE`] otherwise.
pub fn traceable(s: &Session) -> Result<NodeId, &'static str> {
    let [id] = s.selected()[..] else {
        return Err(SELECT_ONE_IMAGE);
    };
    let is_image = matches!(
        s.doc.nodes.get(&id).map(|n| &n.kind),
        Some(NodeKind::Image(_))
    );
    if !is_image || !shown_and_unlocked(&s.doc, id) {
        return Err(SELECT_ONE_IMAGE);
    }
    Ok(id)
}

/// The [`traceable`] image, its bytes copied for the worker.
///
/// # Errors
/// [`SELECT_ONE_IMAGE`] when there is none.
pub fn source(s: &Session) -> Result<Source, &'static str> {
    let id = traceable(s)?;
    let Some(Node {
        kind: NodeKind::Image(image),
        ..
    }) = s.doc.nodes.get(&id)
    else {
        return Err(SELECT_ONE_IMAGE);
    };
    let ImageSource::Embedded { mime, bytes } = &image.source;
    Ok(Source {
        image: id,
        mime: *mime,
        bytes: bytes.clone(),
        rect: image.rect,
    })
}

/// True when `id` and everything holding it are visible and unlocked.
fn shown_and_unlocked(doc: &Document, mut id: NodeId) -> bool {
    loop {
        match doc.nodes.get(&id) {
            Some(n) if n.visible && !n.locked => {}
            _ => return false,
        }
        match doc.parent_of(id) {
            Some(Parent::Node(parent)) => id = parent,
            Some(Parent::Root) => return true,
            _ => return false,
        }
    }
}

/// Every mode's shapes, `first` first, each handed to `found` as soon as it
/// is ready (the worker sends them to the dialog). The image is decoded
/// once.
pub fn trace_all(
    src: &Source,
    first: TraceMode,
    mut found: impl FnMut(TraceMode, Result<Vec<Shape>, String>) -> bool,
) {
    let Some(px) = vextor_render::trace_pixels(src.mime, &src.bytes) else {
        for (mode, ..) in MODES {
            found(mode, Err("Couldn't read this image".to_owned()));
        }
        return;
    };
    let pixels = Pixels {
        width: px.width as usize,
        height: px.height as usize,
        rgba: &px.rgba,
    };
    let to_rect = to_rect(src.rect, px.width, px.height);
    let mut order: Vec<TraceMode> = vec![first];
    order.extend(MODES.iter().map(|m| m.0).filter(|&m| m != first));
    for mode in order {
        let traced = trace(pixels, mode).map_err(|e| e.to_string());
        let shapes = traced.and_then(|t| shapes(&t, to_rect, mode, src.rect));
        if !found(mode, shapes) {
            return; // nobody is waiting any more
        }
    }
}

/// From the traced pixels (`width` × `height`) to the image's rect.
fn to_rect(rect: Rect, width: u32, height: u32) -> Affine {
    Affine::translate((rect.x, rect.y))
        * Affine::scale_non_uniform(
            rect.width / f64::from(width.max(1)),
            rect.height / f64::from(height.max(1)),
        )
}

/// The traced colours as shapes in the image's space: filled solid, or for
/// outlines a black round stroke sized to `image` and no fill.
///
/// # Errors
/// A message when there is nothing to trace.
pub fn shapes(
    traced: &[TracedColor],
    to_rect: Affine,
    mode: TraceMode,
    image: Rect,
) -> Result<Vec<Shape>, String> {
    if traced.is_empty() {
        return Err("Nothing to trace in this image".to_owned());
    }
    Ok(traced
        .iter()
        .map(|t| {
            let path = bezpath_to_path(&(to_rect * &t.path), t.fill_rule);
            let style = if mode == TraceMode::Outlines {
                outline_style(image)
            } else {
                Style {
                    fill: Paint::Solid(t.color),
                    stroke: None,
                }
            };
            Shape {
                geometry: Geometry::Path(path),
                style,
            }
        })
        .collect())
}

/// Outlines only: black, [`OUTLINE_SHARE`] of `image`'s longer side wide,
/// round caps and joins, no fill.
fn outline_style(image: Rect) -> Style {
    let width = (image.width.max(image.height) * OUTLINE_SHARE).max(MIN_OUTLINE_WIDTH);
    Style {
        fill: Paint::None,
        stroke: Some(Stroke {
            paint: Paint::Solid(Rgba::BLACK),
            // two decimals: a clean number in the exported SVG
            width: (width * 100.0).round() / 100.0,
            cap: LineCap::Round,
            join: LineJoin::Round,
            miter_limit: 4.0,
            dash: Vec::new(),
            dash_offset: 0.0,
        }),
    }
}

/// Path nodes in `shapes`.
#[must_use]
pub fn node_count(shapes: &[Shape]) -> usize {
    shapes
        .iter()
        .map(|s| match &s.geometry {
            Geometry::Path(p) => p.subpaths.iter().map(|sp| sp.nodes.len()).sum(),
            _ => 0,
        })
        .sum()
}

/// Puts `shapes` in a group in `image`'s place (same transform and
/// opacity) and selects it, as one undo step. With `keep`, the image stays
/// directly under the group, hidden; otherwise it is removed.
///
/// # Errors
/// A message when the image is gone or the change is refused.
pub fn apply(s: &mut Session, image: NodeId, shapes: Vec<Shape>, keep: bool) -> Result<(), String> {
    let gone = || format!("{LABEL}: the image is gone");
    let Some(Parent::Node(parent)) = s.doc.parent_of(image) else {
        return Err(gone());
    };
    let index = s
        .doc
        .nodes
        .get(&parent)
        .and_then(Node::container)
        .and_then(|c| c.children.iter().position(|&c| c == image))
        .ok_or_else(gone)?;
    let img = s.doc.nodes.get(&image).ok_or_else(gone)?.clone();

    let mut next = s.doc.next_id();
    let mut alloc = || {
        let id = NodeId(next);
        next += 1;
        id
    };
    let group_id = alloc();
    let children: Vec<Node> = shapes
        .into_iter()
        .map(|shape| {
            let name = match (&shape.style.fill, &shape.style.stroke) {
                (Paint::Solid(c), _) => c.to_hex(),
                _ => "Outlines".to_owned(),
            };
            Node {
                name: Some(name),
                ..Node::new(alloc(), NodeKind::Shape(shape))
            }
        })
        .collect();
    let group = Node {
        name: Some(img.name.clone().unwrap_or_else(|| UNNAMED.to_owned())),
        opacity: img.opacity,
        transform: img.transform,
        ..Node::new(
            group_id,
            NodeKind::Group(Container {
                children: children.iter().map(|n| n.id).collect(),
                clip: None,
            }),
        )
    };
    let mut nodes = vec![group];
    nodes.extend(children);

    let commands = if keep {
        vec![
            Command::SetProps {
                id: image,
                name: img.name.clone(),
                visible: false,
                locked: img.locked,
                opacity: img.opacity,
            },
            Command::Insert {
                parent: Some(parent),
                index: index + 1,
                nodes,
            },
        ]
    } else {
        vec![Command::ReplaceWith {
            removed: vec![image],
            inserted: nodes,
            parent: Some(parent),
            index,
        }]
    };
    s.apply(Command::Batch {
        label: LABEL,
        commands,
    })
    .map_err(|e| format!("{LABEL} failed: {e}"))?;
    s.select(vec![group_id]);
    Ok(())
}

#[cfg(test)]
mod tests;
