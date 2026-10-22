//! Inspector ▸ Artboards (spec 08): the document's artboards in export
//! order, to pick the current one, add, reorder and delete; then
//! Inspector ▸ Artboard (spec 05e): the current artboard's name, size and
//! background. Both show while nothing is selected. X and Y move the
//! artboard with the objects on it (spec 08b); W and H resize it about its
//! centre, the objects staying where they are.

use eframe::egui;
use vextor_core::Artboard;

use super::PanelAction;
use super::text::{color_tile, hex_field, section_toggle};
use super::{FIELD_SIZE, GRID_SPACING, Label};
use crate::artboard::{self, DEFAULT_BACKGROUND, MAX_NAME_CHARS};
use crate::icons::{self, Icon};
use crate::new_doc::MAX_SIDE;
use crate::session::Session;

/// Where the X | Y fields sit, so the Name and Colour rows line up with
/// them: the left and right edge of X and the right edge of Y.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Columns {
    start: f32,
    x_end: f32,
    end: f32,
}

impl Columns {
    /// The part of `row` under the X field.
    fn x(self, row: egui::Rect) -> egui::Rect {
        egui::Rect::from_x_y_ranges(self.start..=self.x_end, row.y_range())
    }

    /// The part of `row` from the X field to the end of the Y field.
    fn both(self, row: egui::Rect) -> egui::Rect {
        egui::Rect::from_x_y_ranges(self.start..=self.end, row.y_range())
    }

    /// The part of `row` from the Y label to the end of the Y field.
    fn y(self, row: egui::Rect) -> egui::Rect {
        egui::Rect::from_x_y_ranges(self.x_end + GRID_SPACING[0]..=self.end, row.y_range())
    }
}

/// Where the Artboard section put its fields (read by tests).
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug)]
struct Placed {
    x: egui::Rect,
    y: egui::Rect,
    h: egui::Rect,
    name: egui::Rect,
    tile: Option<egui::Rect>,
    hex: Option<egui::Rect>,
}

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
        body(ui, s, &board);
    });
}

/// Name, X | Y, W | H and background. The Name and Colour fields are
/// placed once the X | Y columns are known, so they line up with them.
fn body(ui: &mut egui::Ui, s: &mut Session, board: &Artboard) -> Placed {
    let name_row = label_row(ui, "Name");
    let [x, y, h] = geometry_fields(ui, s, board);
    let columns = Columns {
        start: x.left(),
        x_end: x.right(),
        end: y.right(),
    };
    let name = name_field(ui, s, board, columns.both(name_row));
    let (tile, hex) = background_fields(ui, s, board, columns);
    Placed {
        x,
        y,
        h,
        name,
        tile,
        hex,
    }
}

/// A row holding only `label`; its rect, for fields placed later.
fn label_row(ui: &mut egui::Ui, label: &str) -> egui::Rect {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.allocate_exact_size(egui::vec2(0.0, FIELD_SIZE[1]), egui::Sense::hover());
    })
    .response
    .rect
}

/// `add` laid out inside `rect`, with interact widgets as large as it.
/// The parent's cursor stays put: `rect` lies in a row already allocated.
fn place<R>(ui: &mut egui::Ui, rect: egui::Rect, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    child.spacing_mut().interact_size = rect.size();
    add(&mut child)
}

/// X | Y, then W | H; returns the X, Y and H fields' rects. X and Y move the artboard with the objects on it;
/// W and H resize it about its centre, the objects staying where they are.
fn geometry_fields(ui: &mut egui::Ui, s: &mut Session, board: &Artboard) -> [egui::Rect; 3] {
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
            let x = number(ui, s, x_label, rect.x, place.clone(), moves, |s, value| {
                artboard::move_to(s, board.id, value, rect.y);
            });
            let y = number(ui, s, y_label, rect.y, place, moves, |s, value| {
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
            let h = number(ui, s, h_label, rect.height, side, resizes, |s, value| {
                artboard::resize(s, board.id, rect.width, value);
            });
            ui.end_row();
            [x, y, h]
        })
        .inner
}

/// A labelled number field in `range` (`hover`: what changing it does)
/// that calls `apply` with a changed value. A drag is one history gesture.
/// Returns the field's rect.
fn number(
    ui: &mut egui::Ui,
    s: &mut Session,
    label: Label,
    value: f64,
    range: std::ops::RangeInclusive<f64>,
    hover: &str,
    apply: impl FnOnce(&mut Session, f64),
) -> egui::Rect {
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
    r.rect
}

/// The name in `rect`, committed when the field loses focus (Enter or a
/// click elsewhere); `Esc` puts the old name back. Returns its rect.
fn name_field(ui: &mut egui::Ui, s: &mut Session, a: &Artboard, rect: egui::Rect) -> egui::Rect {
    let key = egui::Id::unique("artboard-name-draft");
    let mut text = ui
        .data(|d| d.get_temp::<String>(key))
        .unwrap_or_else(|| a.name.clone());
    let r = place(ui, rect, |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut text)
                .char_limit(MAX_NAME_CHARS)
                .desired_width(f32::INFINITY),
        )
    });
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
    r.rect
}

/// Background on/off and its colour: a tile under the X field, the hex
/// value under Y. Changes made while the pointer is held in the colour
/// picker are one undo step. Returns the tile's and hex field's rects.
fn background_fields(
    ui: &mut egui::Ui,
    s: &mut Session,
    a: &Artboard,
    columns: Columns,
) -> (Option<egui::Rect>, Option<egui::Rect>) {
    if section_toggle(ui, "Background", a.background.is_some()) {
        let next = a.background.is_none().then_some(DEFAULT_BACKGROUND);
        artboard::set_background(s, a.id, next);
    }
    let gesture = egui::Id::unique("artboard-background-gesture");
    let in_gesture = ui.data(|d| d.get_temp::<bool>(gesture)).unwrap_or(false);
    let down = ui.input(|i| i.pointer.any_down());
    let mut placed = (None, None);
    if let Some(current) = a.background {
        let row = label_row(ui, "Colour");
        let (tile, hex) = (columns.x(row), columns.y(row));
        let picked = place(ui, tile, |ui| color_tile(ui, current));
        let typed = place(ui, hex, |ui| {
            hex_field(ui, "artboard-background", current, f32::INFINITY)
        });
        if let Some(c) = typed.or(picked) {
            if down && !in_gesture {
                s.begin_gesture();
                ui.data_mut(|d| d.insert_temp(gesture, true));
            }
            artboard::set_background(s, a.id, Some(c));
        }
        placed = (Some(tile), Some(hex));
    }
    if in_gesture && !down {
        s.end_gesture();
        ui.data_mut(|d| d.remove::<bool>(gesture));
    }
    placed
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

    #[test]
    fn name_and_colour_line_up_with_the_x_and_y_fields_without_overlapping() {
        let mut s = Session::new(new_document("card", 320.0, 200.0));
        let id = s.doc.artboards[0].id;
        artboard::set_background(&mut s, id, Some(DEFAULT_BACKGROUND));
        let board = s.doc.artboards[0].clone();
        let ctx = egui::Context::default();
        let mut placed = None;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            placed = Some(body(ui, &mut s, &board));
        });
        out.textures_delta.clear();
        let placed = placed.expect("section shown");
        let (tile, hex) = (placed.tile.expect("tile"), placed.hex.expect("hex"));
        let near = |a: f32, b: f32| (a - b).abs() < 0.5;
        assert!(near(placed.name.left(), placed.x.left()));
        assert!(near(placed.name.right(), placed.y.right()));
        assert!(near(tile.left(), placed.x.left()));
        assert!(near(tile.right(), placed.x.right()));
        assert!(hex.left() > tile.right());
        assert!(near(hex.right(), placed.y.right()));
        // Name above X | Y, the Colour row below W | H.
        assert!(placed.name.bottom() <= placed.x.top());
        assert!(tile.top() > placed.h.bottom());
    }
}
