//! The artboard name label above each artboard, and renaming it in place
//! with a double-click (spec 05e).

use eframe::egui::{self, Painter, Pos2};
use vextor_core::{Artboard, ArtboardId};

use super::{Canvas, screen_rect};
use crate::artboard::{self, MAX_NAME_CHARS};
use crate::session::Session;

/// Size of the label text, in points.
pub(super) const LABEL_FONT: f32 = 11.0;
/// Gap between the label and the artboard's top edge, in points.
pub(super) const LABEL_GAP: f32 = 4.0;
/// Extra hit area around a label, in points.
const LABEL_SLOP: f32 = 3.0;

/// A rename in progress.
pub(super) struct Rename {
    id: ArtboardId,
    text: String,
    /// The field has not been shown yet: focus it and select its text.
    fresh: bool,
}

/// Where `artboard`'s label sits on screen.
fn label_rect(
    painter: &Painter,
    origin: Pos2,
    session: &Session,
    artboard: &Artboard,
) -> egui::Rect {
    let r = screen_rect(session.editor.view, origin, artboard.rect);
    let galley = painter.layout_no_wrap(
        artboard.name.clone(),
        egui::FontId::proportional(LABEL_FONT),
        egui::Color32::PLACEHOLDER,
    );
    let bottom_left = r.left_top() - egui::vec2(0.0, LABEL_GAP);
    egui::Rect::from_min_size(
        bottom_left - egui::vec2(0.0, galley.size().y),
        galley.size(),
    )
}

/// The artboard whose label is under `at`, topmost first.
fn label_at(painter: &Painter, origin: Pos2, session: &Session, at: Pos2) -> Option<ArtboardId> {
    session
        .doc
        .artboards
        .iter()
        .rev()
        .find(|a| {
            label_rect(painter, origin, session, a)
                .expand(LABEL_SLOP)
                .contains(at)
        })
        .map(|a| a.id)
}

impl Canvas {
    /// Handles the pointer over artboard labels: a double-click starts a
    /// rename; presses on a label never reach the tools. Returns true when
    /// the pointer input was used here this frame.
    pub(super) fn label_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        session: &Session,
    ) -> bool {
        if self.is_editing_text() {
            return false;
        }
        let painter = ui.painter_at(rect);
        let (hover, pressed, press_at) = ui.input(|i| {
            (
                i.pointer.hover_pos(),
                i.pointer.primary_pressed(),
                i.pointer.press_origin(),
            )
        });
        let hovered = hover
            .filter(|_| response.hovered())
            .and_then(|p| label_at(&painter, rect.min, session, p));
        if hovered.is_some() && self.rename.is_none() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            response
                .clone()
                .on_hover_text_at_pointer("Double-click to rename");
        }
        if response.double_clicked()
            && let Some(id) = hovered
            && let Some(a) = session.doc.artboards.iter().find(|a| a.id == id)
        {
            self.rename = Some(Rename {
                id,
                text: a.name.clone(),
                fresh: true,
            });
            return true;
        }
        pressed
            && response.hovered()
            && press_at.is_some_and(|p| label_at(&painter, rect.min, session, p).is_some())
    }

    /// The label being renamed, if any: the canvas draws no label for it.
    pub(super) fn renaming(&self) -> Option<ArtboardId> {
        self.rename.as_ref().map(|r| r.id)
    }

    /// The rename field over the label. Enter or a click elsewhere commits
    /// (one undo step); `Esc` cancels.
    pub(super) fn rename_ui(&mut self, ui: &egui::Ui, rect: egui::Rect, session: &mut Session) {
        let Some(rename) = &mut self.rename else {
            return;
        };
        let Some(a) = session.doc.artboards.iter().find(|a| a.id == rename.id) else {
            self.rename = None;
            return;
        };
        let at = label_rect(&ui.painter_at(rect), rect.min, session, a).left_bottom();
        let output = egui::Area::new(egui::Id::unique("artboard-rename"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .fixed_pos(at + egui::vec2(-4.0, 3.0))
            .show(ui.ctx(), |ui| {
                egui::TextEdit::singleline(&mut rename.text)
                    .font(egui::FontId::proportional(LABEL_FONT + 1.0))
                    .char_limit(MAX_NAME_CHARS)
                    .desired_width(160.0)
                    .show(ui)
            })
            .inner;
        if rename.fresh {
            rename.fresh = false;
            let mut state = output.state;
            let all = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(rename.text.chars().count()),
            );
            state.cursor.set_char_range(Some(all));
            state.store(ui.ctx(), output.response.id);
            output.response.request_focus();
        } else if !output.response.has_focus() {
            if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                artboard::rename(session, rename.id, &rename.text);
            }
            self.rename = None;
        }
    }
}
