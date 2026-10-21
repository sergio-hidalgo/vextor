//! Inspector ▸ Artboards (spec 08): the document's artboards in export
//! order, to pick the current one, add, reorder and delete; then
//! Inspector ▸ Artboard (spec 05e): the current artboard's name, size and
//! background. Both show while nothing is selected. X and Y move the
//! artboard with the objects on it (spec 08b); W and H resize it about its
//! centre, the objects staying where they are.

use eframe::egui;
use vextor_core::Artboard;

use super::PanelAction;
use super::text::{color_field, section_toggle};
use super::{FIELD_SIZE, GRID_SPACING, LABEL_WIDTH, Label};
use crate::artboard::{self, DEFAULT_BACKGROUND, MAX_NAME_CHARS};
use crate::icons::{self, Icon};
use crate::new_doc::MAX_SIDE;
use crate::session::Session;

/// Width of the name field: two number fields and the gap between them.
const NAME_WIDTH: f32 = FIELD_SIZE[0] * 2.0 + GRID_SPACING[0];

/// The Artboards list: one row per artboard (a click makes it current),
/// the current one's move up / move down / delete buttons, and Add.
pub(super) fn list_section(ui: &mut egui::Ui, s: &mut Session, actions: &mut Vec<PanelAction>) {
    let current = artboard::current(s).map(|a| a.id);
    let rows: Vec<(vextor_core::ArtboardId, String)> = s
        .doc
        .artboards
        .iter()
        .map(|a| (a.id, a.name.clone()))
        .collect();
    let count = rows.len();
    super::section(ui, "Artboards", |ui| {
        for (i, (id, name)) in rows.into_iter().enumerate() {
            let chosen = current == Some(id);
            ui.horizontal(|ui| {
                if ui.selectable_label(chosen, &name).clicked() {
                    artboard::pick(s, id);
                }
                if !chosen {
                    return;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let only = "A document keeps at least one artboard";
                    if icons::button(ui, Icon::Delete, false, count > 1, "Delete artboard", only)
                        .clicked()
                    {
                        let _ = artboard::delete(s, id);
                    }
                    let last = "Already the last";
                    if icons::button(
                        ui,
                        Icon::MoveDown,
                        false,
                        i + 1 < count,
                        "Move down (later in export order)",
                        last,
                    )
                    .clicked()
                    {
                        artboard::reorder(s, id, false);
                    }
                    let first = "Already the first";
                    if icons::button(
                        ui,
                        Icon::MoveUp,
                        false,
                        i > 0,
                        "Move up (earlier in export order)",
                        first,
                    )
                    .clicked()
                    {
                        artboard::reorder(s, id, true);
                    }
                });
            });
        }
        if icons::button(ui, Icon::Add, false, true, "Add artboard…", "").clicked() {
            actions.push(PanelAction::AddArtboard);
        }
    });
}

/// The Artboard section, or "Nothing selected." for a document without
/// artboards.
pub(super) fn section(ui: &mut egui::Ui, s: &mut Session) {
    let Some(board) = artboard::current(s).cloned() else {
        ui.weak("Nothing selected.");
        return;
    };
    super::section(ui, "Artboard", |ui| {
        egui::Grid::new("artboard-inspector")
            .num_columns(2)
            .spacing(GRID_SPACING)
            .min_col_width(LABEL_WIDTH)
            .show(ui, |ui| {
                ui.label("Name");
                name_field(ui, s, &board);
                ui.end_row();
            });
        geometry_fields(ui, s, &board);
        background_fields(ui, s, &board);
    });
}

/// X | Y, then W | H. X and Y move the artboard with the objects on it;
/// W and H resize it about its centre, the objects staying where they are.
fn geometry_fields(ui: &mut egui::Ui, s: &mut Session, board: &Artboard) {
    let rect = board.rect;
    let place = -MAX_SIDE..=MAX_SIDE;
    let side = artboard::MIN_SIDE..=MAX_SIDE;
    let moves = "Moves it with the objects on it";
    let resizes = "Resizes from the centre; objects stay where they are";
    egui::Grid::new("artboard-geometry")
        .num_columns(4)
        .spacing(GRID_SPACING)
        .show(ui, |ui| {
            let x_label = Label("X", Icon::PosX, "Left edge (X)");
            let y_label = Label("Y", Icon::PosY, "Top edge (Y)");
            number(ui, s, x_label, rect.x, place.clone(), moves, |s, value| {
                artboard::move_to(s, board.id, value, rect.y);
            });
            number(ui, s, y_label, rect.y, place, moves, |s, value| {
                artboard::move_to(s, board.id, rect.x, value);
            });
            ui.end_row();
            let w_label = Label("W", Icon::Width, "Width (W)");
            let h_label = Label("H", Icon::Height, "Height (H)");
            number(
                ui,
                s,
                w_label,
                rect.width,
                side.clone(),
                resizes,
                |s, value| {
                    artboard::resize(s, board.id, value, rect.height);
                },
            );
            number(ui, s, h_label, rect.height, side, resizes, |s, value| {
                artboard::resize(s, board.id, rect.width, value);
            });
            ui.end_row();
        });
}

/// A labelled number field in `range` (`hover`: what changing it does)
/// that calls `apply` with a changed value. A drag is one history gesture.
fn number(
    ui: &mut egui::Ui,
    s: &mut Session,
    label: Label,
    value: f64,
    range: std::ops::RangeInclusive<f64>,
    hover: &str,
    apply: impl FnOnce(&mut Session, f64),
) {
    label.show(ui);
    let mut v = value;
    let r = ui
        .add_sized(
            FIELD_SIZE,
            egui::DragValue::new(&mut v)
                .range(range)
                .clamp_existing_to_range(false)
                .max_decimals(2),
        )
        .on_hover_text(hover);
    if r.drag_started() {
        s.begin_gesture();
    }
    if r.changed() && (v - value).abs() > vextor_core::EPS {
        apply(s, v);
    }
    if r.drag_stopped() {
        s.end_gesture();
    }
}

/// The name, committed when the field loses focus (Enter or a click
/// elsewhere); `Esc` puts the old name back.
fn name_field(ui: &mut egui::Ui, s: &mut Session, a: &Artboard) {
    let key = egui::Id::unique("artboard-name-draft");
    let mut text = ui
        .data(|d| d.get_temp::<String>(key))
        .unwrap_or_else(|| a.name.clone());
    let r = ui.add(
        egui::TextEdit::singleline(&mut text)
            .char_limit(MAX_NAME_CHARS)
            .desired_width(NAME_WIDTH),
    );
    if r.changed() {
        ui.data_mut(|d| d.insert_temp(key, text.clone()));
    }
    if r.lost_focus() {
        if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            artboard::rename(s, a.id, &text);
        }
        ui.data_mut(|d| d.remove::<String>(key));
    } else if !r.has_focus() {
        ui.data_mut(|d| d.remove::<String>(key));
    }
}

/// Background on/off and its colour. Changes made while the pointer is
/// held in the colour picker are one undo step.
fn background_fields(ui: &mut egui::Ui, s: &mut Session, a: &Artboard) {
    if section_toggle(ui, "Background", a.background.is_some()) {
        let next = a.background.is_none().then_some(DEFAULT_BACKGROUND);
        artboard::set_background(s, a.id, next);
    }
    let gesture = egui::Id::unique("artboard-background-gesture");
    let in_gesture = ui.data(|d| d.get_temp::<bool>(gesture)).unwrap_or(false);
    let down = ui.input(|i| i.pointer.any_down());
    if let Some(current) = a.background {
        egui::Grid::new("artboard-background-inspector")
            .num_columns(2)
            .spacing(GRID_SPACING)
            .min_col_width(LABEL_WIDTH)
            .show(ui, |ui| {
                ui.label("Colour");
                if let Some(c) = color_field(ui, "artboard-background", Some(current)) {
                    if down && !in_gesture {
                        s.begin_gesture();
                        ui.data_mut(|d| d.insert_temp(gesture, true));
                    }
                    artboard::set_background(s, a.id, Some(c));
                }
                ui.end_row();
            });
    }
    if in_gesture && !down {
        s.end_gesture();
        ui.data_mut(|d| d.remove::<bool>(gesture));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_doc::new_document;

    #[test]
    fn showing_the_section_leaves_the_document_alone() {
        let mut s = Session::new(new_document("card", 320.0, 200.0));
        let id = s.doc.artboards[0].id;
        artboard::set_background(&mut s, id, Some(DEFAULT_BACKGROUND));
        let before = s.doc.clone();
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| section(ui, &mut s));
        out.textures_delta.clear();
        assert_eq!(s.doc, before);
    }
}
