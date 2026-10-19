//! Resizing bakes the stretch into geometry: strokes keep their width.

#![allow(clippy::many_single_char_names)] // d = document, g/r = node ids

mod common;

use std::f64::consts::FRAC_PI_6;

use common::{add, doc, group, id, outlined, rect};
use kurbo::Affine;
use proptest::prelude::*;
use vextor_core::{
    Command, CornerRadii, Geometry, Node, NodeKind, NodeType, Path, PathNode, Shape, Style,
    SubPath, Transform, Vec2,
};
use vextor_geom::{Fonts, bounds, from_affine, polar, resize, stretched};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// Applies a resize as one batch, like the app does.
fn apply(d: &mut vextor_core::Document, ids: &[vextor_core::NodeId], change: Affine) {
    let r = resize(d, ids, change);
    d.apply(Command::Batch {
        label: "Scale",
        commands: vec![
            Command::SetTransforms {
                items: r.transforms,
            },
            Command::SetGeometry {
                items: r.geometries,
            },
        ],
    })
    .expect("applies");
}

fn transform_of(d: &vextor_core::Document, id: vextor_core::NodeId) -> Transform {
    d.nodes[&id].transform
}

fn geometry_of(d: &vextor_core::Document, id: vextor_core::NodeId) -> Geometry {
    match &d.nodes[&id].kind {
        NodeKind::Shape(s) => s.geometry.clone(),
        _ => panic!("not a shape"),
    }
}

#[test]
fn stretching_a_rect_changes_its_size_not_its_transform() {
    let (mut d, layer) = doc();
    let r = id(&mut d);
    add(
        &mut d,
        layer,
        vec![outlined(rect(r, 0.0, 0.0, 10.0, 10.0), 2.0)],
    );
    apply(&mut d, &[r], Affine::scale_non_uniform(3.0, 1.0));
    assert_eq!(transform_of(&d, r), Transform::IDENTITY);
    assert!(matches!(
        geometry_of(&d, r),
        Geometry::Rect { rect, .. } if close(rect.width, 30.0) && close(rect.height, 10.0)
    ));
}

#[test]
fn a_rotated_shape_keeps_its_rotation_and_gets_the_stretch() {
    let (mut d, layer) = doc();
    let r = id(&mut d);
    let mut node = rect(r, 0.0, 0.0, 10.0, 10.0);
    node.transform = from_affine(Affine::rotate(FRAC_PI_6));
    add(&mut d, layer, vec![node]);
    // Scaling along the shape's own axes (rotate, scale, rotate back) is an
    // axis-aligned stretch in its space: still a rect, still rotated 30°.
    let along = Affine::rotate(FRAC_PI_6)
        * Affine::scale_non_uniform(2.0, 1.0)
        * Affine::rotate(-FRAC_PI_6);
    apply(&mut d, &[r], along);
    let angle = vextor_geom::rotation_degrees(transform_of(&d, r));
    assert!(close(angle, 30.0), "{angle}");
    assert!(matches!(
        geometry_of(&d, r),
        Geometry::Rect { rect, .. } if close(rect.width, 20.0) && close(rect.height, 10.0)
    ));
}

#[test]
fn a_skewing_stretch_turns_a_rect_into_a_path_with_the_same_outline() {
    let (mut d, layer) = doc();
    let r = id(&mut d);
    let mut node = rect(r, 0.0, 0.0, 10.0, 10.0);
    node.transform = from_affine(Affine::rotate(FRAC_PI_6));
    add(&mut d, layer, vec![node]);
    let before = bounds(&d, &Fonts::default(), r).expect("visible");
    // A world-axis stretch on a rotated rect skews it in its own space.
    let change = Affine::scale_non_uniform(2.0, 1.0);
    apply(&mut d, &[r], change);
    assert!(matches!(geometry_of(&d, r), Geometry::Path(_)));
    let after = bounds(&d, &Fonts::default(), r).expect("visible");
    let want = change.transform_rect_bbox(before);
    assert!(close(after.x0, want.x0) && close(after.x1, want.x1));
    assert!(close(after.y0, want.y0) && close(after.y1, want.y1));
}

#[test]
fn a_group_resizes_the_shapes_inside() {
    let (mut d, layer) = doc();
    let (g, r) = (id(&mut d), id(&mut d));
    add(
        &mut d,
        layer,
        vec![
            group(g, vec![r], Transform::translate(5.0, 0.0)),
            rect(r, 0.0, 0.0, 10.0, 10.0),
        ],
    );
    apply(&mut d, &[g], Affine::scale_non_uniform(1.0, 4.0));
    assert_eq!(transform_of(&d, g), Transform::translate(5.0, 0.0));
    assert_eq!(transform_of(&d, r), Transform::IDENTITY);
    assert!(matches!(
        geometry_of(&d, r),
        Geometry::Rect { rect, .. } if close(rect.height, 40.0)
    ));
}

#[test]
fn corner_radius_keeps_its_size() {
    let g = Geometry::Rect {
        rect: vextor_core::Rect::new(0.0, 0.0, 10.0, 10.0),
        radius: CornerRadii {
            tl: 3.0,
            tr: 3.0,
            br: 3.0,
            bl: 3.0,
        },
    };
    let s = stretched(&g, Affine::scale_non_uniform(4.0, 1.0));
    assert!(matches!(s, Geometry::Rect { radius, .. } if close(radius.tl, 3.0)));
}

#[test]
fn a_curve_keeps_its_node_types_valid() {
    let (mut d, layer) = doc();
    let r = id(&mut d);
    let path = Path {
        subpaths: vec![SubPath {
            nodes: vec![
                PathNode::corner(0.0, 0.0),
                PathNode {
                    handle_in: Some(Vec2::new(-5.0, -3.0)),
                    handle_out: Some(Vec2::new(5.0, 3.0)),
                    kind: NodeType::Symmetric,
                    ..PathNode::corner(10.0, 5.0)
                },
                PathNode::corner(20.0, 0.0),
            ],
            closed: false,
        }],
        fill_rule: vextor_core::FillRule::NonZero,
    };
    let node = Node::new(
        r,
        NodeKind::Shape(Shape {
            geometry: Geometry::Path(path),
            style: Style::default(),
        }),
    );
    add(&mut d, layer, vec![outlined(node, 3.0)]);
    // Validation (symmetric handles) runs inside apply.
    apply(&mut d, &[r], Affine::scale_non_uniform(0.5, 3.0));
    let Geometry::Path(p) = geometry_of(&d, r) else {
        panic!("still a path");
    };
    assert_eq!(
        p.subpaths[0].nodes[1].anchor,
        vextor_core::Point::new(5.0, 15.0)
    );
    assert_eq!(p.subpaths[0].nodes[1].handle_out, Some(Vec2::new(2.5, 9.0)));
}

#[test]
fn images_keep_a_stretched_transform() {
    let (mut d, layer) = doc();
    let i = id(&mut d);
    let img = Node::new(
        i,
        NodeKind::Image(vextor_core::Image {
            rect: vextor_core::Rect::new(0.0, 0.0, 4.0, 4.0),
            source: vextor_core::ImageSource::Embedded {
                mime: vextor_core::ImageMime::Png,
                bytes: vec![0x89, b'P', b'N', b'G'],
            },
        }),
    );
    add(&mut d, layer, vec![img]);
    apply(&mut d, &[i], Affine::scale_non_uniform(2.0, 1.0));
    assert_eq!(transform_of(&d, i), Transform::scale(2.0, 1.0));
}

proptest! {
    /// rigid · stretch == m, rigid is orthogonal, stretch is symmetric.
    #[test]
    fn polar_splits_exactly(
        a in -5.0f64..5.0, b in -5.0f64..5.0, c in -5.0f64..5.0, d in -5.0f64..5.0,
        e in -50.0f64..50.0, f in -50.0f64..50.0,
    ) {
        prop_assume!((a * d - b * c).abs() > 1e-3);
        let m = Affine::new([a, b, c, d, e, f]);
        let (rigid, stretch) = polar(m);
        let back = (rigid * stretch).as_coeffs();
        for (x, y) in back.iter().zip(m.as_coeffs()) {
            prop_assert!((x - y).abs() < 1e-6);
        }
        let [ra, rb, rc, rd, ..] = rigid.as_coeffs();
        prop_assert!((ra * ra + rb * rb - 1.0).abs() < 1e-9);
        prop_assert!((ra * rc + rb * rd).abs() < 1e-9);
        let [_, sb, sc, _, ..] = stretch.as_coeffs();
        prop_assert!((sb - sc).abs() < 1e-6);
    }

    /// Whatever the nesting and the box scale, the resized object ends up
    /// with the bounds the box maths says, and no stroke is stretched.
    #[test]
    fn resized_bounds_follow_the_box(
        angle in -3.0f64..3.0,
        sx in 0.2f64..4.0,
        sy in 0.2f64..4.0,
    ) {
        let (mut d, layer) = doc();
        let (g, r) = (id(&mut d), id(&mut d));
        let t = from_affine(Affine::rotate(angle) * Affine::scale_non_uniform(1.5, 0.5));
        add(&mut d, layer, vec![group(g, vec![r], t), outlined(rect(r, 0.0, 0.0, 10.0, 10.0), 2.0)]);
        let b = bounds(&d, &Fonts::default(), g).expect("visible");
        let change = Affine::translate(b.origin().to_vec2())
            * Affine::scale_non_uniform(sx, sy)
            * Affine::translate(-b.origin().to_vec2());
        apply(&mut d, &[g], change);
        let after = bounds(&d, &Fonts::default(), g).expect("visible");
        let want = change.transform_rect_bbox(b);
        prop_assert!((after.x0 - want.x0).abs() < 1e-6 && (after.y1 - want.y1).abs() < 1e-6);
    }
}
