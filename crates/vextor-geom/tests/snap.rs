//! Snapping (spec 08): priority, tolerance at different zooms, targets
//! under rotated and scaled groups, box moves and smart guides.

#![allow(clippy::many_single_char_names)] // d = document, x/y/w/h = a rect

mod common;

use std::f64::consts::FRAC_PI_4;

use common::{add, doc, group, id, rect};
use kurbo::{Affine, Point, Rect};
use vextor_core::{Artboard, Document, NodeId};
use vextor_geom::{
    Fonts, SnapKind, SnapTargets, from_affine, minor_spacing, pixel_snap, tolerance,
};

fn close(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-6
}

/// A 10×10 rect at the origin; snapping to everything but the grid.
fn one_rect() -> (Document, NodeId) {
    let (mut d, layer) = doc();
    let a = id(&mut d);
    add(&mut d, layer, vec![rect(a, 0.0, 0.0, 10.0, 10.0)]);
    (d, a)
}

fn with_artboard(d: &mut Document, x: f64, y: f64, w: f64, h: f64) {
    let id = d.alloc_artboard_id();
    d.artboards.push(Artboard {
        id,
        name: "a".into(),
        rect: vextor_core::Rect::new(x, y, w, h),
        background: None,
    });
}

fn targets(d: &Document) -> SnapTargets {
    SnapTargets::collect(d, &Fonts::default(), &[])
}

#[test]
fn a_node_beats_the_grid_and_an_edge() {
    let (mut d, _) = one_rect();
    d.grid.snap.to_grid = true;
    let s = targets(&d)
        .point(Point::new(10.4, 10.3), 1.0)
        .expect("snaps");
    assert_eq!(s.kind, SnapKind::Node);
    assert!(close(s.point, Point::new(10.0, 10.0)));
}

#[test]
fn the_grid_beats_an_edge() {
    let (mut d, _) = one_rect();
    d.grid.snap.to_grid = true; // 8 px / 4 → every 2 px
    let s = targets(&d).point(Point::new(4.3, 0.4), 0.5).expect("snaps");
    assert_eq!(s.kind, SnapKind::Grid);
    assert!(close(s.point, Point::new(4.0, 0.0)));
}

#[test]
fn an_edge_beats_a_centre_and_an_artboard() {
    let (mut d, _) = one_rect();
    with_artboard(&mut d, 0.0, 0.0, 10.0, 10.0);
    let t = targets(&d);
    let edge = t.point(Point::new(5.0, 0.5), 1.0).expect("snaps");
    assert_eq!(edge.kind, SnapKind::Edge);
    assert!(close(edge.point, Point::new(5.0, 0.0)));
    let centre = t.point(Point::new(5.3, 5.2), 1.0).expect("snaps");
    assert_eq!(centre.kind, SnapKind::Centre);
    assert!(close(centre.point, Point::new(5.0, 5.0)));
}

#[test]
fn an_artboard_edge_and_centre_snap_last() {
    let (mut d, _) = one_rect();
    with_artboard(&mut d, 100.0, 100.0, 50.0, 50.0);
    let t = targets(&d);
    let side = t.point(Point::new(125.0, 100.4), 1.0).expect("snaps");
    assert_eq!(side.kind, SnapKind::Artboard);
    assert!(close(side.point, Point::new(125.0, 100.0)));
    let centre = t.point(Point::new(124.6, 125.3), 1.0).expect("snaps");
    assert!(close(centre.point, Point::new(125.0, 125.0)));
}

#[test]
fn nothing_within_the_tolerance_does_not_snap() {
    let (d, _) = one_rect();
    assert_eq!(targets(&d).point(Point::new(30.0, 30.0), 1.0), None);
}

#[test]
fn the_tolerance_is_in_screen_px_at_25_and_800_percent() {
    let (d, _) = one_rect();
    let t = targets(&d);
    let snap = &d.grid.snap; // 6 screen px
    let near_corner = Point::new(13.0, 10.0); // 3 doc px from (10, 10)
    // 25 %: 6 screen px are 24 doc px.
    assert!((tolerance(snap, 0.25) - 24.0).abs() < 1e-9);
    let s = t.point(near_corner, tolerance(snap, 0.25)).expect("snaps");
    assert!(close(s.point, Point::new(10.0, 10.0)));
    // 800 %: 0.75 doc px, so 3 px away is too far for the node; nearer is not.
    assert!((tolerance(snap, 8.0) - 0.75).abs() < 1e-9);
    assert_eq!(t.point(Point::new(13.0, 13.0), tolerance(snap, 8.0)), None);
    let s = t
        .point(Point::new(10.5, 10.3), tolerance(snap, 8.0))
        .expect("snaps");
    assert_eq!(s.kind, SnapKind::Node);
}

#[test]
fn nodes_of_a_rotated_group_snap_where_they_are_drawn() {
    let (mut d, layer) = doc();
    let (g, r) = (id(&mut d), id(&mut d));
    let turn = from_affine(Affine::rotate(FRAC_PI_4));
    add(
        &mut d,
        layer,
        vec![group(g, vec![r], turn), rect(r, 0.0, 0.0, 10.0, 0.0)],
    );
    let corner = Affine::rotate(FRAC_PI_4) * Point::new(10.0, 0.0);
    let s = targets(&d)
        .point(corner + kurbo::Vec2::new(0.3, -0.2), 1.0)
        .expect("snaps");
    assert_eq!(s.kind, SnapKind::Node);
    assert!(close(s.point, corner));
}

#[test]
fn nodes_of_a_scaled_group_snap_where_they_are_drawn() {
    let (mut d, layer) = doc();
    let (g, r) = (id(&mut d), id(&mut d));
    let twice = from_affine(Affine::scale(2.0));
    add(
        &mut d,
        layer,
        vec![group(g, vec![r], twice), rect(r, 0.0, 0.0, 10.0, 10.0)],
    );
    let s = targets(&d)
        .point(Point::new(19.6, 20.3), 1.0)
        .expect("snaps");
    assert!(close(s.point, Point::new(20.0, 20.0)));
}

#[test]
fn dragged_objects_are_not_targets() {
    let (d, a) = one_rect();
    let t = SnapTargets::collect(&d, &Fonts::default(), &[a]);
    assert_eq!(t.point(Point::new(10.2, 10.2), 1.0), None);
}

#[test]
fn turned_off_kinds_are_skipped() {
    let (mut d, _) = one_rect();
    d.grid.snap.to_nodes = false;
    let s = targets(&d)
        .point(Point::new(10.3, 10.2), 1.0)
        .expect("snaps");
    assert_eq!(s.kind, SnapKind::Edge);
}

#[test]
fn added_nodes_count_as_nodes() {
    let (mut d, _) = doc();
    d.grid.snap.to_artboard = false;
    let mut t = targets(&d);
    t.add_nodes([Point::new(40.0, 40.0)]);
    let s = t.point(Point::new(40.5, 39.6), 1.0).expect("snaps");
    assert_eq!(s.kind, SnapKind::Node);
}

#[test]
fn a_moved_box_lines_up_with_an_edge_and_shows_a_guide() {
    let (d, _) = one_rect();
    let moving = Rect::new(10.6, 50.0, 15.6, 55.0);
    let snap = targets(&d).moving_box(moving, 1.0);
    // Its left side meets the rect's right side (also its corner nodes).
    assert!((snap.offset.x + 0.6).abs() < 1e-9);
    assert_eq!(snap.x, Some(SnapKind::Node));
    assert_eq!(snap.y, None);
    assert_eq!(snap.guides.len(), 1);
    let g = snap.guides[0];
    assert!((g.from.x - 10.0).abs() < 1e-9 && (g.to.x - 10.0).abs() < 1e-9);
    // The guide spans from the rect to the box where it lands.
    assert!(g.from.y <= 0.0 && g.to.y >= 55.0);
}

#[test]
fn a_moved_box_lines_up_centre_to_centre() {
    let (mut d, _) = one_rect();
    d.grid.snap.to_nodes = false;
    d.grid.snap.to_edges = false;
    let moving = Rect::new(2.7, 40.0, 6.7, 44.0); // centre x 4.7
    let snap = targets(&d).moving_box(moving, 0.5);
    assert_eq!(snap.x, Some(SnapKind::Centre));
    assert!((snap.offset.x - 0.3).abs() < 1e-9);
}

#[test]
fn a_moved_box_lands_on_the_grid_without_a_guide() {
    let (mut d, _) = doc();
    d.grid.snap.to_grid = true;
    let snap = targets(&d).moving_box(Rect::new(3.9, 1.9, 7.9, 5.9), 0.5);
    assert_eq!(snap.kind(), Some(SnapKind::Grid));
    assert!((snap.offset.x - 0.1).abs() < 1e-9);
    assert!((snap.offset.y - 0.1).abs() < 1e-9);
    assert_eq!(snap.guides.len(), 0);
}

#[test]
fn grid_spacing_and_pixel_snap() {
    let (d, _) = doc();
    assert_eq!(minor_spacing(&d.grid), Some(2.0));
    let mut bad = d.grid.clone();
    bad.size = 0.0;
    assert_eq!(minor_spacing(&bad), None);
    assert!(close(
        pixel_snap(Point::new(3.4, -2.6)),
        Point::new(3.0, -3.0)
    ));
}

/// Snapping on a 500-object document: collecting at press and resolving
/// a frame. Run with
/// `cargo test --release -p vextor-geom --test snap time_snap -- --ignored --nocapture`.
#[test]
#[ignore = "timing, not a check"]
fn time_snap_500_objects() {
    let (mut d, layer) = doc();
    d.grid.snap.to_grid = true;
    let nodes: Vec<_> = (0..500)
        .map(|i| {
            let n = id(&mut d);
            let (x, y) = (f64::from(i % 25) * 40.0, f64::from(i / 25) * 40.0);
            rect(n, x, y, 30.0, 30.0)
        })
        .collect();
    add(&mut d, layer, nodes);
    let started = std::time::Instant::now();
    let t = targets(&d);
    let collect = started.elapsed();
    let started = std::time::Instant::now();
    for i in 0..100 {
        let p = Point::new(f64::from(i) * 7.3, f64::from(i) * 3.1);
        std::hint::black_box(t.point(p, 1.5));
        std::hint::black_box(t.moving_box(Rect::new(p.x, p.y, p.x + 30.0, p.y + 30.0), 1.5));
    }
    let frame = started.elapsed() / 100;
    println!("collect {collect:?}, one frame (point + box) {frame:?}");
}
