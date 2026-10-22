//! Hit-testing and bounds, including rotated and scaled groups.

#![allow(clippy::many_single_char_names)] // d = document, a/b/g/r = node ids

mod common;

use std::f64::consts::FRAC_PI_4;

use common::{add, doc, group, id, outlined, rect};
use kurbo::{Point, Rect};
use vextor_core::{
    Command, Container, FillRule, Geometry, NodeKind, Path, PathNode, Shape, Style, SubPath,
    Transform,
};
use vextor_geom::{
    Fonts, bounds, from_affine, overlaps, pick, selectable, touching, union_bounds, visual_bounds,
};

fn rect_close(a: Rect, b: Rect) -> bool {
    [a.x0 - b.x0, a.y0 - b.y0, a.x1 - b.x1, a.y1 - b.y1]
        .iter()
        .all(|d| d.abs() < 1e-6)
}

#[test]
fn pick_finds_the_topmost_object() {
    let (mut d, layer) = doc();
    let (a, b) = (id(&mut d), id(&mut d));
    add(&mut d, layer, vec![rect(a, 0.0, 0.0, 10.0, 10.0)]);
    add(&mut d, layer, vec![rect(b, 5.0, 5.0, 10.0, 10.0)]);
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(7.0, 7.0), 0.0),
        Some(b)
    );
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(2.0, 2.0), 0.0),
        Some(a)
    );
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(30.0, 30.0), 0.0),
        None
    );
}

#[test]
fn pick_selects_the_whole_group() {
    let (mut d, layer) = doc();
    let (g, r) = (id(&mut d), id(&mut d));
    add(
        &mut d,
        layer,
        vec![
            group(g, vec![r], Transform::IDENTITY),
            rect(r, 0.0, 0.0, 4.0, 4.0),
        ],
    );
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(1.0, 1.0), 0.0),
        Some(g)
    );
}

#[test]
fn pick_follows_a_rotated_and_scaled_group() {
    let (mut d, layer) = doc();
    let (g, r) = (id(&mut d), id(&mut d));
    // A 10×10 square at the origin, scaled 2× then rotated 45° about the
    // origin: its corner (20, 0) ends up on the diagonal at (14.1, 14.1).
    let t = from_affine(kurbo::Affine::rotate(FRAC_PI_4) * kurbo::Affine::scale(2.0));
    add(
        &mut d,
        layer,
        vec![group(g, vec![r], t), rect(r, 0.0, 0.0, 10.0, 10.0)],
    );
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(0.0, 20.0), 0.0),
        Some(g),
        "inside after rotation"
    );
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(15.0, 2.0), 0.0),
        None,
        "inside before rotation only"
    );
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(0.0, 27.0), 0.0),
        Some(g),
        "inside only thanks to the scale"
    );
    let b = bounds(&d, &Fonts::default(), g).expect("visible");
    let half = 20.0 * FRAC_PI_4.cos();
    assert!(rect_close(b, Rect::new(-half, 0.0, half, 2.0 * half)));
}

#[test]
fn pick_scales_the_stroke_with_its_transform() {
    let (mut d, layer) = doc();
    let r = id(&mut d);
    let mut node = outlined(rect(r, 0.0, 0.0, 10.0, 10.0), 2.0);
    node.transform = Transform::scale(3.0, 3.0);
    add(&mut d, layer, vec![node]);
    // Edge at x = 30; stroke 2 × 3 = 6 wide, so it reaches x = 33.
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(32.5, 15.0), 0.0),
        Some(r)
    );
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(34.0, 15.0), 0.0),
        None
    );
    // No fill: the middle is empty.
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(15.0, 15.0), 0.0),
        None
    );
    // Tolerance widens the reach.
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(34.0, 15.0), 2.0),
        Some(r)
    );
}

#[test]
fn pick_respects_even_odd_holes() {
    let (mut d, layer) = doc();
    let r = id(&mut d);
    let square = |x: f64, s: f64| SubPath {
        nodes: vec![
            PathNode::corner(x, x),
            PathNode::corner(x + s, x),
            PathNode::corner(x + s, x + s),
            PathNode::corner(x, x + s),
        ],
        closed: true,
    };
    let ring = Path {
        subpaths: vec![square(0.0, 30.0), square(10.0, 10.0)],
        fill_rule: FillRule::EvenOdd,
    };
    let node = vextor_core::Node::new(
        r,
        NodeKind::Shape(Shape {
            geometry: Geometry::Path(ring),
            style: Style::default(),
        }),
    );
    add(&mut d, layer, vec![node]);
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(5.0, 5.0), 0.0),
        Some(r)
    );
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(15.0, 15.0), 0.0),
        None,
        "the hole"
    );
}

#[test]
fn hidden_locked_and_locked_layers_are_not_hit() {
    let (mut d, layer) = doc();
    let (a, b) = (id(&mut d), id(&mut d));
    let mut hidden = rect(a, 0.0, 0.0, 10.0, 10.0);
    hidden.visible = false;
    let mut locked = rect(b, 20.0, 0.0, 10.0, 10.0);
    locked.locked = true;
    add(&mut d, layer, vec![hidden, locked]);
    assert_eq!(pick(&d, &Fonts::default(), Point::new(5.0, 5.0), 0.0), None);
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(25.0, 5.0), 0.0),
        None
    );
    assert_eq!(selectable(&d).len(), 0);

    let (mut d, layer) = doc();
    let c = id(&mut d);
    add(&mut d, layer, vec![rect(c, 0.0, 0.0, 10.0, 10.0)]);
    let l = d.nodes[&layer].clone();
    d.apply(Command::SetProps {
        id: layer,
        name: l.name,
        visible: true,
        locked: true,
        opacity: 1.0,
    })
    .expect("locks");
    assert_eq!(pick(&d, &Fonts::default(), Point::new(5.0, 5.0), 0.0), None);
}

#[test]
fn clipped_content_is_hit_only_inside_the_clip() {
    let (mut d, layer) = doc();
    let (g, clip, r) = (id(&mut d), id(&mut d), id(&mut d));
    let mut grp = group(g, vec![clip, r], Transform::IDENTITY);
    if let NodeKind::Group(Container { clip: c, .. }) = &mut grp.kind {
        *c = Some(clip);
    }
    add(
        &mut d,
        layer,
        vec![
            grp,
            rect(clip, 0.0, 0.0, 5.0, 5.0),
            rect(r, 0.0, 0.0, 20.0, 20.0),
        ],
    );
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(2.0, 2.0), 0.0),
        Some(g)
    );
    assert_eq!(
        pick(&d, &Fonts::default(), Point::new(10.0, 10.0), 0.0),
        None,
        "outside the clip"
    );
    let b = bounds(&d, &Fonts::default(), g).expect("visible");
    assert!(rect_close(b, Rect::new(0.0, 0.0, 5.0, 5.0)));
}

#[test]
fn marquee_touches_shapes_not_their_empty_bounding_box() {
    let (mut d, layer) = doc();
    let (line, a, b) = (id(&mut d), id(&mut d), id(&mut d));
    let diagonal = vextor_core::Node::new(
        line,
        NodeKind::Shape(Shape {
            geometry: Geometry::Line {
                from: vextor_core::Point::new(0.0, 0.0),
                to: vextor_core::Point::new(100.0, 100.0),
            },
            style: Style::default(),
        }),
    );
    add(
        &mut d,
        layer,
        vec![
            outlined(diagonal, 1.0),
            rect(a, 200.0, 200.0, 50.0, 50.0),
            rect(b, 300.0, 0.0, 10.0, 10.0),
        ],
    );
    // A box in the empty corner of the line's bounding box.
    assert_eq!(
        touching(&d, &Fonts::default(), Rect::new(70.0, 0.0, 95.0, 20.0)).len(),
        0
    );
    // Crossing the line.
    assert_eq!(
        touching(&d, &Fonts::default(), Rect::new(40.0, 0.0, 60.0, 100.0)),
        vec![line]
    );
    // Fully inside the big rect.
    assert_eq!(
        touching(&d, &Fonts::default(), Rect::new(210.0, 210.0, 220.0, 220.0)),
        vec![a]
    );
    // Around two of them, bottom → top.
    assert_eq!(
        touching(&d, &Fonts::default(), Rect::new(150.0, -10.0, 400.0, 300.0)),
        vec![a, b]
    );
}

#[test]
fn union_bounds_covers_every_object() {
    let (mut d, layer) = doc();
    let (a, b) = (id(&mut d), id(&mut d));
    let mut moved = rect(b, 0.0, 0.0, 10.0, 10.0);
    moved.transform = Transform::translate(50.0, -5.0);
    add(&mut d, layer, vec![rect(a, 0.0, 0.0, 10.0, 10.0), moved]);
    let u = union_bounds(&d, &Fonts::default(), &[a, b]).expect("visible");
    assert!(rect_close(u, Rect::new(0.0, -5.0, 60.0, 10.0)));
}

#[test]
fn overlaps_case_table() {
    let a = Rect::new(0.0, 0.0, 10.0, 10.0);
    let cases = [
        ("apart", Rect::new(20.0, 0.0, 30.0, 10.0), false),
        ("apart vertically", Rect::new(0.0, 11.0, 10.0, 20.0), false),
        ("crossing", Rect::new(5.0, 5.0, 15.0, 15.0), true),
        ("touching an edge", Rect::new(10.0, 0.0, 20.0, 10.0), true),
        ("touching a corner", Rect::new(10.0, 10.0, 20.0, 20.0), true),
        ("nested", Rect::new(2.0, 2.0, 4.0, 4.0), true),
        ("identical", a, true),
    ];
    for (name, b, expected) in cases {
        assert_eq!(overlaps(a, b), expected, "{name}");
        assert_eq!(overlaps(b, a), expected, "{name}, swapped");
    }
}

#[test]
fn visual_bounds_include_half_the_stroke() {
    let (mut d, layer) = doc();
    let (plain, stroked, g, inner) = (id(&mut d), id(&mut d), id(&mut d), id(&mut d));
    add(&mut d, layer, vec![rect(plain, 0.0, 0.0, 10.0, 10.0)]);
    add(
        &mut d,
        layer,
        vec![outlined(rect(stroked, 0.0, 0.0, 10.0, 10.0), 4.0)],
    );
    add(
        &mut d,
        layer,
        vec![
            group(g, vec![inner], Transform::scale(2.0, 2.0)),
            outlined(rect(inner, 0.0, 0.0, 10.0, 10.0), 4.0),
        ],
    );
    let fonts = Fonts::default();
    let v = |id| visual_bounds(&d, &fonts, id).expect("visible");
    assert!(rect_close(v(plain), Rect::new(0.0, 0.0, 10.0, 10.0)));
    assert!(rect_close(v(stroked), Rect::new(-2.0, -2.0, 12.0, 12.0)));
    // The group scales the stroke with it: 4 px wide becomes 8.
    assert!(rect_close(v(g), Rect::new(-4.0, -4.0, 24.0, 24.0)));
    // Plain bounds keep leaving strokes out.
    let b = bounds(&d, &fonts, stroked).expect("visible");
    assert!(rect_close(b, Rect::new(0.0, 0.0, 10.0, 10.0)));
}
