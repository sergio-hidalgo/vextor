//! The canvas: shows the rendered document and handles zoom and pan.
//!
//! The document is rasterised by `vextor-render` into a texture. While a pan
//! or zoom is moving, the last texture is moved and scaled instead of
//! rendering every frame; the canvas renders sharp again once the view
//! stops changing. Overlays (checkerboard, artboard outlines, selection,
//! marquee) are drawn by egui every frame.

use std::fmt::Write as _;
use std::time::Instant;

use eframe::egui::{
    self, Color32, ColorImage, Painter, Pos2, Sense, Stroke, StrokeKind, TextureHandle,
    TextureOptions, TextureWrapMode,
};
use vextor_core::{Point, Rect, Vec2, View};
use vextor_render::{RenderCache, Viewport};

use crate::select::SelectTool;
use crate::session::Session;
use crate::text_tool::TextTool;
use crate::view;

mod artboard_label;
mod drag_layers;
mod selection;
mod text;

/// Side of one checkerboard square, in points.
const CHECKER_CELL: f32 = 8.0;
/// Side of a drawn transform-box handle, in points.
const HANDLE_SIZE: f32 = 7.0;

/// What the current texture shows.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shown {
    viewport: Viewport,
    revision: u64,
}

/// Canvas state kept between frames.
#[derive(Default)]
pub struct Canvas {
    cache: RenderCache,
    texture: Option<TextureHandle>,
    checker: Option<TextureHandle>,
    shown: Option<Shown>,
    /// The view on the previous frame, to tell when a gesture has stopped.
    last_view: Option<View>,
    /// Canvas size in points on the last frame.
    size: Vec2,
    /// Show render and frame times (debug menu).
    pub show_stats: bool,
    last_render_ms: Option<f64>,
    error: Option<String>,
    tool: SelectTool,
    /// The layer textures of the drag in progress (spec 05b).
    drag_layers: Option<drag_layers::DragLayers>,
    /// Text placing and editing (spec 05c).
    text: TextTool,
    /// An artboard label being renamed (spec 05e).
    rename: Option<artboard_label::Rename>,
}

impl Canvas {
    /// Fits all artboards in view (`⌘0`).
    pub fn fit(&self, session: &mut Session) {
        if let Some(target) = view::bounds(session.doc.artboards.iter().map(|a| a.rect)) {
            session.editor.view = view::fit(target, self.size);
        }
    }

    /// Zooms to 100 % about the canvas centre (`⌘1`).
    pub fn actual_size(&self, session: &mut Session) {
        session.editor.view = view::actual_size(session.editor.view, self.size);
    }

    /// Draws the canvas into the rest of `ui` and handles its input.
    /// Returns a message for the user when an edit was refused.
    pub fn ui(&mut self, ui: &mut egui::Ui, session: &mut Session) -> Option<String> {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        self.size = to_vec2(rect.size());
        if session.needs_fit && rect.width() > 0.0 && rect.height() > 0.0 {
            self.fit(session);
            session.needs_fit = false;
        }
        let typing = self.is_editing_text() || self.rename.is_some();
        handle_input(ui, &response, rect, &mut session.editor.view, typing);
        let key_message = self.text_keys(ui, session);
        let message = if self.label_input(ui, &response, rect, session) {
            key_message
        } else {
            self.tool_input(ui, &response, rect, session)
                .or(key_message)
        };

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        let current = session.editor.view;
        self.draw_checkerboards(ui.ctx(), &painter, rect.min, session);
        self.draw_document(ui, &painter, rect, session);
        draw_artboard_outlines(ui, &painter, rect.min, session, self.renaming());
        if self.is_editing_text() {
            self.draw_text_overlay(&painter, rect.min, session);
        } else {
            self.draw_selection(ui, &painter, &response, rect, session);
        }
        self.draw_status(ui, &painter, rect, current);
        self.rename_ui(ui, rect, session);
        self.last_view = Some(current);
        message
    }

    fn draw_document(
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
        let started = Instant::now();
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
        self.last_render_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
    }

    /// A checkerboard behind every transparent artboard.
    fn draw_checkerboards(
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

    fn draw_status(&self, ui: &egui::Ui, painter: &Painter, rect: egui::Rect, current: View) {
        let mut text = format!("{:.0} %", current.zoom * 100.0);
        if self.show_stats {
            let dt = ui.input(|i| i.unstable_dt) * 1000.0;
            let render = self
                .last_render_ms
                .map_or_else(|| "–".to_owned(), |ms| format!("{ms:.1} ms"));
            let _ = write!(text, "   frame {dt:.1} ms   last render {render}");
        }
        let color = ui.visuals().weak_text_color();
        let font = egui::FontId::proportional(12.0);
        painter.text(
            rect.left_bottom() + egui::vec2(8.0, -6.0),
            egui::Align2::LEFT_BOTTOM,
            text,
            font,
            color,
        );
        if let Some(e) = &self.error {
            let error_font = egui::FontId::proportional(13.0);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                e,
                error_font,
                ui.visuals().error_fg_color,
            );
        }
    }
}

/// Zoom (pinch, ⌘-scroll), pan (two-finger scroll, space-drag, middle-drag).
/// While text is being typed, Space types a space instead of panning.
fn handle_input(
    ui: &egui::Ui,
    response: &egui::Response,
    rect: egui::Rect,
    v: &mut View,
    typing: bool,
) {
    let space = !typing && ui.input(|i| i.key_down(egui::Key::Space));
    if response.hovered() {
        let (zoom, scroll, hover) = ui.input(|i| {
            (
                i.zoom_delta(),
                i.smooth_scroll_delta(),
                i.pointer.hover_pos(),
            )
        });
        if let Some(pos) = hover
            && (zoom - 1.0).abs() > f32::EPSILON
        {
            *v = view::zoom_at(*v, to_point(pos - rect.min), f64::from(zoom));
        }
        if scroll != egui::Vec2::ZERO {
            *v = view::pan_by(*v, to_vec2(scroll));
        }
        if space {
            ui.ctx().set_cursor_icon(if response.dragged() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
        }
    }
    let panning = response.dragged_by(egui::PointerButton::Middle)
        || (space && response.dragged_by(egui::PointerButton::Primary));
    if panning {
        *v = view::pan_by(*v, to_vec2(response.drag_delta()));
    }
}

/// Each artboard's outline and name label, except the label of `renaming`
/// (its rename field stands there instead).
fn draw_artboard_outlines(
    ui: &egui::Ui,
    painter: &Painter,
    origin: Pos2,
    session: &Session,
    renaming: Option<vextor_core::ArtboardId>,
) {
    let stroke = Stroke::new(1.0, ui.visuals().weak_text_color());
    let font = egui::FontId::proportional(artboard_label::LABEL_FONT);
    for artboard in &session.doc.artboards {
        let r = screen_rect(session.editor.view, origin, artboard.rect);
        painter.rect_stroke(r, 0.0, stroke, StrokeKind::Outside);
        if renaming == Some(artboard.id) {
            continue;
        }
        painter.text(
            r.left_top() - egui::vec2(0.0, artboard_label::LABEL_GAP),
            egui::Align2::LEFT_BOTTOM,
            &artboard.name,
            font.clone(),
            ui.visuals().weak_text_color(),
        );
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

/// A document rectangle on screen.
fn screen_rect(v: View, origin: Pos2, r: Rect) -> egui::Rect {
    let a = view::doc_to_screen(v, Point::new(r.x, r.y));
    let b = view::doc_to_screen(v, Point::new(r.x + r.width, r.y + r.height));
    egui::Rect::from_two_pos(origin + to_egui(a), origin + to_egui(b))
}

/// Logical points → physical pixels, rounded.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // canvas sizes
fn physical(points: f32, ppp: f64) -> u32 {
    (f64::from(points) * ppp).round().max(0.0) as u32
}

fn to_vec2(v: egui::Vec2) -> Vec2 {
    Vec2::new(f64::from(v.x), f64::from(v.y))
}

fn to_point(v: egui::Vec2) -> Point {
    Point::new(f64::from(v.x), f64::from(v.y))
}

#[allow(clippy::cast_possible_truncation)] // screen coordinates
fn to_egui(p: Point) -> egui::Vec2 {
    egui::vec2(p.x as f32, p.y as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn shown(zoom: f64, revision: u64) -> Shown {
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
