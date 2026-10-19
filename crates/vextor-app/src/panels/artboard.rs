//! Inspector ▸ Artboard (spec 05e): the artboard's name, size and
//! background, shown while nothing is selected.

use eframe::egui;
use vextor_core::Artboard;

use super::text::{color_field, section_toggle};
use super::{FIELD_SIZE, GRID_SPACING, LABEL_WIDTH, Label};
use crate::artboard::{self, DEFAULT_BACKGROUND, MAX_NAME_CHARS};
use crate::icons::Icon;
use crate::session::Session;

/// Width of the name field: two number fields and the gap between them.
const NAME_WIDTH: f32 = FIELD_SIZE[0] * 2.0 + GRID_SPACING[0];

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
                let w = Label("W", Icon::Width, "Width (W)");
                let h = Label("H", Icon::Height, "Height (H)");
                for (label, value) in [(w, board.rect.width), (h, board.rect.height)] {
                    label.show(ui);
                    let mut v = value;
                    ui.add_enabled_ui(false, |ui| {
                        ui.add_sized(FIELD_SIZE, egui::DragValue::new(&mut v).max_decimals(2))
                    })
                    .inner
                    .on_disabled_hover_text("Set when the document is created");
                    ui.end_row();
                }
            });
        background_fields(ui, s, &board);
    });
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
