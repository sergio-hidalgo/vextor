//! The SVG export options (spec 12), shown in the code panel's Options
//! menu and in File ▸ Export…. Each change is saved with the document
//! (`Session::set_export`), not an undo step.

use eframe::egui;
use vextor_core::{IdMode, Styling, SvgExportOptions};

/// How the checkboxes sit under the fields above them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checks {
    /// From the left edge (the code panel's popup).
    Left,
    /// In the fields' column, lined up with the drop-downs (File ▸ Export…).
    UnderFields,
}

/// The options of `current` as widgets; the changed options, if any.
pub fn options_ui(
    ui: &mut egui::Ui,
    current: &SvgExportOptions,
    checks: Checks,
) -> Option<SvgExportOptions> {
    let mut o = current.clone();
    egui::Grid::new("export_options")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Decimals");
            ui.add(egui::DragValue::new(&mut o.precision).range(0..=6))
                .on_hover_text("Digits after the point in every number");
            ui.end_row();

            // Inside the code panel's Options popup these lists are popups of
            // their own; that popup stays open over them (`code_panel.rs`).
            ui.label("Ids");
            egui::ComboBox::from_id_salt("export_ids")
                .selected_text(id_title(o.ids))
                .show_ui(ui, |ui| {
                    for ids in [IdMode::Readable, IdMode::UniquePrefixed, IdMode::None] {
                        ui.selectable_value(&mut o.ids, ids, id_title(ids))
                            .on_hover_text(id_hint(ids));
                    }
                });
            ui.end_row();

            ui.label("Styling");
            egui::ComboBox::from_id_salt("export_styling")
                .selected_text(styling_title(o.styling))
                .show_ui(ui, |ui| {
                    for styling in [Styling::Attributes, Styling::StyleElement] {
                        ui.selectable_value(&mut o.styling, styling, styling_title(styling))
                            .on_hover_text(styling_hint(styling));
                    }
                });
            ui.end_row();
            if checks == Checks::UnderFields {
                checkboxes(ui, &mut o, true);
            }
        });
    if checks == Checks::Left {
        ui.add_space(4.0);
        checkboxes(ui, &mut o, false);
    }
    (o != *current).then_some(o)
}

/// The four switches; `in_grid`: one grid row each, in the fields' column.
fn checkboxes(ui: &mut egui::Ui, o: &mut SvgExportOptions, in_grid: bool) {
    let rows: [(&mut bool, &str, &str); 4] = [
        (
            &mut o.responsive,
            "Responsive (no width or height)",
            "Only a viewBox, so the SVG takes the size CSS gives it",
        ),
        (
            &mut o.current_color,
            "Main colour as currentColor",
            "The most used colour follows the CSS color of the page",
        ),
        (
            &mut o.text_as_outlines,
            "Text as outlines",
            "Paths instead of text: looks the same without the font",
        ),
        (&mut o.minify, "Minify", "Everything on one line"),
    ];
    for (value, text, hint) in rows {
        if in_grid {
            ui.label("");
        }
        ui.checkbox(value, text).on_hover_text(hint);
        if in_grid {
            ui.end_row();
        }
    }
}

fn id_title(ids: IdMode) -> &'static str {
    match ids {
        IdMode::Readable => "From names",
        IdMode::UniquePrefixed => "Unique",
        IdMode::None => "Only where needed",
    }
}

fn id_hint(ids: IdMode) -> &'static str {
    match ids {
        IdMode::Readable => "Named objects get their name as id",
        IdMode::UniquePrefixed => {
            "Names with a vx-… prefix, so they never clash with another SVG on the same page"
        }
        IdMode::None => {
            "No ids on objects; gradients, patterns and clips keep the one they are referenced by"
        }
    }
}

fn styling_title(styling: Styling) -> &'static str {
    match styling {
        Styling::Attributes => "Attributes",
        Styling::StyleElement => "Classes",
    }
}

fn styling_hint(styling: Styling) -> &'static str {
    match styling {
        Styling::Attributes => "Colours and strokes as attributes on each element",
        Styling::StyleElement => "Colours and strokes as classes in one <style> block",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn showing_the_options_changes_nothing() {
        let ctx = egui::Context::default();
        let current = SvgExportOptions::default();
        let mut changed = Some(SvgExportOptions::default());
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            changed = options_ui(ui, &current, Checks::Left);
        });
        out.textures_delta.clear();
        assert_eq!(changed, None);
    }
}
