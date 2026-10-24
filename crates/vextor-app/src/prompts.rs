//! The small dialogs around files: "Save changes?", "Recover unsaved
//! work?" and the one-line message bar.

use eframe::egui::{self, Align2, Rect, Sense, Stroke, Vec2};

/// Width of a dialog's content, in points.
const DIALOG_WIDTH: f32 = 340.0;

/// Space between a dialog's border and its content, in points: wide at
/// the sides so wrapped text never runs close to the edge.
const DIALOG_MARGIN: egui::Margin = egui::Margin::symmetric(28, 18);

/// Width and height of every dialog button, so a row of them centres
/// exactly.
pub(crate) const BUTTON_SIZE: Vec2 = Vec2::new(96.0, 28.0);

/// The answer to "Save changes?".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveAnswer {
    Save,
    DontSave,
    Cancel,
}

/// The answer to "Recover unsaved work?".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoverAnswer {
    Recover,
    Discard,
}

/// Asks whether to save `name` before it is closed or replaced, centred
/// on `canvas`. `None` while the user has not answered; `Esc` or a click
/// outside cancels.
pub fn save_changes(ctx: &egui::Context, canvas: Rect, name: &str) -> Option<SaveAnswer> {
    let text = format!("“{name}” has unsaved changes. They will be lost if you don't save them.");
    let modal = dialog(ctx, "save_changes", canvas, |ui| {
        body(ui, "Save changes?", &text);
        buttons(
            ui,
            &[
                ("Save", SaveAnswer::Save),
                ("Don't Save", SaveAnswer::DontSave),
                ("Cancel", SaveAnswer::Cancel),
            ],
        )
    });
    if modal.should_close() && modal.inner.is_none() {
        return Some(SaveAnswer::Cancel);
    }
    modal.inner
}

/// Offers to recover `name`, left behind when Vextor did not exit cleanly,
/// centred on `canvas`. `None` while the user has not answered; the prompt
/// cannot be dismissed without an answer, so the work is never thrown away
/// by a stray click.
pub fn recover(ctx: &egui::Context, canvas: Rect, name: &str) -> Option<RecoverAnswer> {
    let text =
        format!("Vextor closed before {name} was saved. Recover the last autosaved version?");
    dialog(ctx, "recover", canvas, |ui| {
        body(ui, "Recover unsaved work?", &text);
        buttons(
            ui,
            &[
                ("Recover", RecoverAnswer::Recover),
                ("Discard", RecoverAnswer::Discard),
            ],
        )
    })
    .inner
}

/// Warns that `wanted` is missing and the text would be outlined in
/// `shown`. `Some(true)` converts, `Some(false)` (or `Esc`, or a click
/// outside) cancels, `None` while the user has not answered.
pub fn outline_missing_font(
    ctx: &egui::Context,
    canvas: Rect,
    wanted: &str,
    shown: Option<&str>,
) -> Option<bool> {
    let text = match shown {
        Some(shown) => format!(
            "“{wanted}” is not installed. The outlines will have the shape of {shown}, the font shown instead."
        ),
        None => format!("“{wanted}” is not installed and no font can stand in for it."),
    };
    let modal = dialog(ctx, "outline_missing_font", canvas, |ui| {
        body(ui, "Font missing", &text);
        let options: &[(&str, bool)] = if shown.is_some() {
            &[("Convert", true), ("Cancel", false)]
        } else {
            &[("Cancel", false)]
        };
        buttons(ui, options)
    });
    if modal.should_close() && modal.inner.is_none() {
        return Some(false);
    }
    modal.inner
}

/// Width of the shape size dialog's content, in points: one row of
/// label and field, as wide as its two buttons.
pub(crate) const SHAPE_DIALOG_WIDTH: f32 = BUTTON_SIZE.x * 2.0 + 8.0;
/// Width of the label column of the shape size dialog, in points: the
/// same as one button, so labels line up with Create and fields with
/// Cancel.
const SHAPE_LABEL_WIDTH: f32 = BUTTON_SIZE.x;
/// Size of every field in every shape size dialog, in points.
pub(crate) const SHAPE_FIELD_SIZE: Vec2 = Vec2::new(BUTTON_SIZE.x, 24.0);
/// Largest size the shape dialog accepts, in px (the largest artboard).
const MAX_SHAPE_SIZE: f64 = 10_000.0;

/// Asks for the size of a shape placed with a click (spec 06), editing
/// `size` in place. `Some(true)` (Create, or `Enter`) places it,
/// `Some(false)` (Cancel, `Esc`, a click outside) does not, `None` while
/// the user has not answered.
pub fn shape_size(
    ctx: &egui::Context,
    canvas: Rect,
    title: &str,
    size: &mut vextor_geom::ShapeSize,
) -> Option<bool> {
    use vextor_geom::ShapeSize as S;
    let modal = dialog_of_width(ctx, "shape_size", canvas, SHAPE_DIALOG_WIDTH, |ui| {
        ui.add_space(4.0);
        ui.heading(title);
        ui.add_space(10.0);
        match size {
            S::Rect { width, height } | S::Ellipse { width, height } => {
                length_row(ui, "Width", width);
                length_row(ui, "Height", height);
            }
            S::Polygon { radius, sides } => {
                length_row(ui, "Radius", radius);
                count_row(ui, "Sides", sides, vextor_core::MAX_POLYGON_SIDES);
            }
            S::Star {
                outer,
                inner,
                points,
            } => {
                length_row(ui, "Outer radius", outer);
                length_row(ui, "Inner radius", inner);
                count_row(ui, "Points", points, vextor_core::MAX_STAR_POINTS);
            }
            S::Line { length, angle } => {
                length_row(ui, "Length", length);
                field_row(ui, "Angle", |ui| {
                    ui.add_sized(
                        SHAPE_FIELD_SIZE,
                        egui::DragValue::new(angle)
                            .range(-360.0..=360.0)
                            .suffix("°")
                            .max_decimals(2),
                    );
                });
            }
        }
        ui.add_space(12.0);
        buttons(ui, &[("Create", true), ("Cancel", false)])
    });
    // After the fields, so a value typed and confirmed with Enter counts.
    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        return Some(true);
    }
    if modal.should_close() && modal.inner.is_none() {
        return Some(false);
    }
    modal.inner
}

/// One centred row of the shape size dialog: `label` in its column, then
/// the field `add` draws.
pub(crate) fn field_row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    let gap = ui.spacing().item_spacing.x;
    let row = SHAPE_LABEL_WIDTH + gap + SHAPE_FIELD_SIZE.x;
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - row) / 2.0).max(0.0));
        ui.allocate_ui_with_layout(
            Vec2::new(SHAPE_LABEL_WIDTH, SHAPE_FIELD_SIZE.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(SHAPE_LABEL_WIDTH);
                ui.label(label);
            },
        );
        add(ui);
    });
}

fn length_row(ui: &mut egui::Ui, label: &str, value: &mut f64) {
    field_row(ui, label, |ui| {
        ui.add_sized(
            SHAPE_FIELD_SIZE,
            egui::DragValue::new(value)
                .range(0.0..=MAX_SHAPE_SIZE)
                .suffix(" px")
                .max_decimals(2),
        );
    });
}

fn count_row(ui: &mut egui::Ui, label: &str, value: &mut u32, max: u32) {
    field_row(ui, label, |ui| {
        ui.add_sized(SHAPE_FIELD_SIZE, egui::DragValue::new(value).range(3..=max));
    });
}

/// A modal of fixed width whose centre is the centre of `canvas`, with its
/// content centred.
pub(crate) fn dialog<T>(
    ctx: &egui::Context,
    id: &str,
    canvas: Rect,
    content: impl FnOnce(&mut egui::Ui) -> T,
) -> egui::ModalResponse<T> {
    dialog_of_width(ctx, id, canvas, DIALOG_WIDTH, content)
}

/// [`dialog`] with content `width` points wide.
pub(crate) fn dialog_of_width<T>(
    ctx: &egui::Context,
    id: &str,
    canvas: Rect,
    width: f32,
    content: impl FnOnce(&mut egui::Ui) -> T,
) -> egui::ModalResponse<T> {
    let screen = ctx.content_rect();
    // Before the canvas has been laid out, centre on the window.
    let canvas = if canvas.is_positive() { canvas } else { screen };
    let offset = canvas.center() - screen.center();
    let id = egui::Id::unique(id);
    egui::Modal::new(id)
        .area(egui::Modal::default_area(id).anchor(Align2::CENTER_CENTER, offset))
        .frame(egui::Frame::popup(&ctx.global_style()).inner_margin(DIALOG_MARGIN))
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.vertical_centered(content).inner
        })
}

/// Heading and wrapped text, both centred.
pub(crate) fn body(ui: &mut egui::Ui, heading: &str, text: &str) {
    ui.add_space(4.0);
    ui.heading(heading);
    ui.add_space(4.0);
    ui.label(text);
    ui.add_space(12.0);
}

/// One row of equal-width buttons, centred. Returns the answer of the
/// button clicked this frame.
pub(crate) fn buttons<T: Copy>(ui: &mut egui::Ui, choices: &[(&str, T)]) -> Option<T> {
    let spacing = ui.spacing().item_spacing.x;
    #[allow(clippy::cast_precision_loss)]
    let count = choices.len() as f32;
    let row = BUTTON_SIZE.x * count + spacing * (count - 1.0);
    let mut answer = None;
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - row) / 2.0).max(0.0));
        for (label, value) in choices {
            if ui
                .add(egui::Button::new(*label).min_size(BUTTON_SIZE))
                .clicked()
            {
                answer = Some(*value);
            }
        }
    });
    ui.add_space(4.0);
    answer
}

/// One line under the menu bar showing `message`, with a close button that
/// clears it.
pub fn message_bar(ui: &mut egui::Ui, message: &mut Option<String>) {
    let Some(text) = message.as_deref() else {
        return;
    };
    let mut close = false;
    egui::Panel::top("message").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.colored_label(ui.visuals().warn_fg_color, text);
            if close_button(ui).on_hover_text("Dismiss").clicked() {
                close = true;
            }
        });
    });
    if close {
        *message = None;
    }
}

/// A small round button with a drawn ×, so it never depends on a font
/// having the glyph.
fn close_button(ui: &mut egui::Ui) -> egui::Response {
    let size = ui.spacing().interact_size.y;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let painter = ui.painter();
        painter.circle_filled(rect.center(), size / 2.0, visuals.weak_bg_fill);
        let arm = size * 0.2;
        let stroke = Stroke::new(1.5, visuals.fg_stroke.color);
        let c = rect.center();
        painter.line_segment([c + Vec2::new(-arm, -arm), c + Vec2::new(arm, arm)], stroke);
        painter.line_segment([c + Vec2::new(-arm, arm), c + Vec2::new(arm, -arm)], stroke);
    }
    response
}

#[cfg(test)]
mod tests {
    use eframe::egui::{Pos2, RawInput, Rect};

    use super::*;

    /// Runs a few frames of a 1280×800 window showing a dialog centred on
    /// `canvas`; returns the dialog's frame rect.
    fn dialog_rect(canvas: Rect) -> Rect {
        let ctx = egui::Context::default();
        let mut rect = Rect::NOTHING;
        for _ in 0..3 {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let modal = dialog(ui.ctx(), "test", canvas, |ui| {
                    body(ui, "Title", "Some text that wraps over a line or two.");
                    buttons(ui, &[("A", 1), ("B", 2)])
                });
                rect = modal.response.rect;
            });
            // Headless: no GPU to upload textures to.
            output.textures_delta.clear();
        }
        rect
    }

    /// The frame of the shape size dialog for `size`.
    fn shape_dialog_rect(mut size: vextor_geom::ShapeSize) -> Rect {
        let ctx = egui::Context::default();
        let mut rect = Rect::NOTHING;
        for _ in 0..3 {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let canvas = ui.ctx().content_rect();
                let _ = shape_size(ui.ctx(), canvas, "Shape", &mut size);
                rect = ui.ctx().memory(|m| {
                    m.area_rect(egui::Id::unique("shape_size"))
                        .unwrap_or(Rect::NOTHING)
                });
            });
            output.textures_delta.clear();
        }
        rect
    }

    #[test]
    fn every_shape_dialog_is_the_same_narrow_width() {
        use vextor_geom::ShapeSize as S;
        let sizes = [
            S::Rect {
                width: 1.0,
                height: 1.0,
            },
            S::Polygon {
                radius: 1.0,
                sides: 7,
            },
            S::Star {
                outer: 50.0,
                inner: 40.0,
                points: 20,
            },
            S::Line {
                length: 1.0,
                angle: 0.0,
            },
        ];
        let widths: Vec<f32> = sizes
            .into_iter()
            .map(|s| shape_dialog_rect(s).width())
            .collect();
        assert!(widths[0] > 0.0, "{widths:?}");
        assert!(
            widths.iter().all(|w| (w - widths[0]).abs() < 0.5),
            "{widths:?}"
        );
        let other = dialog_rect(Rect::NOTHING).width();
        assert!(widths[0] < other - 80.0, "{} vs {other}", widths[0]);
    }

    #[test]
    fn dialog_is_centred_on_the_canvas_not_the_window() {
        // Tools 48 px on the left, inspector 240 px on the right.
        let canvas = Rect::from_min_max(Pos2::new(48.0, 30.0), Pos2::new(1040.0, 800.0));
        let rect = dialog_rect(canvas);
        assert!(
            (rect.center() - canvas.center()).length() < 1.0,
            "dialog {rect:?} centre should be canvas centre {:?}",
            canvas.center()
        );
    }

    #[test]
    fn dialog_falls_back_to_the_window_centre_without_a_canvas() {
        let rect = dialog_rect(Rect::NOTHING);
        assert!(
            (rect.center() - Pos2::new(640.0, 400.0)).length() < 1.0,
            "{rect:?}"
        );
    }
}
