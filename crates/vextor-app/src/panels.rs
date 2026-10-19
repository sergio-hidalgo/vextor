//! The side and bottom panels. Layers and Code are placeholders until
//! their specs fill them.

use eframe::egui;
use kurbo::{Affine, Rect};
use vextor_geom::{rotation_degrees, union_bounds};

use vextor_core::ToolId;

use crate::edit_ops;
use crate::session::Session;

/// Gap between a label and its field, and between rows, in points.
const GRID_SPACING: [f32; 2] = [12.0, 6.0];

mod fonts;
mod restyle;
mod text;

pub use fonts::recent_families;

/// Tools (left): the active one is highlighted. Returns the tool clicked.
pub fn tools(ui: &mut egui::Ui, active: Option<ToolId>) -> Option<ToolId> {
    ui.add_space(6.0);
    let mut picked = None;
    ui.vertical_centered(|ui| {
        for (tool, key, hint) in [
            (ToolId::Select, "V", "Select (V)"),
            (
                ToolId::Text,
                "T",
                "Text (T): click to add a label, then type",
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
/// edit was refused.
pub fn inspector_and_layers(ui: &mut egui::Ui, session: Option<&mut Session>) -> Option<String> {
    ui.add_space(6.0);
    ui.strong("Inspector");
    let message = if let Some(s) = session {
        inspector(ui, s)
    } else {
        ui.weak("Nothing selected.");
        None
    };
    ui.add_space(12.0);
    ui.separator();
    ui.strong("Layers");
    ui.weak("The layer list will appear here.");
    message
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
    egui::Grid::new("inspector")
        .num_columns(4)
        .spacing([6.0, 6.0])
        .show(ui, |ui| {
            field(ui, s, "X", b.x0, true, &mut edit, Change::X);
            field(ui, s, "Y", b.y0, true, &mut edit, Change::Y);
            ui.end_row();
            let w_ok = b.width() > vextor_core::EPS;
            let h_ok = b.height() > vextor_core::EPS;
            field(ui, s, "W", b.width(), w_ok, &mut edit, Change::W);
            field(ui, s, "H", b.height(), h_ok, &mut edit, Change::H);
            ui.end_row();
            if let Some(deg) = rotation {
                field(ui, s, "°", deg, true, &mut edit, |v| {
                    Change::Rotation(v - deg)
                });
            } else {
                ui.label("°");
                ui.add_enabled(false, egui::DragValue::new(&mut 0.0))
                    .on_disabled_hover_text("Rotate one object at a time here, or use the corners");
            }
            ui.end_row();
        });
    let text_message = text::section(ui, s);
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
fn field(
    ui: &mut egui::Ui,
    s: &mut Session,
    label: &str,
    value: f64,
    enabled: bool,
    edit: &mut Option<Change>,
    change: impl Fn(f64) -> Change,
) {
    ui.label(label);
    let mut v = value;
    let r = ui.add_enabled(
        enabled,
        egui::DragValue::new(&mut v).speed(1.0).max_decimals(2),
    );
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
