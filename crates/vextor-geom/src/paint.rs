//! Where a paint lands on an object (spec 09): the bounding box that
//! gradients in bounding-box units stretch over, and the map from a
//! gradient's own coordinates to the object's.

use kurbo::Shape as _;
use vextor_core::{EPS, GradientUnits, Node, NodeId, NodeKind, Transform};

use crate::convert::to_bezpath;
use crate::text::{Fonts, text_box};

/// What part of an object a paint is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintPart {
    /// A shape's outline, or a text's glyphs.
    Body,
    /// A text's frame (background and border).
    Frame,
}

/// The box SVG's `objectBoundingBox` refers to for a paint on `part` of
/// node `id`: the geometry's bounds in the node's own coordinates, without
/// the stroke. `None` for nodes without paint (groups, images) and for a
/// frame a text does not have.
#[must_use]
pub fn paint_box(fonts: &Fonts, id: NodeId, node: &Node, part: PaintPart) -> Option<kurbo::Rect> {
    match (&node.kind, part) {
        (NodeKind::Shape(s), PaintPart::Body) => Some(to_bezpath(&s.geometry).bounding_box()),
        (NodeKind::Text(t), PaintPart::Body) => Some(fonts.layout(id, t).glyphs.bounding_box()),
        (NodeKind::Text(t), PaintPart::Frame) if t.frame.is_some() => {
            Some(text_box(t, &fonts.layout(id, t)))
        }
        _ => None,
    }
}

/// The transform from a gradient's coordinates to the painted object's:
/// identity in user space; `bbox` stretched over 0..1 in bounding-box
/// units, `None` when `bbox` is flat (SVG then paints nothing).
#[must_use]
pub fn gradient_space(units: GradientUnits, bbox: kurbo::Rect) -> Option<Transform> {
    match units {
        GradientUnits::UserSpace => Some(Transform::IDENTITY),
        GradientUnits::ObjectBoundingBox => (bbox.width() > EPS && bbox.height() > EPS)
            .then(|| Transform([bbox.width(), 0.0, 0.0, bbox.height(), bbox.x0, bbox.y0])),
    }
}

#[cfg(test)]
mod tests {
    use vextor_core::{Geometry, Point, Shape, Style};

    use super::*;

    #[test]
    fn bounding_box_units_stretch_over_the_box() {
        let bbox = kurbo::Rect::new(10.0, 20.0, 50.0, 30.0);
        let t = gradient_space(GradientUnits::ObjectBoundingBox, bbox).expect("box has area");
        assert_eq!(t.apply(Point::new(0.0, 0.0)), Point::new(10.0, 20.0));
        assert_eq!(t.apply(Point::new(1.0, 1.0)), Point::new(50.0, 30.0));
        let flat = kurbo::Rect::new(0.0, 5.0, 10.0, 5.0);
        assert_eq!(gradient_space(GradientUnits::ObjectBoundingBox, flat), None);
        assert_eq!(
            gradient_space(GradientUnits::UserSpace, flat),
            Some(Transform::IDENTITY)
        );
    }

    #[test]
    fn a_shape_paints_over_its_geometry_without_the_stroke() {
        let node = Node::new(
            NodeId(1),
            NodeKind::Shape(Shape {
                geometry: Geometry::Ellipse {
                    center: Point::new(10.0, 10.0),
                    rx: 5.0,
                    ry: 3.0,
                },
                style: Style::default(),
            }),
        );
        let fonts = Fonts::default();
        let b = paint_box(&fonts, node.id, &node, PaintPart::Body).expect("shape");
        assert!(
            (b.x0 - 5.0).abs() < 1e-9 && (b.y1 - 13.0).abs() < 1e-9,
            "{b:?}"
        );
        assert_eq!(paint_box(&fonts, node.id, &node, PaintPart::Frame), None);
    }
}
