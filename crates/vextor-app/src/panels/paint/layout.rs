//! Rows for the Fill and Stroke sections on the Transform grid (label |
//! field | label | field, `GRID_WIDTH` wide), so their controls start and
//! end where the fields of Transform, Corners and Artboard do:
//!
//! - menus and text fields take the right half (under Y's label and
//!   field), a trailing icon button its last [`BUTTON`] points;
//! - single numbers take the right-hand field column;
//! - a colour tile takes the left-hand field column;
//! - a slider runs from the left-hand field to the end of the second
//!   label column, its number in the right-hand field column;
//! - button groups (Cap, Join) and wrapped tiles take field to end.

use eframe::egui::{self, Rect};

use super::super::{FIELD_SIZE, GRID_SPACING, GRID_WIDTH, ICON_LABEL_WIDTH};

/// Space between groups of rows (paint kind, gradient, stops, colour,
/// swatches, stroke shape).
pub(super) const GROUP_GAP: f32 = 12.0;
/// Width of an icon button closing a row (bin, pipette).
pub(super) const BUTTON: f32 = 28.0;
/// Gap between a field and its trailing button.
const BUTTON_GAP: f32 = 4.0;

/// Left edge of the left-hand field column.
const FIELD1: f32 = ICON_LABEL_WIDTH + GRID_SPACING[0];
/// Left edge of the second label column (the right half).
const LABEL2: f32 = FIELD1 + FIELD_SIZE[0] + GRID_SPACING[0];
/// Left edge of the right-hand field column.
const FIELD2: f32 = LABEL2 + ICON_LABEL_WIDTH + GRID_SPACING[0];

/// One grid row: `GRID_WIDTH` wide, as tall as a field (or an interactive
/// widget, if taller), with its label drawn at the left.
#[derive(Clone, Copy)]
pub(super) struct Row(Rect);

impl Row {
    /// Allocates a row labelled `label` (empty: none).
    pub(super) fn new(ui: &mut egui::Ui, label: &str) -> Self {
        let height = FIELD_SIZE[1].max(ui.spacing().interact_size.y);
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(GRID_WIDTH, height), egui::Sense::hover());
        let row = Self(rect);
        if !label.is_empty() {
            row.put(ui, row.span(0.0, GRID_WIDTH), |ui| ui.label(label));
        }
        row
    }

    fn span(self, from: f32, to: f32) -> Rect {
        Rect::from_x_y_ranges(self.0.left() + from..=self.0.left() + to, self.0.y_range())
    }

    /// The right half: under the second label and field.
    pub(super) fn right_half(self) -> Rect {
        self.span(LABEL2, GRID_WIDTH)
    }

    /// The right half less a trailing button, and that button.
    pub(super) fn right_half_and_button(self) -> (Rect, Rect) {
        let end = GRID_WIDTH - BUTTON;
        (
            self.span(LABEL2, end - BUTTON_GAP),
            self.span(end, GRID_WIDTH),
        )
    }

    /// The left-hand field column.
    pub(super) fn field1(self) -> Rect {
        self.span(FIELD1, FIELD1 + FIELD_SIZE[0])
    }

    /// The right-hand field column.
    pub(super) fn field2(self) -> Rect {
        self.span(FIELD2, GRID_WIDTH)
    }

    /// From the left-hand field to the end of the second label column.
    pub(super) fn slider(self) -> Rect {
        self.span(FIELD1, LABEL2 + ICON_LABEL_WIDTH)
    }

    /// From the left-hand field to the end.
    pub(super) fn wide(self) -> Rect {
        self.span(FIELD1, GRID_WIDTH)
    }

    /// `add` laid out in `rect` (kept within the row; left to right,
    /// centred vertically), with interactive widgets as tall as the row.
    /// The parent's cursor stays put: the row is already allocated.
    pub(super) fn put<R>(
        self,
        ui: &mut egui::Ui,
        rect: Rect,
        add: impl FnOnce(&mut egui::Ui) -> R,
    ) -> R {
        let rect = rect.intersect(self.0);
        let layout = egui::Layout::left_to_right(egui::Align::Center);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
        child.spacing_mut().interact_size.y = rect.height();
        add(&mut child)
    }
}

/// Left edge, from a row's start, of the left-hand field column (for
/// content that wraps, like swatches).
pub(super) const WIDE_START: f32 = FIELD1;

/// A gap between groups of rows.
pub(super) fn group_gap(ui: &mut egui::Ui) {
    ui.add_space(GROUP_GAP);
}

#[cfg(test)]
mod tests {
    use super::super::super::Label;
    use super::*;
    use crate::icons::Icon;

    /// Compares within half a point.
    fn same(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.5
    }

    #[test]
    fn paint_rows_line_up_with_the_transform_fields() {
        let ctx = egui::Context::default();
        let (mut x, mut y, mut row) = (Rect::NOTHING, Rect::NOTHING, None);
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                // A row of the Transform grid, built as Transform builds it.
                egui::Grid::new("transform-like")
                    .num_columns(4)
                    .spacing(GRID_SPACING)
                    .show(ui, |ui| {
                        Label("X", Icon::PosX, "").show(ui);
                        x = ui
                            .add_sized(FIELD_SIZE, egui::DragValue::new(&mut 0.0))
                            .rect;
                        Label("Y", Icon::PosY, "").show(ui);
                        y = ui
                            .add_sized(FIELD_SIZE, egui::DragValue::new(&mut 0.0))
                            .rect;
                        ui.end_row();
                    });
                row = Some(Row::new(ui, "Paint"));
            })
            .textures_delta
            .clear();
        }
        let row = row.expect("a row");
        let (field1, field2, half) = (row.field1(), row.field2(), row.right_half());
        assert!(same(field1.left(), x.left()) && same(field1.right(), x.right()));
        assert!(same(field2.left(), y.left()) && same(field2.right(), y.right()));
        // The right half starts under Y's label: one gap after X's field.
        assert!(same(half.left(), x.right() + GRID_SPACING[0]));
        assert!(same(half.right(), y.right()));
        let (field, button) = row.right_half_and_button();
        assert!(same(button.right(), y.right()) && same(button.width(), BUTTON));
        assert!(field.right() < button.left());
        assert!(same(row.slider().left(), x.left()));
        assert!(same(row.slider().right(), y.left() - GRID_SPACING[0]));
    }

    #[test]
    fn a_menu_with_a_long_choice_stays_in_its_cell() {
        // "Linear gradient" (or a long gradient name) must not push the
        // menu past the number fields' right edge.
        let ctx = egui::Context::default();
        let mut cells = Vec::new();
        for _ in 0..2 {
            cells.clear();
            ctx.run_ui(egui::RawInput::default(), |ui| {
                let row = Row::new(ui, "Paint");
                for (i, cell) in [row.wide(), row.right_half()].into_iter().enumerate() {
                    let menu = row.put(ui, cell, |ui| {
                        egui::ComboBox::from_id_salt(("long", i))
                            .selected_text("Linear gradient · a very long gradient name · 12 uses")
                            .width(ui.available_width())
                            .truncate()
                            .show_ui(ui, |_| {})
                            .response
                            .rect
                    });
                    cells.push((cell, menu));
                }
            })
            .textures_delta
            .clear();
        }
        for (cell, menu) in cells {
            assert!(same(menu.left(), cell.left()), "{menu:?} in {cell:?}");
            assert!(same(menu.right(), cell.right()), "{menu:?} in {cell:?}");
        }
    }
}
