//! A stroke's (or a text border's) shape (spec 09): width, cap, join,
//! miter limit and dashes (Solid / Dashed / Dotted, or typed lengths).

use eframe::egui;
use vextor_core::{LineCap, LineJoin, Stroke};

use super::layout::Row;
use crate::icons::{self, Icon};
use crate::paint_edit::{self, DashPreset, Edit, Target};
use crate::session::Session;

/// The fields for `stroke`; the edit made this frame, if any. A drag is
/// one undo step.
pub(super) fn fields(
    ui: &mut egui::Ui,
    s: &mut Session,
    target: Target,
    stroke: &Stroke,
) -> Option<Edit> {
    let mut edit = None;
    let row = Row::new(ui, "Width");
    let width = row.put(ui, row.field2(), |ui| {
        number(ui, s, stroke.width, 0.0..=1000.0)
    });
    edit = width.map(Edit::Width).or(edit);
    let row = Row::new(ui, "Cap");
    let caps = [
        (LineCap::Butt, Icon::CapButt, "Butt cap"),
        (LineCap::Round, Icon::CapRound, "Round cap"),
        (LineCap::Square, Icon::CapSquare, "Square cap"),
    ];
    let cap = row.put(ui, row.wide(), |ui| choice(ui, stroke.cap, &caps));
    edit = cap.map(Edit::Cap).or(edit);
    let row = Row::new(ui, "Join");
    let joins = [
        (LineJoin::Miter, Icon::JoinMiter, "Miter join"),
        (LineJoin::Round, Icon::JoinRound, "Round join"),
        (LineJoin::Bevel, Icon::JoinBevel, "Bevel join"),
    ];
    let join = row.put(ui, row.wide(), |ui| choice(ui, stroke.join, &joins));
    edit = join.map(Edit::Join).or(edit);
    if stroke.join == LineJoin::Miter {
        let row = Row::new(ui, "Miter limit");
        let miter = row.put(ui, row.field2(), |ui| {
            number(ui, s, stroke.miter_limit, 1.0..=100.0)
        });
        edit = miter.map(Edit::Miter).or(edit);
    }
    let row = Row::new(ui, "Dashes");
    if let Some(e) = row.put(ui, row.right_half(), |ui| dash_menu(ui, target, stroke)) {
        edit = Some(e);
    }
    if paint_edit::dash_preset(stroke) == DashPreset::Custom
        && let Some(e) = custom_dash(ui, target, stroke)
    {
        edit = Some(e);
    }
    edit
}

/// Three equal icon buttons (the name is the tooltip), the one for
/// `current` selected; the one clicked, if another.
fn choice<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    current: T,
    options: &[(T, Icon, &str)],
) -> Option<T> {
    let gap = 4.0;
    #[allow(clippy::cast_precision_loss)] // three options
    let n = options.len() as f32;
    let size = egui::vec2(
        (ui.available_width() - gap * (n - 1.0)) / n,
        ui.available_height(),
    );
    ui.spacing_mut().item_spacing.x = gap;
    let mut picked = None;
    for &(value, icon, name) in options {
        let r = icons::choice_button(ui, icon, size, current == value, name);
        if r.clicked() && current != value {
            picked = Some(value);
        }
    }
    picked
}

/// A number field filling its cell; the new value when the user changed
/// it. A drag is one undo step.
fn number(
    ui: &mut egui::Ui,
    s: &mut Session,
    value: f64,
    range: std::ops::RangeInclusive<f64>,
) -> Option<f64> {
    let mut v = value;
    let r = ui.add_sized(
        ui.available_size(),
        egui::DragValue::new(&mut v)
            .range(range)
            .speed(0.1)
            .max_decimals(2),
    );
    if r.drag_started() {
        s.begin_gesture();
    }
    let out = (r.changed() && (v - value).abs() > vextor_core::EPS).then_some(v);
    if r.drag_stopped() {
        s.end_gesture();
    }
    out
}

/// Solid / Dashed / Dotted / Custom; Custom starts from the current
/// dashes, or 4 2.
fn dash_menu(ui: &mut egui::Ui, target: Target, stroke: &Stroke) -> Option<Edit> {
    let current = paint_edit::dash_preset(stroke);
    let mut picked = current;
    egui::ComboBox::from_id_salt(("stroke-dash", target))
        .selected_text(current.name())
        .width(ui.available_width())
        .truncate()
        .show_ui(ui, |ui| {
            for p in DashPreset::PRESETS {
                ui.selectable_value(&mut picked, p, p.name());
            }
            ui.selectable_value(&mut picked, DashPreset::Custom, DashPreset::Custom.name());
        });
    if picked == current {
        return None;
    }
    if picked == DashPreset::Custom {
        let dash = if stroke.dash.is_empty() {
            vec![4.0, 2.0]
        } else {
            stroke.dash.clone()
        };
        return Some(Edit::Dash(dash, None));
    }
    let (dash, cap) = paint_edit::preset_dash(picked, stroke.width);
    Some(Edit::Dash(dash, cap))
}

/// The dash and gap lengths as typed ("6 3"), committed when the field
/// loses focus; an inline error while they do not parse.
fn custom_dash(ui: &mut egui::Ui, target: Target, stroke: &Stroke) -> Option<Edit> {
    let key = egui::Id::unique(("stroke-dash-text", target));
    let mut text = ui
        .data(|d| d.get_temp::<String>(key))
        .unwrap_or_else(|| paint_edit::format_dash(&stroke.dash));
    let mut edit = None;
    let row = Row::new(ui, "");
    row.put(ui, row.right_half(), |ui| {
        let r = ui
            .add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY))
            .on_hover_text("Dash and gap lengths, like 6 3");
        if r.changed() {
            ui.data_mut(|d| d.insert_temp(key, text.clone()));
        }
        if r.lost_focus()
            && let Ok(dash) = paint_edit::parse_dash(&text)
        {
            edit = Some(Edit::Dash(dash, None));
            ui.data_mut(|d| d.remove::<String>(key));
        }
    });
    if let Err(e) = paint_edit::parse_dash(&text) {
        ui.label(
            egui::RichText::new(e)
                .small()
                .color(ui.visuals().error_fg_color),
        );
    }
    edit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cap_or_join_icon_picks_its_option_and_the_tight_add_is_icon_wide() {
        let ctx = egui::Context::default();
        let joins = [
            (LineJoin::Miter, Icon::JoinMiter, "Miter join"),
            (LineJoin::Round, Icon::JoinRound, "Round join"),
            (LineJoin::Bevel, Icon::JoinBevel, "Bevel join"),
        ];
        let run = |events: Vec<egui::Event>| {
            let mut out = (None, egui::Rect::NOTHING, egui::Rect::NOTHING, false);
            let input = egui::RawInput {
                events,
                ..egui::RawInput::default()
            };
            ctx.run_ui(input, |ui| {
                let add = icons::tight_button(ui, Icon::Add, true, "", "");
                let cell = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(192.0, 20.0));
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(cell)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                out = (
                    choice(&mut child, LineJoin::Miter, &joins),
                    cell,
                    add.rect,
                    add.clicked(),
                );
            })
            .textures_delta
            .clear();
            out
        };
        let (_, cell, add, _) = run(vec![]);
        // The `+` is exactly its icon's size: the plus starts at its left
        // edge, and it is as tall as a swatch tile, so they line up.
        assert!((add.width() - icons::SIZE).abs() < 0.5, "{add:?}");
        assert!((add.height() - icons::SIZE).abs() < 0.5, "{add:?}");
        // Three equal buttons across 192 pt: the middle one is Round.
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let round = cell.center();
        run(vec![egui::Event::PointerMoved(round)]);
        run(vec![press(round, true)]);
        let (picked, ..) = run(vec![press(round, false)]);
        assert_eq!(picked, Some(LineJoin::Round));
        // The hand-drawn `+` still clicks.
        let plus = add.center();
        run(vec![egui::Event::PointerMoved(plus)]);
        run(vec![press(plus, true)]);
        let (.., clicked) = run(vec![press(plus, false)]);
        assert!(clicked, "the + clicks");
    }
}
