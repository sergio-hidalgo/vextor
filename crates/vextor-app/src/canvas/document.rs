//! The document layer of the canvas: the texture `vextor-render` draws,
//! rendered again only when the document, the canvas size or a settled
//! view changes (moved and scaled in between), and the checkerboard
//! behind transparent artboards.

use eframe::egui::{
    self, Color32, ColorImage, Painter, Pos2, TextureHandle, TextureOptions, TextureWrapMode,
};
use vextor_render::Viewport;

use super::{Canvas, screen_rect};
use crate::session::Session;
use crate::view;

/// Side of one checkerboard square, in points.
const CHECKER_CELL: f32 = 8.0;

/// What the current texture shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Shown {
    pub(super) viewport: Viewport,
    pub(super) revision: u64,
}

impl Canvas {
    /// The document texture (or the drag layers standing in for it),
    /// rendering it first when [`needs_render`] says so.
    pub(super) fn draw_document(
        &mut self,
        ui: &egui::Ui,
        painter: &Painter,
        rect: egui::Rect,
        session: &Session,
    ) {
        let ppp = f64::from(ui.ctx().pixels_per_point());
        let viewport = Viewport {
            view: session.editor.view,
            width: physical(rect.width(), ppp),
            height: physical(rect.height(), ppp),
            pixels_per_point: ppp,
        };
        if viewport.width == 0 || viewport.height == 0 {
            return;
        }
        // While the selection is dragged, its layers stand in for a render.
        if self.draw_drag_layers(ui.ctx(), painter, rect, &viewport, session) {
            return;
        }
        let settled = self.last_view == Some(viewport.view);
        if needs_render(self.shown.as_ref(), &viewport, session.revision(), settled) {
            self.render(ui.ctx(), session, &viewport);
        } else if self.shown.is_some_and(|s| s.viewport.view != viewport.view) {
            // Mid-gesture: render once the view holds still for a frame.
            ui.ctx().request_repaint();
        }
        let (Some(texture), Some(shown)) = (&self.texture, self.shown) else {
            return;
        };
        let (origin, k) = view::reproject(shown.viewport.view, viewport.view);
        #[allow(clippy::cast_possible_truncation)] // screen coordinates
        let min = rect.min + egui::vec2(origin.x as f32, origin.y as f32);
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // pixel sizes
        let size = egui::vec2(
            (f64::from(shown.viewport.width) / ppp * k) as f32,
            (f64::from(shown.viewport.height) / ppp * k) as f32,
        );
        let uv = egui::Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0));
        painter.image(
            texture.id(),
            egui::Rect::from_min_size(min, size),
            uv,
            Color32::WHITE,
        );
    }

    fn render(&mut self, ctx: &egui::Context, session: &Session, viewport: &Viewport) {
        match self
            .cache
            .frame(&session.doc, &session.fonts, session.revision(), viewport)
        {
            Ok(frame) => {
                if frame.fresh {
                    let size = [
                        frame.pixmap.width() as usize,
                        frame.pixmap.height() as usize,
                    ];
                    let image = ColorImage::from_rgba_premultiplied(size, frame.pixmap.data());
                    match &mut self.texture {
                        Some(t) => t.set(image, TextureOptions::LINEAR),
                        None => {
                            self.texture =
                                Some(ctx.load_texture("document", image, TextureOptions::LINEAR));
                        }
                    }
                }
                self.shown = Some(Shown {
                    viewport: *viewport,
                    revision: session.revision(),
                });
                self.error = None;
            }
            Err(e) => {
                self.shown = None;
                self.texture = None;
                self.error = Some(format!("The canvas could not be drawn: {e}"));
            }
        }
    }

    /// A checkerboard behind every transparent artboard.
    pub(super) fn draw_checkerboards(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        origin: Pos2,
        session: &Session,
    ) {
        let checker = self.checker.get_or_insert_with(|| checker_texture(ctx));
        for artboard in session
            .doc
            .artboards
            .iter()
            .filter(|a| a.background.is_none())
        {
            let r = screen_rect(session.editor.view, origin, artboard.rect);
            let uv = egui::Rect::from_min_size(Pos2::ZERO, r.size() / (2.0 * CHECKER_CELL));
            painter.image(checker.id(), r, uv, Color32::WHITE);
        }
    }
}

/// Whether the texture must be rendered again: the document changed, the
/// canvas was resized, or the view changed and has stopped moving.
fn needs_render(shown: Option<&Shown>, now: &Viewport, revision: u64, settled: bool) -> bool {
    let Some(shown) = shown else {
        return true;
    };
    let resized = shown.viewport.width != now.width
        || shown.viewport.height != now.height
        || (shown.viewport.pixels_per_point - now.pixels_per_point).abs() > f64::EPSILON;
    shown.revision != revision || resized || (shown.viewport.view != now.view && settled)
}

/// A 2×2 texel checkerboard, tiled by the GPU.
fn checker_texture(ctx: &egui::Context) -> TextureHandle {
    let light = Color32::from_gray(255);
    let dark = Color32::from_gray(214);
    let image = ColorImage::new([2, 2], vec![light, dark, dark, light]);
    let options = TextureOptions {
        wrap_mode: TextureWrapMode::Repeat,
        ..TextureOptions::NEAREST
    };
    ctx.load_texture("checkerboard", image, options)
}

/// Logical points → physical pixels, rounded.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // canvas sizes
fn physical(points: f32, ppp: f64) -> u32 {
    (f64::from(points) * ppp).round().max(0.0) as u32
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use vextor_core::{Vec2, View};

    fn viewport(zoom: f64) -> Viewport {
        Viewport {
            view: View {
                zoom,
                pan: Vec2::new(0.0, 0.0),
            },
            width: 100,
            height: 80,
            pixels_per_point: 2.0,
        }
    }

    pub(in crate::canvas) fn shown(zoom: f64, revision: u64) -> Shown {
        Shown {
            viewport: viewport(zoom),
            revision,
        }
    }

    #[test]
    fn first_frame_renders() {
        assert!(needs_render(None, &viewport(1.0), 0, false));
    }

    #[test]
    fn unchanged_frame_reuses_the_texture() {
        assert!(!needs_render(Some(&shown(1.0, 3)), &viewport(1.0), 3, true));
    }

    #[test]
    fn document_change_renders_even_mid_gesture() {
        assert!(needs_render(Some(&shown(1.0, 3)), &viewport(1.0), 4, false));
    }

    #[test]
    fn moving_view_waits_until_settled() {
        assert!(!needs_render(
            Some(&shown(1.0, 3)),
            &viewport(2.0),
            3,
            false
        ));
        assert!(needs_render(Some(&shown(1.0, 3)), &viewport(2.0), 3, true));
    }

    #[test]
    fn resize_renders_immediately() {
        let mut now = viewport(1.0);
        now.width = 120;
        assert!(needs_render(Some(&shown(1.0, 3)), &now, 3, false));
    }

    #[test]
    fn physical_rounds_points_to_pixels() {
        assert_eq!(physical(100.4, 2.0), 201);
        assert_eq!(physical(0.0, 2.0), 0);
    }
}
