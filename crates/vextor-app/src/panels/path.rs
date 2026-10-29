//! The inspector's Node and Path sections (spec 07). Node: the one
//! selected node's position (document space) as X and Y side by side, its
//! type as three icon buttons, its In and Out handles side by side as size
//! and angle (the path's own units), and its corner radius. Path: Close,
//! Open, Join, Split and Reverse as icon buttons, as in Object ▸ Path. A
//! drag on a field is one undo step.

use eframe::egui;
use vextor_core::{FillRule, NodeId, NodeType, PathNodeRef, Side};
use vextor_geom::node_corner;

use super::{FIELD_SIZE, Label};
use crate::app::keys;
use crate::icons::{self, Icon};
use crate::path_ops::{self, PathAction};
use crate::session::Session;
use crate::shape_ops;

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

/// Width of the label column in each half of the Node section.
const HALF_LABEL: f32 = 48.0;
/// Gap between label and field, and between rows, in the halves.
const HALF_SPACING: [f32; 2] = [8.0, 6.0];

/// The node type buttons: type, icon, tooltip.
const TYPES: [(NodeType, Icon, &str); 3] = [
    (NodeType::Corner, Icon::NodeCorner, "Corner"),
    (NodeType::Smooth, Icon::NodeSmooth, "Smooth"),
    (NodeType::Symmetric, Icon::NodeSymmetric, "Symmetric"),
];

/// The icon of each path command button.
fn action_icon(a: PathAction) -> Icon {
    match a {
        PathAction::Close => Icon::PathClose,
        PathAction::Open => Icon::PathOpen,
        PathAction::Join => Icon::PathJoin,
        PathAction::Split => Icon::PathSplit,
        _ => Icon::PathReverse,
    }
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
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for a in PathAction::STRUCTURE {
                let icon = action_icon(a);
                let tip = match keys::path_action(ui.ctx().os(), a) {
                    None => a.title().to_owned(),
                    Some(k) => format!("{} ({})", a.title(), keys::label(ui.ctx(), k)),
                };
                if icons::button(ui, icon, false, a.can(s), &tip, a.why()).clicked() {
                    ran = a.run(s).err();
                }
            }
        });
        if let Some(id) = path_ops::edited(s)
            && let Some(rule) = s.path_of(id).map(|p| p.fill_rule)
        {
            ran = fill_rule_row(ui, s, id, rule).or(ran.take());
        }
    });
    message.or(ran)
}

/// The fill rule of path `id` (spec 11): Non-zero or Even-odd, where
/// overlapping parts of a compound path make holes. Returns a message when
/// the change was refused.
fn fill_rule_row(ui: &mut egui::Ui, s: &mut Session, id: NodeId, rule: FillRule) -> Option<String> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.label("Fill rule");
        let choices = [
            (
                FillRule::NonZero,
                "Non-zero",
                "Overlapping parts stay filled",
            ),
            (
                FillRule::EvenOdd,
                "Even-odd",
                "Overlapping parts make holes",
            ),
        ];
        for (r, text, tooltip) in choices {
            if ui
                .selectable_label(rule == r, text)
                .on_hover_text(tooltip)
                .clicked()
            {
                picked = Some(r);
            }
        }
    });
    picked.and_then(|r| shape_ops::set_fill_rule(s, id, r).err())
}

/// X and Y side by side; the type as three icon buttons; the In and Out
/// handles side by side (size and angle each); the corner radius.
fn node_section(ui: &mut egui::Ui, s: &mut Session, r: PathNodeRef) -> Option<String> {
    let n = *s.path_node(r)?;
    let at = path_ops::anchor_in_doc(s, r)?;
    let roundable = s
        .path_of(r.shape)
        .and_then(|p| node_corner(&p.subpaths[r.subpath], r.index))
        .is_some();
    let mut edit = None;
    super::section(ui, "Node", |ui| {
        let xy = [
            (
                Label("X", Icon::PosX, "Node position, left to right (X)"),
                at.x,
            ),
            (
                Label("Y", Icon::PosY, "Node position, top to bottom (Y)"),
                at.y,
            ),
        ];
        ui.columns(2, |cols| {
            for (i, (col, (label, value))) in cols.iter_mut().zip(xy).enumerate() {
                half_grid(col, ("node-xy", i), |ui| {
                    label.show(ui);
                    if let Some(v) = field(ui, s, value, -MAX_LENGTH..=MAX_LENGTH) {
                        edit = Some(if i == 0 { Edit::X(v) } else { Edit::Y(v) });
                    }
                    ui.end_row();
                });
            }
        });
        ui.add_space(4.0);
        half_grid(ui, ("node-type", 0), |ui| {
            ui.label("Type");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (kind, icon, name) in TYPES {
                    if icons::button(ui, icon, n.kind == kind, true, name, "").clicked()
                        && n.kind != kind
                    {
                        edit = Some(Edit::Type(kind));
                    }
                }
            });
            ui.end_row();
        });
        ui.add_space(4.0);
        edit = handle_columns(ui, s, n).or(edit);
        if roundable {
            half_grid(ui, ("node-radius", 0), |ui| {
                ui.label("Corner radius");
                if let Some(v) = field(ui, s, n.corner_radius, 0.0..=MAX_LENGTH) {
                    edit = Some(Edit::Radius(v));
                }
                ui.end_row();
            });
        }
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

/// The In handle (left) and Out handle (right): a titled column each with
/// Size and Angle, or "None" for a straight side. Returns the edit made.
fn handle_columns(ui: &mut egui::Ui, s: &mut Session, n: vextor_core::PathNode) -> Option<Edit> {
    let mut edit = None;
    let sides = [
        (Side::In, "In handle", Icon::HandleIn, n.handle_in),
        (Side::Out, "Out handle", Icon::HandleOut, n.handle_out),
    ];
    ui.columns(2, |cols| {
        for (col, (side, title, icon, h)) in cols.iter_mut().zip(sides) {
            col.horizontal(|ui| {
                ui.label(title);
                icons::label(ui, icon, title);
            });
            let Some(h) = h else {
                col.weak("None (straight)");
                continue;
            };
            let (len, deg) = (h.length(), h.y.atan2(h.x).to_degrees());
            half_grid(col, ("node-handle", side as usize), |ui| {
                ui.label("Size");
                if let Some(v) = field(ui, s, len, 0.0..=MAX_LENGTH) {
                    edit = Some(Edit::Handle(side, v, deg));
                }
                ui.end_row();
                ui.label("Angle °");
                if let Some(v) = field(ui, s, deg, -360.0..=360.0) {
                    edit = Some(Edit::Handle(side, len, v));
                }
                ui.end_row();
            });
        }
    });
    edit
}

/// A label + field grid for one half of the Node section.
fn half_grid(ui: &mut egui::Ui, id: (&str, usize), add: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing(HALF_SPACING)
        .min_col_width(HALF_LABEL)
        .show(ui, add);
}

/// A number field. A drag is wrapped in a history gesture. Returns the
/// new value when it changed.
fn field(
    ui: &mut egui::Ui,
    s: &mut Session,
    value: f64,
    range: std::ops::RangeInclusive<f64>,
) -> Option<f64> {
    let mut v = value;
    let r = ui.add_sized(
        FIELD_SIZE,
        egui::DragValue::new(&mut v)
            .speed(1.0)
            .max_decimals(2)
            .range(range),
    );
    if r.drag_started() {
        s.begin_gesture();
    }
    if r.drag_stopped() {
        s.end_gesture();
    }
    (r.changed() && (v - value).abs() > vextor_core::EPS).then_some(v)
}

#[cfg(test)]
mod tests {
    use vextor_core::{
        Command, Geometry, Node, NodeKind, Path, PathNode, Selection, Shape, Style, SubPath,
    };

    use super::*;
    use crate::new_doc::new_document;

    #[test]
    fn showing_the_node_and_path_sections_edits_nothing() {
        let mut doc = new_document("a", 200.0, 200.0);
        let layer = doc.layers[0];
        let id = doc.alloc_node_id();
        let mid = PathNode {
            handle_in: Some(vextor_core::Vec2::new(-10.0, 0.0)),
            handle_out: None,
            ..PathNode::corner(50.0, 0.0)
        };
        let nodes = vec![
            PathNode::corner(0.0, 0.0),
            mid,
            PathNode::corner(50.0, 50.0),
        ];
        let shape = Shape {
            geometry: Geometry::Path(Path {
                subpaths: vec![SubPath {
                    nodes,
                    closed: false,
                }],
                fill_rule: vextor_core::FillRule::NonZero,
            }),
            style: Style::default(),
        };
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![Node::new(id, NodeKind::Shape(shape))],
        })
        .expect("inserts");
        let mut s = Session::new(doc);
        s.editor.selection = Selection::PathNodes(vec![PathNodeRef {
            shape: id,
            subpath: 0,
            index: 1,
        }]);
        let before = s.doc.clone();
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let mut message = None;
            ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| message = sections(ui, &mut s));
            })
            .textures_delta
            .clear();
            assert_eq!(message, None);
        }
        assert_eq!(s.doc, before, "one handle, one straight side, no edit");
        assert_eq!(s.edit_menu().undo, None);
    }
}
