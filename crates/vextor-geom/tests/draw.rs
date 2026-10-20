//! Shape tool geometry (spec 06): drags, the size dialog and rect corner
//! widgets.

mod common;

use kurbo::Point;
use proptest::prelude::*;
use vextor_core::{CornerRadii, Geometry, Node, NodeKind, Rect, Shape, Style};
use vextor_geom::{
    Corner, DrawMods, ShapeKind, ShapeSize, corner_radius_at, corner_widget, drawn, sized,
};

const PLAIN: DrawMods = DrawMods {
    constrain: false,
    from_centre: false,
};
const SHIFT: DrawMods = DrawMods {
    constrain: true,
    from_centre: false,
};
const ALT: DrawMods = DrawMods {
    constrain: false,
    from_centre: true,
};

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn rect_of(g: Option<Geometry>) -> Rect {
    match g {
        Some(Geometry::Rect { rect, .. }) => rect,
        other => panic!("rect, got {other:?}"),
    }
}

fn assert_close(got: f64, want: f64) {
    assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
}

#[test]
fn rect_spans_the_drag_in_any_direction() {
    let r = rect_of(drawn(ShapeKind::Rect, p(50.0, 40.0), p(10.0, 70.0), PLAIN));
    assert_eq!(r, Rect::new(10.0, 40.0, 40.0, 30.0));
}

#[test]
fn shift_makes_a_square_along_the_longer_side() {
    let r = rect_of(drawn(ShapeKind::Rect, p(50.0, 40.0), p(10.0, 50.0), SHIFT));
    assert_eq!(
        r,
        Rect::new(10.0, 40.0, 40.0, 40.0),
        "grows down, as dragged"
    );
}

#[test]
fn alt_draws_from_the_centre() {
    let r = rect_of(drawn(ShapeKind::Rect, p(50.0, 40.0), p(60.0, 45.0), ALT));
    assert_eq!(r, Rect::new(40.0, 35.0, 20.0, 10.0));
}

#[test]
fn shift_alt_ellipse_is_a_circle_about_the_press() {
    let both = DrawMods {
        constrain: true,
        from_centre: true,
    };
    match drawn(ShapeKind::Ellipse, p(50.0, 50.0), p(60.0, 54.0), both) {
        Some(Geometry::Ellipse { center, rx, ry }) => {
            assert_eq!((center.x, center.y), (50.0, 50.0));
            assert_eq!((rx, ry), (10.0, 10.0));
        }
        other => panic!("ellipse, got {other:?}"),
    }
}

#[test]
fn ellipse_fills_the_dragged_box() {
    match drawn(ShapeKind::Ellipse, p(0.0, 0.0), p(40.0, 20.0), PLAIN) {
        Some(Geometry::Ellipse { center, rx, ry }) => {
            assert_eq!((center.x, center.y, rx, ry), (20.0, 10.0, 20.0, 10.0));
        }
        other => panic!("ellipse, got {other:?}"),
    }
}

#[test]
fn a_click_or_flat_drag_draws_nothing() {
    assert_eq!(
        drawn(ShapeKind::Rect, p(5.0, 5.0), p(5.0, 5.0), PLAIN),
        None
    );
    assert_eq!(
        drawn(ShapeKind::Rect, p(5.0, 5.0), p(25.0, 5.0), PLAIN),
        None
    );
    assert_eq!(
        drawn(ShapeKind::Line, p(5.0, 5.0), p(5.0, 5.0), PLAIN),
        None
    );
    let poly = ShapeKind::Polygon { sides: 6 };
    assert_eq!(drawn(poly, p(5.0, 5.0), p(5.0, 5.0), ALT), None);
}

#[test]
fn polygon_fills_a_square_or_reaches_the_pointer_from_the_centre() {
    let kind = ShapeKind::Polygon { sides: 6 };
    match drawn(kind, p(0.0, 0.0), p(40.0, 20.0), PLAIN) {
        Some(Geometry::Polygon {
            center,
            radius,
            sides,
            corner_radius,
        }) => {
            assert_eq!((center.x, center.y, radius), (20.0, 20.0, 20.0));
            assert_eq!((sides, corner_radius), (6, 0.0));
        }
        other => panic!("polygon, got {other:?}"),
    }
    match drawn(kind, p(0.0, 0.0), p(30.0, 40.0), ALT) {
        Some(Geometry::Polygon { center, radius, .. }) => {
            assert_eq!((center.x, center.y, radius), (0.0, 0.0, 50.0));
        }
        other => panic!("polygon, got {other:?}"),
    }
}

#[test]
fn star_inner_radius_is_a_share_of_the_outer() {
    let kind = ShapeKind::Star {
        points: 5,
        inner: 0.4,
    };
    match drawn(kind, p(0.0, 0.0), p(0.0, 50.0), ALT) {
        Some(Geometry::Star {
            outer,
            inner,
            points,
            ..
        }) => {
            assert_close(outer, 50.0);
            assert_close(inner, 20.0);
            assert_eq!(points, 5);
        }
        other => panic!("star, got {other:?}"),
    }
}

#[test]
fn sides_and_points_are_kept_in_range() {
    let many = drawn(
        ShapeKind::Polygon { sides: 99 },
        p(0.0, 0.0),
        p(9.0, 9.0),
        PLAIN,
    );
    assert!(matches!(many, Some(Geometry::Polygon { sides: 20, .. })));
    let few = ShapeKind::Star {
        points: 1,
        inner: 0.5,
    };
    assert!(matches!(
        drawn(few, p(0.0, 0.0), p(9.0, 9.0), PLAIN),
        Some(Geometry::Star { points: 3, .. })
    ));
}

#[test]
fn shift_snaps_a_line_to_45_degrees_keeping_its_length() {
    match drawn(ShapeKind::Line, p(0.0, 0.0), p(10.0, 1.0), SHIFT) {
        Some(Geometry::Line { from, to }) => {
            assert_eq!((from.x, from.y), (0.0, 0.0));
            assert_close(to.x, 101.0_f64.sqrt());
            assert_close(to.y, 0.0);
        }
        other => panic!("line, got {other:?}"),
    }
    match drawn(ShapeKind::Line, p(0.0, 0.0), p(10.0, 9.0), SHIFT) {
        Some(Geometry::Line { to, .. }) => assert_close(to.x, to.y),
        other => panic!("line, got {other:?}"),
    }
}

#[test]
fn alt_line_is_centred_on_the_press() {
    match drawn(ShapeKind::Line, p(10.0, 10.0), p(15.0, 10.0), ALT) {
        Some(Geometry::Line { from, to }) => {
            assert_eq!((from.x, to.x), (5.0, 15.0));
        }
        other => panic!("line, got {other:?}"),
    }
}

#[test]
fn the_size_dialog_places_boxes_by_their_top_left_or_centre() {
    let size = ShapeSize::Rect {
        width: 120.0,
        height: 40.0,
    };
    assert_eq!(
        rect_of(sized(size, p(10.0, 20.0), false)),
        Rect::new(10.0, 20.0, 120.0, 40.0)
    );
    assert_eq!(
        rect_of(sized(size, p(10.0, 20.0), true)),
        Rect::new(-50.0, 0.0, 120.0, 40.0)
    );
    let zero = ShapeSize::Ellipse {
        width: 0.0,
        height: 40.0,
    };
    assert_eq!(sized(zero, p(0.0, 0.0), false), None);
}

#[test]
fn the_size_dialog_draws_stars_and_lines() {
    let star = ShapeSize::Star {
        outer: 30.0,
        inner: 12.0,
        points: 7,
    };
    assert!(matches!(
        sized(star, p(0.0, 0.0), false),
        Some(Geometry::Star { outer, inner, points: 7, .. }) if (outer - 30.0).abs() < 1e-9 && (inner - 12.0).abs() < 1e-9
    ));
    let line = ShapeSize::Line {
        length: 10.0,
        angle: 90.0,
    };
    match sized(line, p(5.0, 5.0), false) {
        Some(Geometry::Line { from, to }) => {
            assert_eq!((from.x, from.y), (5.0, 5.0));
            assert_close(to.x, 5.0);
            assert_close(to.y, 15.0);
        }
        other => panic!("line, got {other:?}"),
    }
}

#[test]
fn dragging_a_corner_widget_along_the_diagonal_rounds_by_half_the_move() {
    let r = Rect::new(0.0, 0.0, 100.0, 60.0);
    let from = p(10.0, 10.0);
    let tl = corner_radius_at(r, Corner::TopLeft, 0.0, from, p(18.0, 16.0));
    assert_close(tl, 7.0);
    // The bottom-right corner rounds when dragged up-left.
    let br = corner_radius_at(r, Corner::BottomRight, 5.0, from, p(0.0, 0.0));
    assert_close(br, 15.0);
}

#[test]
fn corner_radius_stays_between_zero_and_half_the_short_side() {
    let r = Rect::new(0.0, 0.0, 100.0, 60.0);
    let o = p(0.0, 0.0);
    assert_close(
        corner_radius_at(r, Corner::TopLeft, 4.0, o, p(-50.0, -50.0)),
        0.0,
    );
    assert_close(
        corner_radius_at(r, Corner::TopLeft, 4.0, o, p(500.0, 500.0)),
        30.0,
    );
}

#[test]
fn the_widget_sits_at_the_radius_but_never_on_the_corner() {
    let r = Rect::new(0.0, 0.0, 100.0, 60.0);
    let sharp = CornerRadii::default();
    assert_eq!(corner_widget(r, sharp, Corner::TopRight, 8.0), p(92.0, 8.0));
    let round = Corner::BottomLeft.with(sharp, 20.0);
    assert_eq!(
        corner_widget(r, round, Corner::BottomLeft, 8.0),
        p(20.0, 40.0)
    );
    let tiny = Rect::new(0.0, 0.0, 6.0, 6.0);
    assert_eq!(
        corner_widget(tiny, sharp, Corner::TopLeft, 8.0),
        p(3.0, 3.0),
        "at most the centre"
    );
}

fn kind_strategy() -> impl Strategy<Value = ShapeKind> {
    prop_oneof![
        Just(ShapeKind::Rect),
        Just(ShapeKind::Ellipse),
        (0u32..40).prop_map(|sides| ShapeKind::Polygon { sides }),
        (0u32..40, 0.01f64..0.99).prop_map(|(points, inner)| ShapeKind::Star { points, inner }),
        Just(ShapeKind::Line),
    ]
}

proptest! {
    /// Whatever the drag, a drawn shape is one the document accepts.
    #[test]
    fn every_drawn_shape_is_valid(
        kind in kind_strategy(),
        start in (-1e4f64..1e4, -1e4f64..1e4),
        end in (-1e4f64..1e4, -1e4f64..1e4),
        constrain: bool,
        from_centre: bool,
    ) {
        let mods = DrawMods { constrain, from_centre };
        if let Some(geometry) = drawn(kind, p(start.0, start.1), p(end.0, end.1), mods) {
            let (mut doc, layer) = common::doc();
            let id = common::id(&mut doc);
            let node = Node::new(id, NodeKind::Shape(Shape { geometry, style: Style::default() }));
            common::add(&mut doc, layer, vec![node]);
        }
    }
}
