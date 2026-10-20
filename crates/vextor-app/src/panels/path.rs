//! The inspector's Node and Path sections (spec 07). Node: the one
//! selected node's position (document space), its type, its handles as
//! length and angle (the path's own units), and its corner radius. Path:
//! Close, Open, Join, Split and Reverse, as in Object ▸ Path. A drag on a
//! field is one undo step.

use eframe::egui;
use vextor_core::{NodeType, PathNodeRef, Side};
use vextor_geom::node_corner;

use super::{FIELD_SIZE, GRID_SPACING, LABEL_WIDTH};
use crate::path_ops::{self, PathAction};
use crate::session::Session;

/// Largest length or radius the fields accept, in px.
const MAX_LENGTH: f64 = 10_000.0;

/// One edit from the Node section.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Edit {
    X(f64),
    Y(f64),
    Type(NodeType),
    /// A handle's new length (or `None`) and angle in degrees.
    Handle(Side, f64, f64),
    Radius(f64),
}

/// The Node section (one node selected) and the Path section (a path
/// selected or edited). Returns a message when an edit was refused.
pub fn sections(ui: &mut egui::Ui, s: &mut Session) -> Option<String> {
    let nodes = path_ops::selected_nodes(s);
    let message = match nodes[..] {
        [r] => node_section(ui, s, r),
        [] => None,
        _ => {
            super::section(ui, "Node", |ui| {
                ui.weak(format!("{} nodes selected", nodes.len()));
            });
            None
        }
    };
    let shows = path_ops::edited(s).is_some()
        || path_ops::join_target(s).is_some()
        || !path_ops::reverse_targets(s).is_empty();
    if !shows {
        return message;
    }
    let mut ran = None;
    super::section(ui, "Path", |ui| {
        ui.horizontal_wrapped(|ui| {
            for a in PathAction::STRUCTURE {
                let button = ui
                    .add_enabled(a.can(s), egui::Button::new(a.title()))
                    .on_disabled_hover_text(a.why());
                if button.clicked() {
                    ran = a.run(s).err();
                }
            }
        });
    });
    message.or(ran)
}

fn node_section(ui: &mut egui::Ui, s: &mut Session, r: PathNodeRef) -> Option<String> {
    let n = *s.path_node(r)?;
    let at = path_ops::anchor_in_doc(s, r)?;
    let roundable = s
        .path_of(r.shape)
        .and_then(|p| node_corner(&p.subpaths[r.subpath], r.index))
        .is_some();
    let mut edit = None;
    super::section(ui, "Node", |ui| {
        egui::Grid::new("node-inspector")
            .num_columns(2)
            .spacing(GRID_SPACING)
            .min_col_width(LABEL_WIDTH)
            .show(ui, |ui| {
                if let Some(v) = number(ui, s, "X", at.x, -MAX_LENGTH..=MAX_LENGTH) {
                    edit = Some(Edit::X(v));
                }
                if let Some(v) = number(ui, s, "Y", at.y, -MAX_LENGTH..=MAX_LENGTH) {
                    edit = Some(Edit::Y(v));
                }
                ui.label("Type");
                ui.horizontal(|ui| {
                    for (kind, name) in [
                        (NodeType::Corner, "Corner"),
                        (NodeType::Smooth, "Smooth"),
                        (NodeType::Symmetric, "Symmetric"),
                    ] {
                        if ui.selectable_label(n.kind == kind, name).clicked() && n.kind != kind {
                            edit = Some(Edit::Type(kind));
                        }
                    }
                });
                ui.end_row();
                edit = handle_rows(ui, s, n).or(edit);
                if roundable
                    && let Some(v) =
                        number(ui, s, "Corner radius", n.corner_radius, 0.0..=MAX_LENGTH)
                {
                    edit = Some(Edit::Radius(v));
                }
            });
    });
    match edit? {
        Edit::X(x) => path_ops::move_node_to(s, r, kurbo::Point::new(x, at.y)),
        Edit::Y(y) => path_ops::move_node_to(s, r, kurbo::Point::new(at.x, y)),
        Edit::Type(kind) => path_ops::set_node_type(s, r, kind),
        Edit::Handle(side, len, deg) => {
            let (sin, cos) = deg.to_radians().sin_cos();
            let h = vextor_geom::handle_or_none(kurbo::Vec2::new(cos, sin) * len);
            path_ops::set_handle(s, r, side, h, false)
        }
        Edit::Radius(v) => path_ops::set_corner_radius(s, &[r], v),
    }
    .err()
}

/// The In and Out handle rows: length and angle of each, or "None".
/// Returns the edit made, if any.
fn handle_rows(ui: &mut egui::Ui, s: &mut Session, n: vextor_core::PathNode) -> Option<Edit> {
    let mut edit = None;
    for (side, name, h) in [
        (Side::In, "In handle", n.handle_in),
        (Side::Out, "Out handle", n.handle_out),
    ] {
        let Some(h) = h else {
            ui.label(name);
            ui.weak("None (straight)");
            ui.end_row();
            continue;
        };
        let (len, deg) = (h.length(), h.y.atan2(h.x).to_degrees());
        if let Some(v) = number(ui, s, name, len, 0.0..=MAX_LENGTH) {
            edit = Some(Edit::Handle(side, v, deg));
        }
        if let Some(v) = number(ui, s, "  Angle °", deg, -360.0..=360.0) {
            edit = Some(Edit::Handle(side, len, v));
        }
    }
    edit
}

/// A labelled number row. A drag is wrapped in a history gesture.
/// Returns the new value when it changed.
fn number(
    ui: &mut egui::Ui,
    s: &mut Session,
    label: &str,
    value: f64,
    range: std::ops::RangeInclusive<f64>,
) -> Option<f64> {
    ui.label(label);
    let mut v = value;
    let r = ui.add_sized(
        FIELD_SIZE,
        egui::DragValue::new(&mut v)
            .speed(1.0)
            .max_decimals(2)
            .range(range),
    );
    ui.end_row();
    if r.drag_started() {
        s.begin_gesture();
    }
    if r.drag_stopped() {
        s.end_gesture();
    }
    (r.changed() && (v - value).abs() > vextor_core::EPS).then_some(v)
}
