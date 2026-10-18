//! Rasterising a document region.

use tiny_skia::{Mask, Pixmap, PixmapPaint};
use vextor_core::{Container, Document, EPS, NodeId, NodeKind, Shape, Transform, View};
use vextor_geom::to_bezpath;

use crate::error::RenderError;
use crate::paint;

/// Largest pixmap side rendered, in physical pixels.
pub const MAX_PIXMAP_SIDE: u32 = 16_384;

/// Deeper nesting than this is not drawn (guards the recursion).
const MAX_DEPTH: usize = 256;

/// The region of the document to draw: the canvas view plus the size of
/// the canvas in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// Zoom and pan, in logical points (see [`View`]).
    pub view: View,
    /// Physical pixels.
    pub width: u32,
    /// Physical pixels.
    pub height: u32,
    /// Physical pixels per logical point (2.0 on a Retina screen).
    pub pixels_per_point: f64,
}

impl Viewport {
    /// Document space → pixmap pixels.
    #[must_use]
    pub fn to_pixels(&self) -> Transform {
        let s = self.view.zoom * self.pixels_per_point;
        let p = self.view.pan.scale(self.pixels_per_point);
        Transform([s, 0.0, 0.0, s, p.x, p.y])
    }

    fn check(&self) -> Result<(), RenderError> {
        let (width, height) = (self.width, self.height);
        if width == 0 || height == 0 {
            return Err(RenderError::Empty { width, height });
        }
        if width > MAX_PIXMAP_SIDE || height > MAX_PIXMAP_SIDE {
            return Err(RenderError::TooLarge { width, height });
        }
        let positive = |v: f64| v.is_finite() && v > 0.0;
        if !positive(self.view.zoom)
            || !positive(self.pixels_per_point)
            || !self.view.pan.is_finite()
        {
            return Err(RenderError::InvalidView);
        }
        Ok(())
    }
}

/// Draws `doc` as seen through `viewport` into a new transparent pixmap.
///
/// # Errors
/// [`RenderError`] when the viewport is empty, too large or invalid.
pub fn render(doc: &Document, viewport: &Viewport) -> Result<Pixmap, RenderError> {
    viewport.check()?;
    let mut pixmap = new_pixmap(viewport.width, viewport.height)?;
    draw(doc, viewport, &mut pixmap);
    Ok(pixmap)
}

/// Like [`render`], reusing `pixmap` when it already has the viewport's size.
///
/// # Errors
/// [`RenderError`] when the viewport is empty, too large or invalid.
pub fn render_into(
    doc: &Document,
    viewport: &Viewport,
    pixmap: &mut Pixmap,
) -> Result<(), RenderError> {
    viewport.check()?;
    if pixmap.width() == viewport.width && pixmap.height() == viewport.height {
        pixmap.fill(tiny_skia::Color::TRANSPARENT);
    } else {
        *pixmap = new_pixmap(viewport.width, viewport.height)?;
    }
    draw(doc, viewport, pixmap);
    Ok(())
}

fn new_pixmap(width: u32, height: u32) -> Result<Pixmap, RenderError> {
    Pixmap::new(width, height).ok_or(RenderError::Empty { width, height })
}

fn draw(doc: &Document, viewport: &Viewport, target: &mut Pixmap) {
    let base = viewport.to_pixels();
    draw_artboard_backgrounds(doc, base, target);
    for &id in &doc.layers {
        draw_node(doc, id, base, target, 0);
    }
}

fn draw_artboard_backgrounds(doc: &Document, base: Transform, target: &mut Pixmap) {
    for artboard in &doc.artboards {
        let Some(color) = artboard.background else {
            continue;
        };
        let (Some(rect), Some(fill)) = (
            paint::rect(artboard.rect),
            paint::solid(vextor_core::Paint::Solid(color)),
        ) else {
            continue;
        };
        target.fill_rect(rect, &fill, paint::transform(base), None);
    }
}

/// Draws one node and its subtree. `parent` maps the parent's coordinates
/// to pixmap pixels.
fn draw_node(doc: &Document, id: NodeId, parent: Transform, target: &mut Pixmap, depth: usize) {
    let Some(node) = doc.nodes.get(&id) else {
        return;
    };
    if !node.visible || node.opacity <= EPS || depth > MAX_DEPTH {
        return;
    }
    let world = node.transform.then(parent);
    let clip = node.container().and_then(|c| c.clip);
    if node.opacity >= 1.0 - EPS && clip.is_none() {
        draw_content(doc, &node.kind, world, target, depth);
        return;
    }
    // Group opacity and clips are applied to the subtree as a whole, so it
    // is drawn into its own layer first.
    let Some(mut layer) = Pixmap::new(target.width(), target.height()) else {
        return;
    };
    draw_content(doc, &node.kind, world, &mut layer, depth);
    if let Some(clip) = clip {
        apply_clip(doc, clip, world, &mut layer);
    }
    #[allow(clippy::cast_possible_truncation)] // opacity is in 0..=1
    let paint = PixmapPaint {
        opacity: node.opacity as f32,
        ..PixmapPaint::default()
    };
    target.draw_pixmap(
        0,
        0,
        layer.as_ref(),
        &paint,
        tiny_skia::Transform::identity(),
        None,
    );
}

fn draw_content(
    doc: &Document,
    kind: &NodeKind,
    world: Transform,
    target: &mut Pixmap,
    depth: usize,
) {
    match kind {
        NodeKind::Layer(c) | NodeKind::Group(c) => draw_children(doc, c, world, target, depth),
        NodeKind::Shape(shape) => draw_shape(shape, world, target),
        // Embedded images are drawn from a later spec.
        NodeKind::Image(_) => {}
    }
}

fn draw_children(
    doc: &Document,
    c: &Container,
    world: Transform,
    target: &mut Pixmap,
    depth: usize,
) {
    for &child in &c.children {
        if Some(child) != c.clip {
            draw_node(doc, child, world, target, depth + 1);
        }
    }
}

fn draw_shape(shape: &Shape, world: Transform, target: &mut Pixmap) {
    let Some(path) = paint::path(&to_bezpath(&shape.geometry)) else {
        return;
    };
    let ts = paint::transform(world);
    if let Some(fill) = paint::solid(shape.style.fill) {
        target.fill_path(&path, &fill, paint::fill_rule(&shape.geometry), ts, None);
    }
    if let Some(stroke) = &shape.style.stroke
        && stroke.width > EPS
        && let Some(color) = paint::solid(stroke.paint)
    {
        target.stroke_path(&path, &color, &paint::stroke(stroke), ts, None);
    }
}

/// Keeps only the part of `layer` inside the clip shape.
fn apply_clip(doc: &Document, clip: NodeId, container: Transform, layer: &mut Pixmap) {
    let mut mask = Mask::new(layer.width(), layer.height());
    if let (Some(mask), Some(node)) = (mask.as_mut(), doc.nodes.get(&clip))
        && let NodeKind::Shape(shape) = &node.kind
        && let Some(path) = paint::path(&to_bezpath(&shape.geometry))
    {
        let ts = paint::transform(node.transform.then(container));
        mask.fill_path(&path, paint::fill_rule(&shape.geometry), true, ts);
    }
    match mask {
        Some(mask) => layer.apply_mask(&mask),
        // A clip that cannot be built hides the content, as SVG does.
        None => layer.fill(tiny_skia::Color::TRANSPARENT),
    }
}

#[cfg(test)]
mod tests {
    use vextor_core::Vec2;

    use super::*;

    fn viewport(width: u32, height: u32) -> Viewport {
        Viewport {
            view: View::default(),
            width,
            height,
            pixels_per_point: 1.0,
        }
    }

    #[test]
    fn empty_viewport_is_an_error() {
        let doc = Document::new("t");
        assert_eq!(
            render(&doc, &viewport(0, 10)),
            Err(RenderError::Empty {
                width: 0,
                height: 10
            })
        );
    }

    #[test]
    fn oversized_viewport_is_an_error() {
        let doc = Document::new("t");
        assert!(matches!(
            render(&doc, &viewport(MAX_PIXMAP_SIDE + 1, 1)),
            Err(RenderError::TooLarge { .. })
        ));
    }

    #[test]
    fn nan_zoom_is_an_error() {
        let doc = Document::new("t");
        let mut vp = viewport(4, 4);
        vp.view.zoom = f64::NAN;
        assert_eq!(render(&doc, &vp), Err(RenderError::InvalidView));
    }

    #[test]
    fn to_pixels_scales_zoom_and_pan_by_density() {
        let vp = Viewport {
            view: View {
                zoom: 3.0,
                pan: Vec2::new(10.0, 20.0),
            },
            width: 1,
            height: 1,
            pixels_per_point: 2.0,
        };
        assert_eq!(vp.to_pixels().0, [6.0, 0.0, 0.0, 6.0, 20.0, 40.0]);
    }

    #[test]
    fn render_into_reuses_a_same_size_pixmap() {
        let doc = Document::new("t");
        let mut pixmap = Pixmap::new(8, 8).expect("8×8 is a valid size");
        pixmap.fill(tiny_skia::Color::WHITE);
        render_into(&doc, &viewport(8, 8), &mut pixmap).expect("valid viewport");
        assert!(
            pixmap.data().iter().all(|&b| b == 0),
            "cleared to transparent"
        );
    }
}
