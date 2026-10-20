//! The inspector's Shape section (spec 06), for one selected rect, polygon
//! or star: corner radii, sides, points, inner radius and corner rounding.
//! Each change is a `SetGeometry`; a drag on a field is one undo step.

use eframe::egui;
use vextor_core::{
    Command, CornerRadii, Geometry, MAX_POLYGON_SIDES, MAX_STAR_POINTS, NodeId, NodeKind,
};
use vextor_geom::Corner;

use super::{FIELD_SIZE, GRID_SPACING, LABEL_WIDTH};
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

/// The Shape section. Returns a message when an edit was refused.
pub fn section(ui: &mut egui::Ui, s: &mut Session) -> Option<String> {
    let (id, geometry) = selected(s)?;
    let mut changed = None;
    super::section(ui, "Shape", |ui| {
        egui::Grid::new("shape-inspector")
            .num_columns(2)
            .spacing(GRID_SPACING)
            .min_col_width(LABEL_WIDTH)
            .show(ui, |ui| changed = fields(ui, s, &geometry));
    });
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

/// The rows for `g`; returns the edited geometry, if any.
fn fields(ui: &mut egui::Ui, s: &mut Session, g: &Geometry) -> Option<Geometry> {
    let mut out = None;
    match g.clone() {
        Geometry::Rect { rect, radius } => {
            let names = [
                (Corner::TopLeft, "Top left"),
                (Corner::TopRight, "Top right"),
                (Corner::BottomRight, "Bottom right"),
                (Corner::BottomLeft, "Bottom left"),
            ];
            for (corner, name) in names {
                if let Some(v) = number(ui, s, name, corner.radius(radius), 0.0..=MAX_RADIUS, false)
                {
                    out = Some(Geometry::Rect {
                        rect,
                        radius: corner.with(radius, v),
                    });
                }
            }
            let all = radius.tl;
            let same = [radius.tr, radius.br, radius.bl]
                .iter()
                .all(|r| (r - all).abs() < vextor_core::EPS);
            let shown = if same { all } else { 0.0 };
            if let Some(v) = number(ui, s, "All corners", shown, 0.0..=MAX_RADIUS, false) {
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
        }
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
    let mut v = value;
    let r = ui.add_sized(
        FIELD_SIZE,
        egui::DragValue::new(&mut v)
            .speed(if whole { 0.1 } else { 0.5 })
            .range(range)
            .clamp_existing_to_range(false)
            .max_decimals(if whole { 0 } else { 2 }),
    );
    ui.end_row();
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
        let (mut s, _) = session(big);
        let before = s.doc.clone();
        show(&mut s);
        assert_eq!(s.doc, before);
        assert_eq!(s.edit_menu().undo, None);
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
