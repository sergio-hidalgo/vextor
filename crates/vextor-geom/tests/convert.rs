//! Geometry → `BezPath` conversions checked against analytic areas and bounds.

use std::f64::consts::PI;

use kurbo::{BezPath, PathEl, Shape as _};
use vextor_core::{
    Command, CornerRadii, Document, FillRule, Geometry, Node, NodeKind, NodeType, Path, PathNode,
    Point, Rect, Shape, Style, SubPath, Vec2,
};
use vextor_geom::{bezpath_to_path, geometry_to_path, path_to_bezpath, to_bezpath};

fn area(g: &Geometry) -> f64 {
    to_bezpath(g).area().abs()
}

fn assert_close(got: f64, want: f64, tol: f64) {
    assert!((got - want).abs() <= tol, "got {got}, want {want} (±{tol})");
}

fn rect(w: f64, h: f64, r: f64) -> Geometry {
    Geometry::Rect {
        rect: Rect::new(10.0, 20.0, w, h),
        radius: CornerRadii {
            tl: r,
            tr: r,
            br: r,
            bl: r,
        },
    }
}

fn square_path(radius: f64) -> Path {
    let node = |x, y| PathNode {
        corner_radius: radius,
        ..PathNode::corner(x, y)
    };
    Path {
        subpaths: vec![SubPath {
            nodes: vec![
                node(0.0, 0.0),
                node(10.0, 0.0),
                node(10.0, 10.0),
                node(0.0, 10.0),
            ],
            closed: true,
        }],
        fill_rule: FillRule::NonZero,
    }
}

#[test]
fn plain_rect() {
    let g = rect(30.0, 40.0, 0.0);
    assert_close(area(&g), 1200.0, 1e-9);
    let bb = to_bezpath(&g).bounding_box();
    assert_eq!((bb.x0, bb.y0, bb.x1, bb.y1), (10.0, 20.0, 40.0, 60.0));
    let curves = to_bezpath(&g)
        .elements()
        .iter()
        .filter(|e| matches!(e, PathEl::CurveTo(..)))
        .count();
    assert_eq!(curves, 0, "sharp corners need no curves");
}

#[test]
fn rounded_rect_area() {
    let r = 3.0;
    assert_close(
        area(&rect(30.0, 40.0, r)),
        1200.0 - (4.0 - PI) * r * r,
        0.01,
    );
}

#[test]
fn rounded_rect_radii_are_scaled_to_fit() {
    // all radii 10 on a 10×10 square → a circle of radius 5
    assert_close(area(&rect(10.0, 10.0, 10.0)), PI * 25.0, 0.05);
    let bb = to_bezpath(&rect(10.0, 10.0, 10.0)).bounding_box();
    assert_close(bb.width(), 10.0, 1e-9);
}

#[test]
fn per_corner_radii() {
    let g = Geometry::Rect {
        rect: Rect::new(0.0, 0.0, 20.0, 20.0),
        radius: CornerRadii {
            tl: 4.0,
            tr: 0.0,
            br: 2.0,
            bl: 0.0,
        },
    };
    assert_close(area(&g), 400.0 - (4.0 - PI) / 4.0 * (16.0 + 4.0), 0.01);
}

#[test]
fn ellipse_area_and_bounds() {
    let g = Geometry::Ellipse {
        center: Point::new(5.0, 5.0),
        rx: 4.0,
        ry: 2.0,
    };
    assert_close(area(&g), PI * 8.0, 0.01);
    let bb = to_bezpath(&g).bounding_box();
    assert_close(bb.x0, 1.0, 1e-9);
    assert_close(bb.y1, 7.0, 1e-9);
}

#[test]
fn regular_polygon_area() {
    let g = Geometry::Polygon {
        center: Point::new(0.0, 0.0),
        radius: 10.0,
        sides: 6,
        corner_radius: 0.0,
    };
    assert_close(area(&g), 3.0 * 3f64.sqrt() / 2.0 * 100.0, 1e-9);
    let bb = to_bezpath(&g).bounding_box();
    assert_close(bb.y0, -10.0, 1e-9); // first vertex points up
}

#[test]
fn star_area() {
    let g = Geometry::Star {
        center: Point::new(0.0, 0.0),
        outer: 10.0,
        inner: 4.0,
        points: 5,
        corner_radius: 0.0,
    };
    assert_close(area(&g), 5.0 * 10.0 * 4.0 * (PI / 5.0).sin(), 1e-9);
}

#[test]
fn polygon_corner_radius_rounds_inside_the_outline() {
    let sharp = Geometry::Polygon {
        center: Point::new(0.0, 0.0),
        radius: 10.0,
        sides: 4,
        corner_radius: 0.0,
    };
    let round = Geometry::Polygon {
        center: Point::new(0.0, 0.0),
        radius: 10.0,
        sides: 4,
        corner_radius: 2.0,
    };
    // square of side 10√2: rounding removes (4 − π)·r² in total
    assert_close(area(&round), area(&sharp) - (4.0 - PI) * 4.0, 0.01);
    let bb = to_bezpath(&round).bounding_box();
    assert!(bb.y0 > -10.0);
}

#[test]
fn huge_corner_radius_is_clamped_not_broken() {
    let g = Geometry::Star {
        center: Point::new(0.0, 0.0),
        outer: 10.0,
        inner: 4.0,
        points: 5,
        corner_radius: 1e9,
    };
    let a = area(&g);
    assert!(a.is_finite() && a > 0.0);
    assert!(to_bezpath(&g).bounding_box().width() <= 20.0);
}

#[test]
fn line_is_open_with_two_points() {
    let g = Geometry::Line {
        from: Point::new(0.0, 0.0),
        to: Point::new(3.0, 4.0),
    };
    assert_eq!(to_bezpath(&g).elements().len(), 2);
    assert_close(area(&g), 0.0, 1e-12);
}

#[test]
fn path_corner_radius_on_straight_corners() {
    assert_close(path_to_bezpath(&square_path(0.0)).area().abs(), 100.0, 1e-9);
    assert_close(
        path_to_bezpath(&square_path(2.0)).area().abs(),
        100.0 - (4.0 - PI) * 4.0,
        0.01,
    );
}

#[test]
fn path_corner_radius_is_ignored_next_to_curves() {
    let mut p = square_path(2.0);
    p.subpaths[0].nodes[1].handle_in = Some(Vec2::new(-3.0, -2.0));
    let curves = path_to_bezpath(&p)
        .elements()
        .iter()
        .filter(|e| matches!(e, PathEl::CurveTo(..)))
        .count();
    // segment 0→1 is curved, so nodes 0 and 1 stay sharp; nodes 2 and 3
    // are rounded: 1 curved segment + 2 fillets.
    assert_eq!(curves, 3);
}

#[test]
fn open_path_ends_are_never_rounded() {
    let mut p = square_path(2.0);
    p.subpaths[0].closed = false;
    let curves = path_to_bezpath(&p)
        .elements()
        .iter()
        .filter(|e| matches!(e, PathEl::CurveTo(..)))
        .count();
    assert_eq!(curves, 2); // only the two interior corners
}

#[test]
fn path_round_trips_through_bezpath() {
    let curvy = |x: f64, y: f64| PathNode {
        handle_in: Some(Vec2::new(-2.0, 1.0)),
        handle_out: Some(Vec2::new(3.0, 0.5)),
        ..PathNode::corner(x, y)
    };
    let p = Path {
        subpaths: vec![
            SubPath {
                nodes: vec![
                    PathNode::corner(0.0, 0.0),
                    curvy(10.0, 0.0),
                    PathNode::corner(20.0, 5.0),
                ],
                closed: false,
            },
            SubPath {
                nodes: vec![
                    curvy(0.0, 10.0),
                    PathNode::corner(10.0, 20.0),
                    curvy(0.0, 20.0),
                ],
                closed: true,
            },
        ],
        fill_rule: FillRule::EvenOdd,
    };
    let mut want = p.clone();
    // open ends lose the handle that has no segment
    want.subpaths[0].nodes[1].kind = NodeType::Corner;
    assert_eq!(
        bezpath_to_path(&path_to_bezpath(&p), FillRule::EvenOdd),
        want
    );
}

#[test]
fn quadratics_are_raised_to_cubics() {
    let mut bez = BezPath::new();
    bez.move_to((0.0, 0.0));
    bez.quad_to((3.0, 3.0), (6.0, 0.0));
    let p = bezpath_to_path(&bez, FillRule::NonZero);
    let nodes = &p.subpaths[0].nodes;
    assert_eq!(nodes[0].handle_out, Some(Vec2::new(2.0, 2.0)));
    assert_eq!(nodes[1].handle_in, Some(Vec2::new(-2.0, 2.0)));
}

/// Every geometry kind converts to a path that core accepts.
#[test]
fn convert_to_path_is_valid_for_every_kind() {
    let geometries = [
        rect(10.0, 10.0, 2.0),
        rect(10.0, 10.0, 0.0),
        Geometry::Ellipse {
            center: Point::new(0.0, 0.0),
            rx: 3.0,
            ry: 2.0,
        },
        Geometry::Polygon {
            center: Point::new(0.0, 0.0),
            radius: 5.0,
            sides: 3,
            corner_radius: 1.0,
        },
        Geometry::Star {
            center: Point::new(0.0, 0.0),
            outer: 5.0,
            inner: 2.0,
            points: 5,
            corner_radius: 0.5,
        },
        Geometry::Line {
            from: Point::new(0.0, 0.0),
            to: Point::new(1.0, 1.0),
        },
        Geometry::Path(square_path(1.0)),
    ];
    let mut doc = Document::new("convert");
    let layer = doc.layers[0];
    for g in geometries {
        let id = doc.alloc_node_id();
        let node = Node::new(
            id,
            NodeKind::Shape(Shape {
                geometry: g.clone(),
                style: Style::default(),
            }),
        );
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![node],
        })
        .expect("inserts");
        let path = geometry_to_path(&g);
        let before = area(&g);
        doc.apply(Command::ConvertToPath {
            items: vec![(id, path.clone())],
        })
        .expect("valid path");
        assert_close(path_to_bezpath(&path).area().abs(), before, 1e-9);
    }
}
