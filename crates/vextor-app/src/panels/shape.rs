//! The inspector's Corners section for one selected rect (its four corner
//! radii laid out like the corners, then all at once, icon labelled on
//! the Transform grid), and Shape for a polygon or star (sides, points,
//! inner radius, rounding) (spec 06). Each change is a `SetGeometry`; a
//! drag on a field is one undo step.

use eframe::egui;
use vextor_core::{
    Command, CornerRadii, Geometry, MAX_POLYGON_SIDES, MAX_STAR_POINTS, NodeId, NodeKind,
};
use vextor_geom::Corner;

use super::{FIELD_SIZE, GRID_SPACING, LABEL_WIDTH, Label};
use crate::icons::Icon;
use crate::session::Session;

/// Undo label of a Shape section change.
pub const LABEL: &str = "Change Shape";

/// Largest radius the fields accept, in px (the largest artboard).
const MAX_RADIUS: f64 = 10_000.0;

/// The one selected shape and its geometry, when it has options here.
fn selected(s: &Session) -> Option<(NodeId, Geometry)> {
    let [id] = s.selected()[..] else {
        return None;
    };
    match &s.doc.nodes.get(&id)?.kind {
        NodeKind::Shape(shape)
            if matches!(
                shape.geometry,
                Geometry::Rect { .. } | Geometry::Polygon { .. } | Geometry::Star { .. }
            ) =>
        {
            Some((id, shape.geometry.clone()))
        }
        _ => None,
    }
}

/// The Corners (rect) or Shape (polygon, star) section. Returns a
/// message when an edit was refused.
pub fn section(ui: &mut egui::Ui, s: &mut Session) -> Option<String> {
    let (id, geometry) = selected(s)?;
    let mut changed = None;
    if let Geometry::Rect { rect, radius } = geometry {
        super::section(ui, "Corners", |ui| {
            egui::Grid::new("corners-inspector")
                .num_columns(4)
                .spacing(GRID_SPACING)
                .show(ui, |ui| changed = corner_fields(ui, s, rect, radius));
        });
    } else {
        super::section(ui, "Shape", |ui| {
            egui::Grid::new("shape-inspector")
                .num_columns(2)
                .spacing(GRID_SPACING)
                .min_col_width(LABEL_WIDTH)
                .show(ui, |ui| changed = fields(ui, s, &geometry));
        });
    }
    let geometry = changed?;
    s.apply(Command::Batch {
        label: LABEL,
        commands: vec![Command::SetGeometry {
            items: vec![(id, geometry)],
        }],
    })
    .err()
    .map(|e| format!("Change shape failed: {e}"))
}

/// A rect's corners as they sit (top left | top right, bottom left |
/// bottom right), then All corners (blank-looking 0 while they differ);
/// returns the edited geometry, if any.
fn corner_fields(
    ui: &mut egui::Ui,
    s: &mut Session,
    rect: vextor_core::Rect,
    radius: CornerRadii,
) -> Option<Geometry> {
    let mut out = None;
    let rows = [
        [
            (Corner::TopLeft, Label("", Icon::CornerTopLeft, "Top left")),
            (
                Corner::TopRight,
                Label("", Icon::CornerTopRight, "Top right"),
            ),
        ],
        [
            (
                Corner::BottomLeft,
                Label("", Icon::CornerBottomLeft, "Bottom left"),
            ),
            (
                Corner::BottomRight,
                Label("", Icon::CornerBottomRight, "Bottom right"),
            ),
        ],
    ];
    for row in rows {
        for (corner, label) in row {
            if let Some(v) = radius_field(ui, s, label, corner.radius(radius)) {
                out = Some(Geometry::Rect {
                    rect,
                    radius: corner.with(radius, v),
                });
            }
        }
        ui.end_row();
    }
    let all = radius.tl;
    let same = [radius.tr, radius.br, radius.bl]
        .iter()
        .all(|r| (r - all).abs() < vextor_core::EPS);
    let shown = if same { all } else { 0.0 };
    let label = Label("", Icon::CornerAll, "All corners");
    if let Some(v) = radius_field(ui, s, label, shown) {
        out = Some(Geometry::Rect {
            rect,
            radius: CornerRadii {
                tl: v,
                tr: v,
                br: v,
                bl: v,
            },
        });
    }
    ui.end_row();
    out
}

/// One icon-labelled corner radius (a grid cell pair, no row end); the
/// new value when the user changed it. A drag is one undo step.
fn radius_field(ui: &mut egui::Ui, s: &mut Session, label: Label, value: f64) -> Option<f64> {
    label.show(ui);
    edited(ui, s, value, 0.0..=MAX_RADIUS, false)
}

/// The rows for a polygon or star; returns the edited geometry, if any.
fn fields(ui: &mut egui::Ui, s: &mut Session, g: &Geometry) -> Option<Geometry> {
    let mut out = None;
    match g.clone() {
        Geometry::Polygon {
            center,
            radius,
            sides,
            corner_radius,
        } => {
            let max = f64::from(MAX_POLYGON_SIDES);
            if let Some(v) = number(ui, s, "Sides", f64::from(sides), 3.0..=max, true) {
                out = Some(Geometry::Polygon {
                    center,
                    radius,
                    sides: count(v),
                    corner_radius,
                });
            }
            if let Some(v) = number(ui, s, "Rounding", corner_radius, 0.0..=MAX_RADIUS, false) {
                out = Some(Geometry::Polygon {
                    center,
                    radius,
                    sides,
                    corner_radius: v,
                });
            }
        }
        Geometry::Star {
            center,
            outer,
            inner,
            points,
            corner_radius,
        } => {
            let star = |inner, points, corner_radius| Geometry::Star {
                center,
                outer,
                inner,
                points,
                corner_radius,
            };
            let max = f64::from(MAX_STAR_POINTS);
            if let Some(v) = number(ui, s, "Points", f64::from(points), 3.0..=max, true) {
                out = Some(star(inner, count(v), corner_radius));
            }
            let share = if outer > vextor_core::EPS {
                inner / outer * 100.0
            } else {
                0.0
            };
            if let Some(v) = number(ui, s, "Inner radius %", share, 1.0..=100.0, false) {
                out = Some(star(outer * v / 100.0, points, corner_radius));
            }
            if let Some(v) = number(ui, s, "Rounding", corner_radius, 0.0..=MAX_RADIUS, false) {
                out = Some(star(inner, points, v));
            }
        }
        _ => {}
    }
    out
}

/// A whole count from a field value (always within 3..=20 by its range).
fn count(v: f64) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // range 3..=20
    let n = v.round() as u32;
    n
}

/// A labelled number field (`whole`: a count); returns the new value when
/// the user changed it. A drag is one undo step (a history gesture).
fn number(
    ui: &mut egui::Ui,
    s: &mut Session,
    label: &str,
    value: f64,
    range: std::ops::RangeInclusive<f64>,
    whole: bool,
) -> Option<f64> {
    ui.label(label);
    let out = edited(ui, s, value, range, whole);
    ui.end_row();
    out
}

/// A number field for `value` in `range` (`whole`: a count); the new
/// value when the user changed it. A drag is one undo step.
fn edited(
    ui: &mut egui::Ui,
    s: &mut Session,
    value: f64,
    range: std::ops::RangeInclusive<f64>,
    whole: bool,
) -> Option<f64> {
    let mut v = value;
    let r = ui.add_sized(
        FIELD_SIZE,
        egui::DragValue::new(&mut v)
            .speed(if whole { 0.1 } else { 0.5 })
            .range(range)
            .clamp_existing_to_range(false)
            .max_decimals(if whole { 0 } else { 2 }),
    );
    if r.drag_started() {
        s.begin_gesture();
    }
    let edited = (r.changed() && (v - value).abs() > vextor_core::EPS).then_some(v);
    if r.drag_stopped() {
        s.end_gesture();
    }
    edited
}

#[cfg(test)]
mod tests {
    use vextor_core::{Node, Point, Rect, Shape, Style};

    use super::*;
    use crate::new_doc::new_document;

    fn session(geometry: Geometry) -> (Session, NodeId) {
        let mut doc = new_document("a", 400.0, 200.0);
        let layer = doc.layers[0];
        let id = doc.alloc_node_id();
        let node = Node::new(
            id,
            NodeKind::Shape(Shape {
                geometry,
                style: Style::default(),
            }),
        );
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![node],
        })
        .expect("inserts");
        let mut s = Session::new(doc);
        s.select(vec![id]);
        (s, id)
    }

    fn show(s: &mut Session) {
        let ctx = egui::Context::default();
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                let _ = section(ui, s);
            })
            .textures_delta
            .clear();
        }
    }

    #[test]
    fn showing_the_section_changes_nothing() {
        let big = Geometry::Polygon {
            center: Point::new(50.0, 50.0),
            radius: 40.0,
            sides: 7,
            corner_radius: 50_000.0,
        };
        let mixed = Geometry::Rect {
            rect: Rect::new(0.0, 0.0, 40.0, 20.0),
            radius: CornerRadii {
                tl: 2.0,
                tr: 4.0,
                br: 6.0,
                bl: 8.0,
            },
        };
        for geometry in [big, mixed] {
            let (mut s, _) = session(geometry);
            let before = s.doc.clone();
            show(&mut s);
            assert_eq!(s.doc, before);
            assert_eq!(s.edit_menu().undo, None);
        }
    }

    #[test]
    fn only_rects_polygons_and_stars_have_a_section() {
        let (s, _) = session(Geometry::Ellipse {
            center: Point::new(0.0, 0.0),
            rx: 4.0,
            ry: 4.0,
        });
        assert!(selected(&s).is_none());
        let (s, _) = session(Geometry::Rect {
            rect: Rect::new(0.0, 0.0, 4.0, 4.0),
            radius: CornerRadii::default(),
        });
        assert!(selected(&s).is_some());
    }

    #[test]
    fn counts_round_to_whole_numbers() {
        assert_eq!(count(5.4), 5);
        assert_eq!(count(5.6), 6);
    }
}
