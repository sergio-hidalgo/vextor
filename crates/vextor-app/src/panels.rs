//! The side and bottom panels. Code is a placeholder until its spec
//! fills it.

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
/// Width of an icon label's column in the four-column grids (Transform,
/// Artboard, Corners): fixed, so their fields line up from section to
/// section whatever letter or icon labels them. It equals egui's minimum
/// grid column width (the interact width, 40), which those grids apply
/// anyway, so the constant says what is on screen.
const ICON_LABEL_WIDTH: f32 = 40.0;
/// Size of every number field: fixed, so typing or dragging a longer
/// value never shifts the panel.
const FIELD_SIZE: [f32; 2] = [64.0, 20.0];
/// Width of a four-column grid (label, field, label, field): where its
/// right-hand fields end. Rows of buttons end there too.
const GRID_WIDTH: f32 = 2.0 * ICON_LABEL_WIDTH + 2.0 * FIELD_SIZE[0] + 3.0 * GRID_SPACING[0];
/// Space kept free at the right of the inspector's content, matching the
/// panel's own margin on the left (the scroll bar floats over it).
const RIGHT_MARGIN: i8 = 10;
/// Empty space under the inspector's last section, so scrolled to the
/// end it sits clear of the Layers panel instead of against its edge.
const INSPECTOR_END_SPACE: f32 = 24.0;
/// Starting height of the Layers area, in points.
const LAYERS_HEIGHT: f32 = 220.0;

mod arrange;
mod artboard;
mod fonts;
mod layers;
mod paint;
mod path;
mod restyle;
mod shape;
mod text;

pub use arrange::PanelAction;
pub use fonts::recent_families;
pub use paint::escape_bad_hex;

/// The tools strip, in groups: tool, icon, tooltip (name and key).
const TOOLS: [&[(ToolId, Icon, &str)]; 5] = [
    &[
        (ToolId::Select, Icon::ToolSelect, "Select (V)"),
        (ToolId::Node, Icon::ToolNode, "Nodes (A)"),
    ],
    &[(ToolId::Pen, Icon::ToolPen, "Pen (P)")],
    &[(ToolId::Text, Icon::ToolText, "Text (T)")],
    &[
        (ToolId::Rect, Icon::ToolRect, "Rectangle (R)"),
        (ToolId::Ellipse, Icon::ToolEllipse, "Ellipse (E)"),
        (ToolId::Polygon, Icon::ToolPolygon, "Polygon (G)"),
        (ToolId::Star, Icon::ToolStar, "Star (S)"),
        (ToolId::Line, Icon::ToolLine, "Line (L)"),
    ],
    &[(ToolId::Zoom, Icon::ToolZoom, "Zoom (Z)")],
];

/// Tools (left): one icon per tool, in groups (selecting, drawing, text,
/// shapes, zoom); the active one is highlighted. Returns the tool clicked.
pub fn tools(ui: &mut egui::Ui, active: Option<ToolId>) -> Option<ToolId> {
    ui.add_space(6.0);
    let mut picked = None;
    ui.vertical_centered(|ui| {
        for (i, group) in TOOLS.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            for &(tool, icon, tooltip) in *group {
                let on = active == Some(tool);
                if icons::tool_button(ui, icon, on, active.is_some(), tooltip).clicked() {
                    picked = Some(tool);
                }
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
    mut session: Option<&mut Session>,
    to_artboard: &mut bool,
) -> (Option<String>, Vec<PanelAction>) {
    // Layers keep their own resizable area at the bottom; the inspector
    // scrolls in the space above, so neither draws past the panel.
    let layers_message = egui::Panel::bottom("layers")
        .resizable(true)
        .default_size(LAYERS_HEIGHT)
        .min_size(48.0)
        .show(ui, |ui| {
            ui.add_space(6.0);
            panel_title(ui, "Layers");
            if let Some(s) = session.as_deref_mut() {
                layers::show(ui, s)
            } else {
                ui.weak("No document open.");
                None
            }
        })
        .inner;
    let (message, actions) = egui::ScrollArea::vertical()
        .id_salt("inspector-scroll")
        .auto_shrink(false)
        .show(ui, |ui| {
            let out = right_margin(ui, |ui| inspector_sections(ui, session, to_artboard));
            ui.add_space(INSPECTOR_END_SPACE);
            out
        })
        .inner;
    (layers_message.or(message), actions)
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
        // Path nodes or a segment selected (spec 07): only what edits them.
        if crate::path_ops::editing_nodes(s) {
            let message = path::sections(ui, s);
            // Align and distribute the selected nodes (spec 07).
            let nodes = crate::path_ops::selected_nodes(s).len();
            let can = edit_ops::Can {
                edit: nodes >= 1,
                distribute: nodes >= 3,
                ..edit_ops::Can::default()
            };
            arrange::align_section(ui, can, arrange::Targets::Nodes, to_artboard, &mut actions);
            return (message, actions);
        }
        if s.selected().is_empty() {
            artboard::list_section(ui, s, &mut actions);
            artboard::section(ui, s);
            return (None, actions);
        }
        let message = inspector(ui, s, &mut actions);
        let message = path::sections(ui, s).or(message);
        let can = edit_ops::can(s);
        arrange::align_section(
            ui,
            can,
            arrange::Targets::Objects,
            to_artboard,
            &mut actions,
        );
        arrange::arrange_section(ui, can, &mut actions);
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
fn inspector(ui: &mut egui::Ui, s: &mut Session, actions: &mut Vec<PanelAction>) -> Option<String> {
    let can_edit = edit_ops::can(s).edit;
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
                    ui.add_enabled_ui(false, |ui| {
                        ui.add_sized(FIELD_SIZE, egui::DragValue::new(&mut 0.0))
                    })
                    .inner
                    .on_disabled_hover_text("Rotate one object at a time here, or use the corners");
                }
                // Under H's label (empty), then the flips ending with its field.
                ui.label("");
                flip_buttons(ui, can_edit, actions);
                ui.end_row();
            });
    });
    let shape_message = shape::section(ui, s);
    let text_message = text::section(ui, s).or(shape_message);
    let text_message = paint::sections(ui, s).or(text_message);
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

/// Flip Horizontal and Flip Vertical as icon buttons sharing a number
/// field's width, so they start and end where the fields above do (like
/// Object ▸ Flip). Returns the buttons' rect.
fn flip_buttons(ui: &mut egui::Ui, enabled: bool, actions: &mut Vec<PanelAction>) -> egui::Rect {
    let size = egui::vec2(FIELD_SIZE[0], FIELD_SIZE[1]);
    let layout = egui::Layout::right_to_left(egui::Align::Center);
    let gap = 4.0;
    // Each button: half the field (less the gap), and the field's height,
    // so the row stays one even line with the ° field beside it.
    let button = egui::vec2((FIELD_SIZE[0] - gap) / 2.0, FIELD_SIZE[1]);
    ui.allocate_ui_with_layout(size, layout, |ui| {
        ui.spacing_mut().item_spacing.x = gap;
        // Right to left: vertical sits last, so horizontal reads first.
        for (icon, horizontal, tip) in [
            (Icon::FlipVertical, false, "Flip vertically"),
            (Icon::FlipHorizontal, true, "Flip horizontally"),
        ] {
            let why = "Select an object first";
            if icons::quiet_button(ui, icon, button, false, enabled, tip, why).clicked() {
                actions.push(PanelAction::Flip { horizontal });
            }
        }
    })
    .response
    .rect
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
/// A field's label: its letter (may be empty), then a small icon saying
/// what it is; the last part is the tooltip. Always
/// [`ICON_LABEL_WIDTH`] wide, so fields line up across sections.
#[derive(Clone, Copy)]
struct Label(&'static str, Icon, &'static str);

impl Label {
    fn show(self, ui: &mut egui::Ui) -> egui::Rect {
        ui.horizontal(|ui| {
            ui.set_min_width(ICON_LABEL_WIDTH);
            ui.spacing_mut().item_spacing.x = 4.0;
            if !self.0.is_empty() {
                ui.label(self.0);
            }
            icons::label(ui, self.1, self.2);
        })
        .response
        .rect
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
    fn every_grid_label_is_one_width_so_fields_line_up_across_sections() {
        // Transform, Artboard and Corners share the four-column grid; with
        // labels this wide their fields start and end at the same x.
        let labels = [
            Label("X", Icon::PosX, ""),
            Label("Y", Icon::PosY, ""),
            Label("W", Icon::Width, ""),
            Label("H", Icon::Height, ""),
            Label("°", Icon::Angle, ""),
            Label("", Icon::CornerTopLeft, ""),
            Label("", Icon::CornerBottomRight, ""),
            Label("", Icon::CornerAll, ""),
        ];
        let ctx = egui::Context::default();
        let mut widths = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            for label in labels {
                widths.push(label.show(ui).width());
            }
        })
        .textures_delta
        .clear();
        for w in widths {
            assert!((w - ICON_LABEL_WIDTH).abs() < 0.5, "{w}");
        }
    }

    #[test]
    fn flip_buttons_span_a_field_and_flip_each_way() {
        let ctx = egui::Context::default();
        let run = |events: Vec<egui::Event>| {
            let mut actions = Vec::new();
            let mut cell = egui::Rect::NOTHING;
            let input = egui::RawInput {
                events,
                ..egui::RawInput::default()
            };
            ctx.run_ui(input, |ui| cell = flip_buttons(ui, true, &mut actions))
                .textures_delta
                .clear();
            (cell, actions)
        };
        let (cell, _) = run(vec![]);
        // Drawn from the cell's left (0): the buttons start and end where
        // a field does.
        assert!((cell.right() - FIELD_SIZE[0]).abs() < 0.5, "{cell:?}");
        assert!(cell.left().abs() < 0.5, "{cell:?}");
        // As tall as a field, from the same top.
        assert!((cell.height() - FIELD_SIZE[1]).abs() < 0.5, "{cell:?}");
        assert!(cell.top().abs() < 0.5, "{cell:?}");
        let click = |pos| {
            [true, false].map(|pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            })
        };
        // Vertical is flush right; horizontal sits just left of it.
        let vertical = cell.right_center() - egui::vec2(6.0, 0.0);
        let horizontal = cell.right_center() - egui::vec2(40.0, 0.0);
        for (at, want) in [(horizontal, true), (vertical, false)] {
            run(vec![egui::Event::PointerMoved(at)]);
            let [down, up] = click(at);
            run(vec![down]);
            let (_, actions) = run(vec![up]);
            assert_eq!(actions, vec![PanelAction::Flip { horizontal: want }]);
        }
    }

    #[test]
    fn every_tool_is_in_the_strip_once_with_its_name_and_key() {
        let tools: Vec<_> = TOOLS.iter().flat_map(|g| g.iter()).collect();
        let mut ids: Vec<String> = tools.iter().map(|t| format!("{:?}", t.0)).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), tools.len(), "no tool twice");
        assert_eq!(ids.len(), 10, "Select, Nodes, Pen, Text, five shapes, Zoom");
        for &(tool, _, tooltip) in tools {
            assert!(
                tooltip.ends_with(')') && tooltip.contains(" ("),
                "{tool:?}: {tooltip}"
            );
        }
    }

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
