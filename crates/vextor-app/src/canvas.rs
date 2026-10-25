//! The canvas: shows the rendered document and handles zoom and pan.
//!
//! The document is rasterised by `vextor-render` into a texture. While a pan
//! or zoom is moving, the last texture is moved and scaled instead of
//! rendering every frame; the canvas renders sharp again once the view
//! stops changing. Overlays (checkerboard, artboard outlines, selection,
//! marquee) are drawn by egui every frame.

use eframe::egui::{self, Painter, Pos2, Sense, Stroke, StrokeKind, TextureHandle};
use vextor_core::{Point, Rect, Vec2, View};
use vextor_render::RenderCache;

use crate::documents::DocId;
use crate::node_tool::NodeTool;
use crate::pen_tool::PenTool;
use crate::select::SelectTool;
use crate::session::Session;
use crate::text_tool::TextTool;
use crate::view;

use self::document::Shown;

mod artboard_label;
pub mod context_menu;
mod document;
mod drag_layers;
mod eyedropper;
mod gradient_handles;
mod grid;
#[cfg(test)]
mod harness;
mod nodes;
mod path_marks;
mod pen;
mod selection;
mod shapes;
mod snap;
mod text;
mod zoom_bar;
mod zoom_tool;

/// Side of a drawn transform-box handle, in points.
const HANDLE_SIZE: f32 = 7.0;

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
    error: Option<String>,
    tool: SelectTool,
    /// The layer textures of the drag in progress (spec 05b).
    drag_layers: Option<drag_layers::DragLayers>,
    /// Text placing and editing (spec 05c).
    text: TextTool,
    /// An artboard label being renamed (spec 05e).
    rename: Option<artboard_label::Rename>,
    /// An artboard label being dragged (spec 08b).
    artboard_drag: Option<crate::artboard::ArtboardDrag>,
    /// The zoom tool's press in progress (spec 08b).
    zoom_drag: Option<zoom_tool::ZoomDrag>,
    /// The zoom bar's typed percentage, while its menu is open (spec 08b).
    zoom_text: Option<String>,
    /// The document drawn last frame (spec 05f).
    document: Option<DocId>,
    /// Shape tools, corner widgets and the size dialog (spec 06).
    shapes: shapes::ShapeInput,
    /// The pen (spec 07).
    pen: PenTool,
    /// Node editing (spec 07).
    nodes: NodeTool,
    /// Snapping and its hint (spec 08).
    snap: crate::snapping::Snapper,
    /// View ▸ Smart Guides turned off (spec 08; not saved).
    pub hide_smart_guides: bool,
    /// The last right-click landed on an object (spec 08c).
    menu_on_object: bool,
    /// What the context menu asked the app to do: a path operation (which
    /// may ask about text first, or open a dialog) or Export Selection….
    request: Option<context_menu::MenuRequest>,
    /// A gradient handle being dragged (spec 09).
    gradient_drag: Option<gradient_handles::GradientDrag>,
    /// The gradient handle under the pointer (or dragged) this frame: its
    /// grab cursor wins over the Select tool's resize and rotate ones.
    gradient_hover: Option<crate::paint_edit::GradientPoint>,
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

    /// Zooms one stop in (`inward`, `⌘+`) or out (`⌘-`) about the
    /// selection's centre, or the canvas centre with nothing selected.
    pub fn zoom_step(&self, session: &mut Session, inward: bool) {
        let v = session.editor.view;
        let at = vextor_geom::union_bounds(&session.doc, &session.fonts, &session.selected())
            .map_or_else(
                || view::centre(self.size),
                |b| view::doc_to_screen(v, Point::new(b.center().x, b.center().y)),
            );
        session.editor.view = view::step(v, at, inward);
    }

    /// Sets the zoom to `zoom` about the canvas centre (the zoom bar).
    pub fn zoom_to(&self, session: &mut Session, zoom: f64) {
        let v = session.editor.view;
        session.editor.view = view::zoom_to(v, view::centre(self.size), zoom);
    }

    /// Starts afresh for document `doc` when it is not the one drawn last:
    /// the texture, render cache, drag, text edit and rename all belonged
    /// to the previous document (a revision number means nothing across
    /// documents). The view itself is the session's, so it is kept.
    fn show_document(&mut self, doc: DocId) {
        if self.document == Some(doc) {
            return;
        }
        *self = Self {
            checker: self.checker.take(),
            size: self.size,
            hide_smart_guides: self.hide_smart_guides,
            document: Some(doc),
            // Polygon sides and star points carry over to the next shape.
            shapes: self.shapes.carry_over(),
            ..Self::default()
        };
    }

    /// Draws document `doc` into the rest of `ui` and handles its input.
    /// Returns a message for the user when an edit was refused.
    pub fn ui(&mut self, ui: &mut egui::Ui, doc: DocId, session: &mut Session) -> Option<String> {
        self.show_document(doc);
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        self.size = to_vec2(rect.size());
        if session.needs_fit && rect.width() > 0.0 && rect.height() > 0.0 {
            self.fit(session);
            session.needs_fit = false;
        }
        let typing = self.is_editing_text() || self.rename.is_some();
        handle_input(ui, &response, rect, &mut session.editor.view, typing);
        self.pen_keys(ui, session);
        let key_message = self
            .text_keys(ui, session)
            .or_else(|| self.shape_keys(ui, session));
        let (used, label_message) = self.label_input(ui, &response, rect, session);
        let key_message = label_message.or(key_message);
        self.gradient_hover = None;
        let message = if used
            || self.eyedropper_input(ui, &response, rect, session)
            || self.gradient_input(ui, &response, rect, session)
            || self.zoom_input(ui, &response, rect, session)
        {
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
        if session.doc.grid.visible {
            grid::draw(&painter, rect, &session.doc.grid, current);
        }
        draw_artboard_outlines(ui, &painter, rect.min, session, self.renaming());
        let hover = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|_| response.hovered())
            .map(|p| {
                let d = view::screen_to_doc(session.editor.view, to_point(p - rect.min));
                kurbo::Point::new(d.x, d.y)
            });
        let hover = self.snap_hover(ui, session, hover);
        if self.is_editing_text() {
            self.draw_text_overlay(&painter, rect.min, session);
        } else if session.editor.tool == vextor_core::ToolId::Pen {
            self.draw_pen(ui, &painter, rect.min, session, hover);
        } else if session.editor.tool == vextor_core::ToolId::Node {
            self.draw_nodes(&painter, rect.min, session, hover);
        } else if session.editor.tool == vextor_core::ToolId::Zoom {
            self.draw_box(&painter, rect.min, session, &session.selected());
            self.draw_zoom_rect(&painter);
        } else {
            self.draw_selection(ui, &painter, &response, rect, session);
            self.draw_gradient_handles(&painter, rect.min, session);
        }
        Self::draw_eyedropper_hint(ui, &painter, rect, session);
        self.draw_snap_hint(&painter, rect.min, session, ui.visuals().extreme_bg_color);
        self.draw_status(ui, &painter, rect);
        self.zoom_bar(ui, rect, session);
        self.rename_ui(ui, rect, session);
        self.last_view = Some(current);
        let dialog_message = self.size_dialog_ui(ui.ctx(), rect, session);
        let menu_message = if self.is_editing_text() {
            None
        } else {
            self.context_menu(&response, rect, session)
        };
        message.or(dialog_message).or(menu_message)
    }

    /// A render error, over the canvas. The zoom shows in the zoom bar
    /// (spec 08b).
    fn draw_status(&self, ui: &egui::Ui, painter: &Painter, rect: egui::Rect) {
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

/// A document rectangle on screen.
fn screen_rect(v: View, origin: Pos2, r: Rect) -> egui::Rect {
    let a = view::doc_to_screen(v, Point::new(r.x, r.y));
    let b = view::doc_to_screen(v, Point::new(r.x + r.width, r.y + r.height));
    egui::Rect::from_two_pos(origin + to_egui(a), origin + to_egui(b))
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

    #[test]
    fn switching_documents_starts_the_canvas_afresh() {
        let mut docs = crate::documents::Documents::default();
        let new = |name| Session::new(crate::new_doc::new_document(name, 24.0, 24.0));
        let a = docs.add(new("a"), None);
        let b = docs.add(new("b"), None);
        let mut canvas = Canvas::default();
        canvas.show_document(a);
        canvas.shown = Some(document::tests::shown(1.0, 3));
        canvas.show_document(a);
        assert!(canvas.shown.is_some(), "same document: texture kept");
        canvas.show_document(b);
        assert!(canvas.shown.is_none(), "other document: rendered again");
        assert!(canvas.renaming().is_none());
    }
}
