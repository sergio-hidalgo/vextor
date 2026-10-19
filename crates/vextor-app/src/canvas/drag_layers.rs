//! Fast drag (spec 05b): while the selection is moved, scaled or rotated,
//! the canvas does not render the document. When the button goes down on
//! the selection it renders the press-time document once as three layers
//! (below the selection, the selection, above it) and then draws those
//! textures every frame, the selection one through the drag's affine —
//! except while scaling, when the (small) selection layer is rendered again
//! each frame so strokes keep their width. Release renders sharp again.
//!
//! Only the bottom layer covers the whole canvas; the selection and above
//! layers are cropped to what they draw (above is skipped when empty), so
//! the start of a drag renders and uploads little more than one frame.

use std::collections::BTreeSet;

use eframe::egui::{self, Color32, ColorImage, Pos2, TextureHandle, TextureOptions};
use kurbo::{Affine, Rect};
use vextor_core::{Document, NodeId, NodeKind, Paint, View};
use vextor_render::{RenderError, Viewport, render};

use super::Canvas;

/// Extra room around a cropped layer, in points (anti-aliasing).
const CROP_MARGIN: f64 = 2.0;

/// One rendered layer: its texture and the canvas area it covers, in points
/// relative to the canvas, for the view it was rendered at.
struct Layer {
    texture: TextureHandle,
    area: Rect,
}

/// The layers of one drag, rendered for `viewport`.
pub(super) struct DragLayers {
    below: Layer,
    selection: Option<Layer>,
    above: Option<Layer>,
    viewport: Viewport,
}

impl Canvas {
    /// Draws the drag layers while the button is down on the selection
    /// (building them on the press frame). Returns false when the document
    /// must be drawn the normal way.
    pub(super) fn draw_drag_layers(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        rect: egui::Rect,
        viewport: &Viewport,
        current: &Document,
    ) -> bool {
        let scaling = self.tool.scaling().is_some();
        let Some(preview) = self.tool.preview() else {
            self.drag_layers = None;
            return false;
        };
        let stale = self.drag_layers.as_ref().is_none_or(|l| {
            l.viewport.width != viewport.width || l.viewport.height != viewport.height
        });
        if stale {
            // A failed render falls back to the normal path (`None`).
            self.drag_layers = DragLayers::build(ctx, preview.before, preview.ids, viewport).ok();
        }
        let Some(layers) = &mut self.drag_layers else {
            return false;
        };
        if scaling {
            // A scale changes the shapes, not just where they are: the
            // selection is rendered again from the live document (its
            // layer is small) so strokes keep their width while dragging.
            let objects = top_level(current);
            let selected: BTreeSet<NodeId> = preview.ids.iter().copied().collect();
            let only = keep_only(current, &objects, |id| selected.contains(id), false);
            layers.selection = layer(ctx, "drag-selection", &only, viewport, Some(&objects))
                .ok()
                .flatten();
            layers.draw(painter, rect.min, viewport.view, None);
        } else {
            layers.draw(painter, rect.min, viewport.view, Some(preview.change));
        }
        true
    }
}

impl DragLayers {
    fn build(
        ctx: &egui::Context,
        before: &Document,
        ids: &[NodeId],
        viewport: &Viewport,
    ) -> Result<Self, RenderError> {
        let [below, selection, above] = split(before, ids);
        let objects = top_level(before);
        Ok(Self {
            below: layer(ctx, "drag-below", &below, viewport, None)?.ok_or(RenderError::Empty {
                width: viewport.width,
                height: viewport.height,
            })?,
            selection: layer(ctx, "drag-selection", &selection, viewport, Some(&objects))?,
            above: layer(ctx, "drag-above", &above, viewport, Some(&objects))?,
            viewport: *viewport,
        })
    }

    /// Draws the layers for view `now`, the selection moved by `change`
    /// (document space). `origin` is the canvas's top-left corner. A live
    /// selection layer (`change` = `None`) was rendered for `now` as it is.
    fn draw(&self, painter: &egui::Painter, origin: Pos2, now: View, change: Option<Affine>) {
        let rendered = to_screen(self.viewport.view);
        let back = to_screen(now) * rendered.inverse();
        let moved = change.map_or(Affine::IDENTITY, |c| {
            to_screen(now) * c * rendered.inverse()
        });
        painter.add(quad(&self.below, origin, back));
        if let Some(l) = &self.selection {
            painter.add(quad(l, origin, moved));
        }
        if let Some(l) = &self.above {
            painter.add(quad(l, origin, back));
        }
    }
}

/// Renders `doc` into a texture. With `crop` (the top-level objects), only
/// the canvas area its visible objects cover is rendered, or nothing when
/// none is visible on the canvas; without, the whole canvas.
fn layer(
    ctx: &egui::Context,
    name: &str,
    doc: &Document,
    viewport: &Viewport,
    crop: Option<&[NodeId]>,
) -> Result<Option<Layer>, RenderError> {
    let ppp = viewport.pixels_per_point;
    let canvas = Rect::new(
        0.0,
        0.0,
        f64::from(viewport.width) / ppp,
        f64::from(viewport.height) / ppp,
    );
    let area = match crop {
        None => canvas,
        Some(objects) => {
            let visible: Vec<NodeId> = objects
                .iter()
                .copied()
                .filter(|id| doc.nodes.get(id).is_some_and(|n| n.visible))
                .collect();
            let Some(b) = vextor_geom::union_bounds(doc, &visible) else {
                return Ok(None);
            };
            let margin = stroke_reach(doc, &visible) * viewport.view.zoom + CROP_MARGIN;
            let on_screen = to_screen(viewport.view)
                .transform_rect_bbox(b)
                .inflate(margin, margin);
            // Whole device pixels, inside the canvas.
            let snapped = Rect::new(
                (on_screen.x0 * ppp).floor() / ppp,
                (on_screen.y0 * ppp).floor() / ppp,
                (on_screen.x1 * ppp).ceil() / ppp,
                (on_screen.y1 * ppp).ceil() / ppp,
            )
            .intersect(canvas);
            if snapped.width() <= 0.0 || snapped.height() <= 0.0 {
                return Ok(None);
            }
            snapped
        }
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // canvas sizes
    let cropped = Viewport {
        view: View {
            zoom: viewport.view.zoom,
            pan: vextor_core::Vec2::new(
                viewport.view.pan.x - area.x0,
                viewport.view.pan.y - area.y0,
            ),
        },
        width: (area.width() * ppp).round() as u32,
        height: (area.height() * ppp).round() as u32,
        pixels_per_point: ppp,
    };
    let pixmap = render(doc, &cropped)?;
    let size = [pixmap.width() as usize, pixmap.height() as usize];
    let image = ColorImage::from_rgba_premultiplied(size, pixmap.data());
    Ok(Some(Layer {
        texture: ctx.load_texture(name, image, TextureOptions::LINEAR),
        area,
    }))
}

/// How far strokes of `ids` (and their descendants) reach past the
/// geometric bounds, in document px: half the width, times the miter limit
/// for mitred joins, scaled by the node's transform.
fn stroke_reach(doc: &Document, ids: &[NodeId]) -> f64 {
    // Top-down, so each node's world transform costs one multiplication.
    fn walk(doc: &Document, id: NodeId, parent: Affine, depth: usize) -> f64 {
        let Some(node) = doc.nodes.get(&id) else {
            return 0.0;
        };
        let world = parent * vextor_geom::to_affine(node.transform);
        match &node.kind {
            NodeKind::Shape(s) => s
                .style
                .stroke
                .as_ref()
                .filter(|st| st.paint != Paint::None)
                .map_or(0.0, |st| {
                    let scale = world.determinant().abs().sqrt();
                    st.width / 2.0 * st.miter_limit.max(1.0) * scale
                }),
            NodeKind::Layer(c) | NodeKind::Group(c) if depth < 64 => c
                .children
                .iter()
                .map(|&child| walk(doc, child, world, depth + 1))
                .fold(0.0, f64::max),
            _ => 0.0,
        }
    }
    ids.iter()
        .map(|&id| {
            let parent = vextor_geom::to_affine(doc.parent_transform(id));
            walk(doc, id, parent, 0)
        })
        .fold(0.0, f64::max)
}

/// Document → canvas points for a view.
fn to_screen(v: View) -> Affine {
    Affine::translate((v.pan.x, v.pan.y)) * Affine::scale(v.zoom)
}

/// A layer's texture over its area, mapped through `a`. Two triangles map
/// an affine image exactly.
fn quad(layer: &Layer, origin: Pos2, a: Affine) -> egui::Shape {
    let r = layer.area;
    let mut mesh = egui::Mesh::with_texture(layer.texture.id());
    let corners = [(r.x0, r.y0), (r.x1, r.y0), (r.x1, r.y1), (r.x0, r.y1)];
    let uvs = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
    for ((x, y), (u, v)) in corners.into_iter().zip(uvs) {
        let p = a * kurbo::Point::new(x, y);
        #[allow(clippy::cast_possible_truncation)] // screen coordinates
        let pos = origin + egui::vec2(p.x as f32, p.y as f32);
        mesh.vertices.push(egui::epaint::Vertex {
            pos,
            uv: egui::pos2(u, v),
            color: Color32::WHITE,
        });
    }
    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
    egui::Shape::mesh(mesh)
}

/// The children of every layer, in paint order.
fn top_level(doc: &Document) -> Vec<NodeId> {
    doc.layers
        .iter()
        .filter_map(|l| doc.nodes.get(l)?.container())
        .flat_map(|c| c.children.iter().copied())
        .collect()
}

/// The press-time document as three documents to render: unselected
/// objects below the topmost selected object (with the artboard
/// backgrounds), the selected objects, and unselected objects above. The
/// top-level objects are the children of every layer, in paint order.
pub(super) fn split(doc: &Document, ids: &[NodeId]) -> [Document; 3] {
    let objects = top_level(doc);
    let selected: BTreeSet<NodeId> = ids.iter().copied().collect();
    let top = objects.iter().rposition(|o| selected.contains(o));
    let above: BTreeSet<NodeId> = top
        .map(|t| objects[t + 1..].iter().copied().collect())
        .unwrap_or_default();
    [
        keep_only(
            doc,
            &objects,
            |id| !selected.contains(id) && !above.contains(id),
            true,
        ),
        keep_only(doc, &objects, |id| selected.contains(id), false),
        keep_only(doc, &objects, |id| above.contains(id), false),
    ]
}

/// `doc` showing only the top-level `objects` that `keep` accepts, with or
/// without the artboard backgrounds.
fn keep_only(
    doc: &Document,
    objects: &[NodeId],
    keep: impl Fn(&NodeId) -> bool,
    backgrounds: bool,
) -> Document {
    let mut d = doc.clone();
    for id in objects.iter().filter(|id| !keep(id)) {
        if let Some(n) = d.nodes.get_mut(id) {
            n.visible = false;
        }
    }
    if !backgrounds {
        for a in &mut d.artboards {
            a.background = None;
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use vextor_core::{Command, CornerRadii, Geometry, Node, NodeKind, Rgba, Shape, Style};

    use super::*;
    use crate::new_doc::new_document;

    /// Four rects a, b, c, d bottom → top, on an artboard with a background.
    fn doc() -> (Document, [NodeId; 4]) {
        let mut doc = new_document("a", 100.0, 100.0);
        doc.artboards[0].background = Some(Rgba::rgb(255, 255, 255));
        let layer = doc.layers[0];
        let ids = [(); 4].map(|()| doc.alloc_node_id());
        let nodes = ids
            .iter()
            .map(|&id| {
                Node::new(
                    id,
                    NodeKind::Shape(Shape {
                        geometry: Geometry::Rect {
                            rect: vextor_core::Rect::new(0.0, 0.0, 10.0, 10.0),
                            radius: CornerRadii::default(),
                        },
                        style: Style::default(),
                    }),
                )
            })
            .collect();
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes,
        })
        .expect("inserts");
        (doc, ids)
    }

    fn visible(doc: &Document, ids: &[NodeId]) -> Vec<NodeId> {
        ids.iter()
            .copied()
            .filter(|id| doc.nodes[id].visible)
            .collect()
    }

    #[test]
    fn split_by_the_topmost_selected_object() {
        let (doc, [a, b, c, d]) = doc();
        let [below, selection, above] = split(&doc, &[b, c]);
        let all = [a, b, c, d];
        assert_eq!(visible(&below, &all), vec![a]);
        assert_eq!(visible(&selection, &all), vec![b, c]);
        assert_eq!(visible(&above, &all), vec![d]);
    }

    #[test]
    fn an_object_between_selected_ones_goes_below() {
        let (doc, [a, b, c, d]) = doc();
        let [below, _, above] = split(&doc, &[a, c]);
        assert_eq!(visible(&below, &[a, b, c, d]), vec![b]);
        assert_eq!(visible(&above, &[a, b, c, d]), vec![d]);
    }

    #[test]
    fn only_the_bottom_layer_keeps_artboard_backgrounds() {
        let (doc, [_, b, ..]) = doc();
        let [below, selection, above] = split(&doc, &[b]);
        assert!(below.artboards[0].background.is_some());
        assert_eq!(selection.artboards[0].background, None);
        assert_eq!(above.artboards[0].background, None);
    }

    #[test]
    fn to_screen_matches_the_view_maths() {
        let v = View {
            zoom: 2.0,
            pan: vextor_core::Vec2::new(10.0, -4.0),
        };
        let s = crate::view::doc_to_screen(v, vextor_core::Point::new(4.0, 3.0));
        let k = to_screen(v) * kurbo::Point::new(4.0, 3.0);
        assert_eq!((k.x, k.y), (s.x, s.y));
    }

    #[test]
    fn the_quad_puts_the_texture_through_the_affine() {
        let ctx = egui::Context::default();
        let image = ColorImage::new([1, 1], vec![Color32::WHITE]);
        let tex = ctx.load_texture("t", image, TextureOptions::LINEAR);
        let a = Affine::translate((10.0, 0.0)) * Affine::scale(2.0);
        let egui::Shape::Mesh(mesh) = quad(
            &Layer {
                texture: tex,
                area: Rect::new(0.0, 0.0, 4.0, 3.0),
            },
            Pos2::new(5.0, 5.0),
            a,
        ) else {
            panic!("a mesh");
        };
        let corners: Vec<Pos2> = mesh.vertices.iter().map(|v| v.pos).collect();
        assert_eq!(
            corners,
            vec![
                Pos2::new(15.0, 5.0),
                Pos2::new(23.0, 5.0),
                Pos2::new(23.0, 11.0),
                Pos2::new(15.0, 11.0)
            ]
        );
    }
}
