//! Transform-box maths, align / distribute and retargeting into nested
//! parents.

#![allow(clippy::many_single_char_names)] // d = document, g/r = node ids

mod common;

use std::f64::consts::FRAC_PI_2;

use common::{add, doc, group, id, rect};
use kurbo::{Affine, Point, Rect, Vec2};
use proptest::prelude::*;
use vextor_core::{Command, Transform};
use vextor_geom::{
    Align, Fonts, Handle, align, bounds, distribute, flip, from_affine, retarget, rotation_angle,
    rotation_degrees, scale,
};

const B: Rect = Rect::new(10.0, 20.0, 50.0, 40.0); // 40 × 20

fn close(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-6
}

#[test]
fn corner_scale_keeps_the_opposite_corner() {
    let t = scale(B, Handle::SE, Vec2::new(40.0, 20.0), false, false);
    assert!(close(t * Point::new(10.0, 20.0), Point::new(10.0, 20.0)));
    assert!(close(t * Point::new(50.0, 40.0), Point::new(90.0, 60.0)));
}

#[test]
fn edge_scale_changes_one_axis() {
    let t = scale(B, Handle::E, Vec2::new(20.0, 99.0), false, false);
    assert!(close(t * Point::new(50.0, 40.0), Point::new(70.0, 40.0)));
}

#[test]
fn shift_keeps_proportions() {
    // Dragged mostly right: x doubles, so y doubles too.
    let t = scale(B, Handle::SE, Vec2::new(40.0, 2.0), true, false);
    assert!(close(t * Point::new(50.0, 40.0), Point::new(90.0, 60.0)));
    // An edge handle scales the other axis alike, about the middle line.
    let e = scale(B, Handle::E, Vec2::new(40.0, 0.0), true, false);
    assert!(close(e * Point::new(50.0, 40.0), Point::new(90.0, 50.0)));
    assert!(close(e * Point::new(10.0, 20.0), Point::new(10.0, 10.0)));
}

#[test]
fn alt_scales_about_the_centre() {
    let t = scale(B, Handle::E, Vec2::new(20.0, 0.0), false, true);
    assert!(close(t * B.center(), B.center()));
    assert!(close(t * Point::new(50.0, 30.0), Point::new(70.0, 30.0)));
    assert!(close(t * Point::new(10.0, 30.0), Point::new(-10.0, 30.0)));
}

#[test]
fn dragging_past_the_opposite_side_mirrors_but_never_collapses() {
    let t = scale(B, Handle::E, Vec2::new(-80.0, 0.0), false, false);
    assert!(close(t * Point::new(50.0, 30.0), Point::new(-30.0, 30.0)));
    let flat = scale(B, Handle::E, Vec2::new(-40.0, 0.0), false, false);
    assert!(flat.determinant().abs() > 0.0);
}

#[test]
fn zero_width_box_scales_only_the_other_axis() {
    let line = Rect::new(0.0, 0.0, 0.0, 10.0);
    let t = scale(line, Handle::SE, Vec2::new(5.0, 10.0), false, false);
    assert!(close(t * Point::new(0.0, 10.0), Point::new(0.0, 20.0)));
}

#[test]
fn rotation_snaps_to_15_degrees() {
    let c = Point::new(0.0, 0.0);
    let a = rotation_angle(c, Point::new(10.0, 0.0), Point::new(10.0, 3.0), true);
    assert!((a.to_degrees() - 15.0).abs() < 1e-9);
    let free = rotation_angle(c, Point::new(10.0, 0.0), Point::new(0.0, 10.0), false);
    assert!((free - FRAC_PI_2).abs() < 1e-9);
}

#[test]
fn flip_mirrors_about_the_centre() {
    let h = flip(B, true);
    assert!(close(h * Point::new(10.0, 20.0), Point::new(50.0, 20.0)));
    let v = flip(B, false);
    assert!(close(v * Point::new(10.0, 20.0), Point::new(10.0, 40.0)));
}

#[test]
fn rotation_degrees_reads_the_x_axis() {
    let t = from_affine(Affine::rotate(30_f64.to_radians()) * Affine::scale(3.0));
    assert!((rotation_degrees(t) - 30.0).abs() < 1e-9);
    assert!((rotation_degrees(Transform::IDENTITY)).abs() < 1e-9);
}

#[test]
fn align_and_distribute() {
    let boxes = [
        Rect::new(0.0, 0.0, 10.0, 10.0),
        Rect::new(30.0, 5.0, 40.0, 25.0),
        Rect::new(100.0, 0.0, 120.0, 10.0),
    ];
    let target = Rect::new(0.0, 0.0, 120.0, 25.0);
    assert_eq!(
        align(&boxes, target, Align::Left)[2],
        Vec2::new(-100.0, 0.0)
    );
    assert_eq!(
        align(&boxes, target, Align::Bottom)[0],
        Vec2::new(0.0, 15.0)
    );
    assert_eq!(
        align(&boxes, target, Align::Centre)[1],
        Vec2::new(25.0, 0.0)
    );
    // Widths 10 + 10 + 20 = 40 over a 120 span: two gaps of 40.
    let d = distribute(&boxes, true);
    assert_eq!(d[0], Vec2::ZERO);
    assert_eq!(d[1], Vec2::new(20.0, 0.0));
    assert_eq!(d[2], Vec2::ZERO);
    assert!(
        distribute(&boxes[..2], true)
            .iter()
            .all(|v| *v == Vec2::ZERO)
    );
}

#[test]
fn retarget_moves_a_child_of_a_rotated_scaled_group_in_world_space() {
    let (mut d, layer) = doc();
    let (g, r) = (id(&mut d), id(&mut d));
    let t = from_affine(Affine::rotate(0.7) * Affine::scale_non_uniform(2.0, 0.5));
    add(
        &mut d,
        layer,
        vec![group(g, vec![r], t), rect(r, 0.0, 0.0, 10.0, 10.0)],
    );
    let before = bounds(&d, &Fonts::default(), r).expect("visible");
    let items = retarget(&d, &[r], Affine::translate((15.0, -4.0)));
    d.apply(Command::SetTransforms { items }).expect("applies");
    let after = bounds(&d, &Fonts::default(), r).expect("visible");
    assert!(close(
        after.origin(),
        before.origin() + Vec2::new(15.0, -4.0)
    ));
    assert!((after.width() - before.width()).abs() < 1e-6);
}

proptest! {
    /// Whatever the nesting transform, a world-space scale from a handle
    /// puts the object's bounds exactly where the box maths says.
    #[test]
    fn retargeted_bounds_follow_the_box(
        angle in -3.0f64..3.0,
        sx in 0.2f64..4.0,
        sy in 0.2f64..4.0,
        dx in -50.0f64..50.0,
        dy in -50.0f64..50.0,
    ) {
        let (mut d, layer) = doc();
        let (g, r) = (id(&mut d), id(&mut d));
        let t = from_affine(Affine::rotate(angle) * Affine::scale_non_uniform(sx, sy));
        add(&mut d, layer, vec![group(g, vec![r], t), rect(r, 0.0, 0.0, 10.0, 10.0)]);
        // Scale the group (an axis-aligned world box) from its SE corner.
        let b = bounds(&d, &Fonts::default(), g).expect("visible");
        let change = scale(b, Handle::SE, Vec2::new(dx, dy), false, false);
        let items = retarget(&d, &[g], change);
        d.apply(Command::SetTransforms { items }).expect("applies");
        let after = bounds(&d, &Fonts::default(), g).expect("visible");
        let want = change.transform_rect_bbox(b);
        prop_assert!((after.x0 - want.x0).abs() < 1e-6 && (after.y0 - want.y0).abs() < 1e-6);
        prop_assert!((after.x1 - want.x1).abs() < 1e-6 && (after.y1 - want.y1).abs() < 1e-6);
    }
}
