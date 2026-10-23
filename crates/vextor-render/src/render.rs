//! Rasterising a document region.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use tiny_skia::{Mask, Pixmap, PixmapPaint};
use vextor_core::{
    Document, EPS, Geometry, NodeId, NodeKind, Paint, Rect, Resource, ResourceId, Shape, Style,
    Text, Transform, View,
};
use vextor_geom::{Fonts, text_box, to_bezpath};

use crate::error::RenderError;

mod pattern;

use self::pattern::MAX_TILES;
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
pub fn render(doc: &Document, fonts: &Fonts, viewport: &Viewport) -> Result<Pixmap, RenderError> {
    viewport.check()?;
    let mut pixmap = new_pixmap(viewport.width, viewport.height)?;
    draw(doc, fonts, viewport, &mut pixmap);
    Ok(pixmap)
}

/// Like [`render`], reusing `pixmap` when it already has the viewport's size.
///
/// # Errors
/// [`RenderError`] when the viewport is empty, too large or invalid.
pub fn render_into(
    doc: &Document,
    fonts: &Fonts,
    viewport: &Viewport,
    pixmap: &mut Pixmap,
) -> Result<(), RenderError> {
    viewport.check()?;
    if pixmap.width() == viewport.width && pixmap.height() == viewport.height {
        pixmap.fill(tiny_skia::Color::TRANSPARENT);
    } else {
        *pixmap = new_pixmap(viewport.width, viewport.height)?;
    }
    draw(doc, fonts, viewport, pixmap);
    Ok(())
}

fn new_pixmap(width: u32, height: u32) -> Result<Pixmap, RenderError> {
    Pixmap::new(width, height).ok_or(RenderError::Empty { width, height })
}

fn draw(doc: &Document, fonts: &Fonts, viewport: &Viewport, target: &mut Pixmap) {
    let base = viewport.to_pixels();
    draw_artboard_backgrounds(doc, base, target);
    let scene = Scene::new(doc, fonts);
    for &id in &doc.layers {
        scene.draw_node(id, base, target, 0);
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

/// A rendered pattern tile and the transform from its pixels to the
/// painted object's own coordinates.
type Tile = Rc<(Pixmap, Transform)>;

/// What one render draws from, and the pattern tiles drawn so far (each
/// pattern is drawn once per scale per render, however many objects use
/// it).
struct Scene<'a> {
    doc: &'a Document,
    fonts: &'a Fonts,
    /// How many pattern tiles deep this scene draws (guards patterns that
    /// contain themselves).
    nesting: usize,
    tiles: RefCell<BTreeMap<(ResourceId, u64, u64), Option<Tile>>>,
    /// Tiles this render may still draw, shared with nested scenes.
    budget: Rc<Cell<usize>>,
}

impl<'a> Scene<'a> {
    fn new(doc: &'a Document, fonts: &'a Fonts) -> Self {
        Self {
            doc,
            fonts,
            nesting: 0,
            tiles: RefCell::default(),
            budget: Rc::new(Cell::new(MAX_TILES)),
        }
    }

    /// Draws one node and its subtree. `parent` maps the parent's
    /// coordinates to pixmap pixels.
    fn draw_node(&self, id: NodeId, parent: Transform, target: &mut Pixmap, depth: usize) {
        let Some(node) = self.doc.nodes.get(&id) else {
            return;
        };
        if !node.visible || node.opacity <= EPS || depth > MAX_DEPTH {
            return;
        }
        let world = node.transform.then(parent);
        let clip = node.container().and_then(|c| c.clip);
        if node.opacity >= 1.0 - EPS && clip.is_none() {
            self.draw_content(id, &node.kind, world, target, depth);
            return;
        }
        // Group opacity and clips are applied to the subtree as a whole, so
        // it is drawn into its own layer first.
        let Some(mut layer) = Pixmap::new(target.width(), target.height()) else {
            return;
        };
        self.draw_content(id, &node.kind, world, &mut layer, depth);
        if let Some(clip) = clip {
            apply_clip(self.doc, clip, world, &mut layer);
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
        &self,
        id: NodeId,
        kind: &NodeKind,
        world: Transform,
        target: &mut Pixmap,
        depth: usize,
    ) {
        match kind {
            NodeKind::Layer(c) | NodeKind::Group(c) => {
                for &child in &c.children {
                    if Some(child) != c.clip {
                        self.draw_node(child, world, target, depth + 1);
                    }
                }
            }
            NodeKind::Shape(shape) => self.draw_shape(shape, world, target),
            NodeKind::Text(text) => self.draw_text(id, text, world, target),
            // Embedded images are drawn from a later spec.
            NodeKind::Image(_) => {}
        }
    }

    fn draw_shape(&self, shape: &Shape, world: Transform, target: &mut Pixmap) {
        let bez = to_bezpath(&shape.geometry);
        let Some(path) = paint::path(&bez) else {
            return;
        };
        let rule = paint::fill_rule(&shape.geometry);
        self.fill_and_stroke(target, &path, &bez, &shape.style, rule, world);
    }

    /// Draws a text: its frame (background, then border) behind the glyph
    /// outlines, which are filled and stroked like a shape (non-zero rule).
    fn draw_text(&self, id: NodeId, text: &Text, world: Transform, target: &mut Pixmap) {
        let layout = self.fonts.layout(id, text);
        let winding = tiny_skia::FillRule::Winding;
        if let Some(frame) = &text.frame {
            let b = text_box(text, &layout);
            let geometry = Geometry::Rect {
                rect: Rect::new(b.x0, b.y0, b.width(), b.height()),
                radius: frame.radius,
            };
            let bez = to_bezpath(&geometry);
            if let Some(path) = paint::path(&bez) {
                let style = Style {
                    fill: frame.fill,
                    stroke: frame.stroke.clone(),
                };
                self.fill_and_stroke(target, &path, &bez, &style, winding, world);
            }
        }
        if let Some(path) = paint::path(&layout.glyphs) {
            self.fill_and_stroke(target, &path, &layout.glyphs, &text.style, winding, world);
        }
    }

    /// Fills `path` (`bez` in the object's own coordinates), then strokes
    /// it, as SVG paints a shape.
    fn fill_and_stroke(
        &self,
        target: &mut Pixmap,
        path: &tiny_skia::Path,
        bez: &kurbo::BezPath,
        style: &Style,
        rule: tiny_skia::FillRule,
        world: Transform,
    ) {
        // Only bounding-box gradients need the box (spec 09); solid paints,
        // most of a document, skip it.
        let gradient = |p: Paint| matches!(p, Paint::Gradient(_));
        let bbox =
            if gradient(style.fill) || style.stroke.as_ref().is_some_and(|s| gradient(s.paint)) {
                kurbo::Shape::bounding_box(bez)
            } else {
                kurbo::Rect::ZERO
            };
        let ts = paint::transform(world);
        self.with_paint(style.fill, bbox, world, |p| {
            target.fill_path(path, p, rule, ts, None);
        });
        if let Some(stroke) = &style.stroke
            && stroke.width > EPS
        {
            let s = paint::stroke(stroke);
            self.with_paint(stroke.paint, bbox, world, |p| {
                target.stroke_path(path, p, &s, ts, None);
            });
        }
    }

    /// Calls `draw` with the tiny-skia paint for `paint` on an object whose
    /// geometry bounds are `bbox` and whose coordinates `world` maps to
    /// pixels; not at all when it paints nothing.
    fn with_paint(
        &self,
        paint: Paint,
        bbox: kurbo::Rect,
        world: Transform,
        draw: impl FnOnce(&tiny_skia::Paint<'_>),
    ) {
        match paint {
            Paint::Gradient(id) => {
                if let Some(Resource::Gradient(g)) = self.doc.resources.get(&id)
                    && let Some(shader) = paint::gradient(g, bbox)
                {
                    draw(&paint::shaded(shader));
                }
            }
            Paint::Pattern(id) => {
                if let Some(tile) = self.pattern_tile(id, world) {
                    let (pixmap, ts) = &*tile;
                    let shader = tiny_skia::Pattern::new(
                        pixmap.as_ref(),
                        tiny_skia::SpreadMode::Repeat,
                        tiny_skia::FilterQuality::Bicubic,
                        1.0,
                        paint::transform(*ts),
                    );
                    draw(&paint::shaded(shader));
                }
            }
            solid => {
                if let Some(p) = paint::solid(solid) {
                    draw(&p);
                }
            }
        }
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
            render(&doc, &Fonts::default(), &viewport(0, 10)),
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
            render(&doc, &Fonts::default(), &viewport(MAX_PIXMAP_SIDE + 1, 1)),
            Err(RenderError::TooLarge { .. })
        ));
    }

    #[test]
    fn nan_zoom_is_an_error() {
        let doc = Document::new("t");
        let mut vp = viewport(4, 4);
        vp.view.zoom = f64::NAN;
        assert_eq!(
            render(&doc, &Fonts::default(), &vp),
            Err(RenderError::InvalidView)
        );
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
        render_into(&doc, &Fonts::default(), &viewport(8, 8), &mut pixmap).expect("valid viewport");
        assert!(
            pixmap.data().iter().all(|&b| b == 0),
            "cleared to transparent"
        );
    }
}
