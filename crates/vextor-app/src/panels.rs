//! The side and bottom panels. Layers and Code are placeholders until
//! their specs fill them.

use eframe::egui;
use kurbo::{Affine, Rect};
use vextor_geom::{rotation_degrees, union_bounds};

use vextor_core::ToolId;

use crate::edit_ops;
use crate::icons::{self, Icon};
use crate::session::Session;

/// Gap between a label and its field, and between rows, in points.
const GRID_SPACING: [f32; 2] = [12.0, 6.0];
/// Width of the label column in the Text section's grids, so their
/// fields (colour swatches and hex values included) line up.
const LABEL_WIDTH: f32 = 84.0;
/// Size of every number field: fixed, so typing or dragging a longer
/// value never shifts the panel.
const FIELD_SIZE: [f32; 2] = [64.0, 20.0];
/// Space kept free at the right of the inspector's content, matching the
/// panel's own margin on the left (the scroll bar floats over it).
const RIGHT_MARGIN: i8 = 10;
/// Starting height of the Layers area, in points.
const LAYERS_HEIGHT: f32 = 160.0;

mod arrange;
mod artboard;
mod fonts;
mod restyle;
mod shape;
mod text;

pub use arrange::PanelAction;
pub use fonts::recent_families;

/// Tools (left): the active one is highlighted. Returns the tool clicked.
pub fn tools(ui: &mut egui::Ui, active: Option<ToolId>) -> Option<ToolId> {
    ui.add_space(6.0);
    let mut picked = None;
    ui.vertical_centered(|ui| {
        for (tool, key, hint) in [
            (ToolId::Select, "V", "Select (V)"),
            (
                ToolId::Pen,
                "P",
                "Pen (P): click for straight, drag for curves, click the first node to close. \
                 ⇧ angle steps, ⌥ breaks a handle, Enter / Esc ends, ⌫ removes the last node",
            ),
            (
                ToolId::Text,
                "T",
                "Text (T): click to add a label, then type",
            ),
            (
                ToolId::Rect,
                "R",
                "Rectangle (R): drag to draw, click for a size. ⇧ square, ⌥ from centre",
            ),
            (
                ToolId::Ellipse,
                "E",
                "Ellipse (E): drag to draw, click for a size. ⇧ circle, ⌥ from centre",
            ),
            (
                ToolId::Polygon,
                "G",
                "Polygon (G): drag to draw, ↑↓ while dragging for sides. ⌥ from centre",
            ),
            (
                ToolId::Star,
                "S",
                "Star (S): drag to draw, ↑↓ points, ←→ inner radius. ⌥ from centre",
            ),
            (
                ToolId::Line,
                "L",
                "Line (L): drag to draw. ⇧ 45° steps, ⌥ from centre",
            ),
        ] {
            let button = egui::Button::selectable(active == Some(tool), key)
                .min_size(egui::vec2(32.0, 32.0));
            if ui
                .add_enabled(active.is_some(), button)
                .on_hover_text(hint)
                .clicked()
            {
                picked = Some(tool);
            }
        }
    });
    picked
}

/// Inspector and Layers (right). Returns a message for the user when an
/// edit was refused, and the Align / Group buttons clicked, for the app
/// to run like their menu items. `to_artboard` is Align to Artboard.
pub fn inspector_and_layers(
    ui: &mut egui::Ui,
    session: Option<&mut Session>,
    to_artboard: &mut bool,
) -> (Option<String>, Vec<PanelAction>) {
    // Layers keep their own resizable area at the bottom; the inspector
    // scrolls in the space above, so neither draws past the panel.
    egui::Panel::bottom("layers")
        .resizable(true)
        .default_size(LAYERS_HEIGHT)
        .min_size(48.0)
        .show(ui, |ui| {
            ui.add_space(6.0);
            panel_title(ui, "Layers");
            egui::ScrollArea::vertical()
                .id_salt("layers-scroll")
                .auto_shrink(false)
                .show(ui, |ui| {
                    right_margin(ui, |ui| ui.weak("The layer list will appear here."));
                });
        });
    egui::ScrollArea::vertical()
        .id_salt("inspector-scroll")
        .auto_shrink(false)
        .show(ui, |ui| {
            right_margin(ui, |ui| inspector_sections(ui, session, to_artboard))
        })
        .inner
}

/// A panel's title (Inspector, Layers), in the title yellow so it reads
/// above its sections, as "New document" does on the start screen.
fn panel_title(ui: &mut egui::Ui, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .strong()
            .color(crate::theme::YELLOW_SOFT),
    );
}

/// `add` with [`RIGHT_MARGIN`] kept free on its right.
fn right_margin<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .inner_margin(egui::Margin {
            right: RIGHT_MARGIN,
            ..egui::Margin::ZERO
        })
        .show(ui, add)
        .inner
}

/// The inspector's title and its sections for the current selection.
fn inspector_sections(
    ui: &mut egui::Ui,
    session: Option<&mut Session>,
    to_artboard: &mut bool,
) -> (Option<String>, Vec<PanelAction>) {
    ui.add_space(6.0);
    panel_title(ui, "Inspector");
    let mut actions = Vec::new();
    let message = if let Some(s) = session {
        if s.selected().is_empty() {
            artboard::section(ui, s);
            return (None, actions);
        }
        let message = inspector(ui, s);
        let can = edit_ops::can(s);
        arrange::align_section(ui, can, to_artboard, &mut actions);
        arrange::group_section(ui, can, &mut actions);
        message
    } else {
        ui.weak("Nothing selected.");
        None
    };
    (message, actions)
}

/// A collapsible inspector section, open until the user folds it (egui
/// remembers it per title).
fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(8.0);
    ui.separator();
    egui::CollapsingHeader::new(egui::RichText::new(title).strong())
        .id_salt(title)
        .default_open(true)
        .show(ui, add);
}

/// X, Y, W, H of the selection box and the rotation of a single object.
/// Dragging a value is one undo step; typing one is one step per change.
fn inspector(ui: &mut egui::Ui, s: &mut Session) -> Option<String> {
    let ids = s.selected();
    let Some(b) = union_bounds(&s.doc, &s.fonts, &ids) else {
        ui.weak("Nothing selected.");
        return None;
    };
    let rotation = match ids.as_slice() {
        [one] => Some(rotation_degrees(s.doc.world_transform(*one))),
        _ => None,
    };
    let mut edit = None;
    section(ui, "Transform", |ui| {
        egui::Grid::new("inspector")
            .num_columns(4)
            .spacing(GRID_SPACING)
            .show(ui, |ui| {
                let x_label = Label("X", Icon::PosX, "Left edge (X)");
                let y_label = Label("Y", Icon::PosY, "Top edge (Y)");
                field(ui, s, x_label, b.x0, true, &mut edit, Change::X);
                field(ui, s, y_label, b.y0, true, &mut edit, Change::Y);
                ui.end_row();
                let w_ok = b.width() > vextor_core::EPS;
                let h_ok = b.height() > vextor_core::EPS;
                let w_label = Label("W", Icon::Width, "Width (W)");
                let h_label = Label("H", Icon::Height, "Height (H)");
                field(ui, s, w_label, b.width(), w_ok, &mut edit, Change::W);
                field(ui, s, h_label, b.height(), h_ok, &mut edit, Change::H);
                ui.end_row();
                let angle = Label("°", Icon::Angle, "Rotation in degrees");
                if let Some(deg) = rotation {
                    field(ui, s, angle, deg, true, &mut edit, |v| {
                        Change::Rotation(v - deg)
                    });
                } else {
                    angle.show(ui);
                    ui.add_enabled(false, egui::DragValue::new(&mut 0.0))
                        .on_disabled_hover_text(
                            "Rotate one object at a time here, or use the corners",
                        );
                }
                ui.end_row();
            });
    });
    let shape_message = shape::section(ui, s);
    let text_message = text::section(ui, s).or(shape_message);
    let Some(change) = edit else {
        return text_message;
    };
    let keep_ratio = edit_ops::has_text(&s.doc, &ids);
    let (affine, label) = change.affine(b, keep_ratio);
    match change {
        Change::W(_) | Change::H(_) => edit_ops::resize(s, affine),
        _ => edit_ops::transform(s, affine, label),
    }
    .err()
}

/// One edit from the inspector.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Change {
    X(f64),
    Y(f64),
    W(f64),
    H(f64),
    /// Degrees to add.
    Rotation(f64),
}

impl Change {
    /// The document-space change for box `b`, and its undo label. With
    /// `keep_ratio` (text selected) W and H scale both sides alike.
    fn affine(self, b: Rect, keep_ratio: bool) -> (Affine, &'static str) {
        let about = |sx: f64, sy: f64| {
            Affine::translate((b.x0, b.y0))
                * Affine::scale_non_uniform(sx, sy)
                * Affine::translate((-b.x0, -b.y0))
        };
        match self {
            Self::X(x) => (Affine::translate((x - b.x0, 0.0)), "Move"),
            Self::Y(y) => (Affine::translate((0.0, y - b.y0)), "Move"),
            Self::W(w) => {
                let k = w.max(0.01) / b.width();
                (about(k, if keep_ratio { k } else { 1.0 }), "Resize")
            }
            Self::H(h) => {
                let k = h.max(0.01) / b.height();
                (about(if keep_ratio { k } else { 1.0 }, k), "Resize")
            }
            Self::Rotation(deg) => (Affine::rotate_about(deg.to_radians(), b.center()), "Rotate"),
        }
    }
}

/// A labelled number. A drag is wrapped in a history gesture.
/// A field's label: its letter, then a small icon saying what it is.
#[derive(Clone, Copy)]
struct Label(&'static str, Icon, &'static str);

impl Label {
    fn show(self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(self.0);
            icons::label(ui, self.1, self.2);
        });
    }
}

fn field(
    ui: &mut egui::Ui,
    s: &mut Session,
    label: Label,
    value: f64,
    enabled: bool,
    edit: &mut Option<Change>,
    change: impl Fn(f64) -> Change,
) {
    label.show(ui);
    let mut v = value;
    let r = ui
        .add_enabled_ui(enabled, |ui| {
            ui.add_sized(
                FIELD_SIZE,
                egui::DragValue::new(&mut v).speed(1.0).max_decimals(2),
            )
        })
        .inner;
    if r.drag_started() {
        s.begin_gesture();
    }
    if r.changed() && (v - value).abs() > vextor_core::EPS {
        *edit = Some(change(v));
    }
    if r.drag_stopped() {
        s.end_gesture();
    }
}

/// Code (bottom).
pub fn code(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.strong("Code");
    ui.weak("The SVG of the artboard will appear here.");
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Rect = Rect::new(10.0, 20.0, 50.0, 40.0);

    #[test]
    fn size_fields_scale_from_the_top_left() {
        let (a, _) = Change::W(80.0).affine(B, false);
        assert_eq!(
            a * kurbo::Point::new(10.0, 20.0),
            kurbo::Point::new(10.0, 20.0)
        );
        assert_eq!(
            a * kurbo::Point::new(50.0, 40.0),
            kurbo::Point::new(90.0, 40.0)
        );
    }

    #[test]
    fn size_fields_keep_proportions_for_text() {
        let (a, _) = Change::H(40.0).affine(B, true);
        assert_eq!(
            a * kurbo::Point::new(50.0, 40.0),
            kurbo::Point::new(90.0, 60.0)
        );
    }

    #[test]
    fn position_fields_move_the_box_there() {
        let (a, label) = Change::Y(0.0).affine(B, false);
        assert_eq!(
            a * kurbo::Point::new(10.0, 20.0),
            kurbo::Point::new(10.0, 0.0)
        );
        assert_eq!(label, "Move");
    }

    #[test]
    fn rotation_turns_about_the_centre() {
        let (a, _) = Change::Rotation(180.0).affine(B, false);
        let p = a * kurbo::Point::new(10.0, 20.0);
        assert!((p - kurbo::Point::new(50.0, 40.0)).hypot() < 1e-9);
    }
}
