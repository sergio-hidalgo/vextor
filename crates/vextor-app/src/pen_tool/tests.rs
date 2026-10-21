//! Scripted pen events → the expected `Path` (spec 07 acceptance).

use kurbo::Point;
use vextor_core::{
    Command, NodeId, NodeKind, NodeType, Path, PathNode, Selection, Transform, Vec2,
};

use super::*;
use crate::new_doc::new_document;
use crate::select::Mods;

const NONE: Mods = Mods {
    shift: false,
    alt: false,
};
const SHIFT: Mods = Mods {
    shift: true,
    alt: false,
};
const ALT: Mods = Mods {
    shift: false,
    alt: true,
};

/// One pen event, document space, at 100 %.
#[derive(Clone, Copy, Debug)]
enum Ev {
    Click(f64, f64),
    ShiftClick(f64, f64),
    /// Press at the first point, drag to the second, release.
    Drag((f64, f64), (f64, f64)),
    /// The same with Alt held while dragging.
    AltDrag((f64, f64), (f64, f64)),
    Backspace,
    Finish,
}

fn session() -> Session {
    Session::new(new_document("a", 400.0, 400.0))
}

fn run(s: &mut Session, pen: &mut PenTool, events: &[Ev]) {
    let pt = |(x, y): (f64, f64)| Point::new(x, y);
    for &e in events {
        match e {
            Ev::Click(x, y) => {
                pen.press(s, Point::new(x, y), NONE, 1.0).expect("press");
                pen.release(s).expect("release");
            }
            Ev::ShiftClick(x, y) => {
                pen.press(s, Point::new(x, y), SHIFT, 1.0).expect("press");
                pen.release(s).expect("release");
            }
            Ev::Drag(a, b) | Ev::AltDrag(a, b) => {
                let mods = if matches!(e, Ev::AltDrag(..)) {
                    ALT
                } else {
                    NONE
                };
                pen.press(s, pt(a), NONE, 1.0).expect("press");
                let mid = Point::new(f64::midpoint(a.0, b.0), f64::midpoint(a.1, b.1));
                pen.drag_to(s, mid, NONE, 1.0).expect("drag");
                pen.drag_to(s, pt(b), mods, 1.0).expect("drag");
                pen.release(s).expect("release");
            }
            Ev::Backspace => {
                pen.backspace(s).expect("backspace");
            }
            Ev::Finish => {
                pen.finish(s);
            }
        }
    }
}

/// Draws `events` with a fresh pen and returns the path drawn.
fn draw(s: &mut Session, events: &[Ev]) -> (NodeId, Path) {
    let mut pen = PenTool::default();
    run(s, &mut pen, events);
    let [id] = s.selected()[..] else {
        panic!("one path selected: {:?}", s.editor.selection);
    };
    (id, s.path_of(id).expect("a path").clone())
}

fn anchors(p: &Path, subpath: usize) -> Vec<(f64, f64)> {
    p.subpaths[subpath]
        .nodes
        .iter()
        .map(|n| (n.anchor.x, n.anchor.y))
        .collect()
}

#[test]
fn clicks_draw_a_straight_polyline_in_one_undo_step() {
    let mut s = session();
    let before = s.doc.clone();
    let (id, path) = draw(
        &mut s,
        &[
            Ev::Click(10.0, 10.0),
            Ev::Click(100.0, 10.0),
            Ev::Click(100.0, 80.0),
            Ev::Finish,
        ],
    );
    assert_eq!(
        anchors(&path, 0),
        vec![(10.0, 10.0), (100.0, 10.0), (100.0, 80.0)]
    );
    let sp = &path.subpaths[0];
    assert!(!sp.closed);
    assert!(
        sp.nodes
            .iter()
            .all(|n| n.handle_in.is_none() && n.handle_out.is_none())
    );
    let NodeKind::Shape(shape) = &s.doc.nodes[&id].kind else {
        panic!("shape");
    };
    assert!(shape.style.stroke.is_some(), "open segments show");
    assert_eq!(s.edit_menu().undo, Some(LABEL));
    assert!(s.undo());
    assert_eq!(s.doc, before, "the whole path was one step");
}

#[test]
fn a_drag_makes_a_symmetric_curved_node_and_alt_breaks_it() {
    let mut s = session();
    let (_, path) = draw(
        &mut s,
        &[
            Ev::Click(0.0, 0.0),
            Ev::Drag((100.0, 0.0), (130.0, 40.0)),
            Ev::AltDrag((200.0, 0.0), (200.0, 50.0)),
            Ev::Click(300.0, 0.0),
            Ev::Finish,
        ],
    );
    let n = &path.subpaths[0].nodes;
    assert_eq!(n.len(), 4);
    assert_eq!(n[1].kind, NodeType::Symmetric);
    assert_eq!(n[1].handle_out, Some(Vec2::new(30.0, 40.0)));
    assert_eq!(n[1].handle_in, Some(Vec2::new(-30.0, -40.0)));
    assert_eq!(n[2].kind, NodeType::Corner, "⌥ broke the handle");
    assert_eq!(n[2].handle_out, Some(Vec2::new(0.0, 50.0)));
    assert_eq!(
        n[2].handle_in,
        Some(Vec2::new(0.0, -25.0)),
        "the incoming handle stays where it was before ⌥"
    );
    assert_eq!(n[3], PathNode::corner(300.0, 0.0));
}

#[test]
fn clicking_the_first_node_closes_the_path() {
    let mut s = session();
    let mut pen = PenTool::default();
    run(
        &mut s,
        &mut pen,
        &[
            Ev::Click(0.0, 0.0),
            Ev::Click(100.0, 0.0),
            Ev::Click(50.0, 80.0),
        ],
    );
    assert_eq!(pen.hover(&s, Point::new(2.0, 1.0), 1.0), Hover::Close);
    run(&mut s, &mut pen, &[Ev::Click(2.0, 1.0)]);
    assert!(!pen.is_drawing(), "closing ends the path");
    let [id] = s.selected()[..] else {
        panic!("selected");
    };
    let path = s.path_of(id).expect("path");
    assert!(path.subpaths[0].closed);
    assert_eq!(path.subpaths[0].nodes.len(), 3, "no node added on close");
    assert_eq!(s.edit_menu().undo, Some(LABEL));
}

#[test]
fn dragging_on_the_first_node_curves_the_closing_segment() {
    let mut s = session();
    let (_, path) = draw(
        &mut s,
        &[
            Ev::Click(0.0, 0.0),
            Ev::Click(100.0, 0.0),
            Ev::Click(50.0, 80.0),
            Ev::Drag((0.0, 0.0), (0.0, -20.0)),
        ],
    );
    let first = path.subpaths[0].nodes[0];
    assert!(path.subpaths[0].closed);
    assert_eq!(first.kind, NodeType::Symmetric);
    assert_eq!(first.handle_in, Some(Vec2::new(0.0, 20.0)));
    assert_eq!(first.handle_out, Some(Vec2::new(0.0, -20.0)));
}

#[test]
fn backspace_removes_the_last_node_and_enter_with_one_node_draws_nothing() {
    let mut s = session();
    let (_, path) = draw(
        &mut s,
        &[
            Ev::Click(0.0, 0.0),
            Ev::Click(50.0, 0.0),
            Ev::Click(50.0, 50.0),
            Ev::Backspace,
            Ev::Finish,
        ],
    );
    assert_eq!(anchors(&path, 0), vec![(0.0, 0.0), (50.0, 0.0)]);

    let mut s = session();
    let before = s.doc.clone();
    let mut pen = PenTool::default();
    run(&mut s, &mut pen, &[Ev::Click(0.0, 0.0), Ev::Finish]);
    assert_eq!(s.doc, before, "a lone node is taken back");
    assert_eq!(s.edit_menu().undo, None);
    run(
        &mut s,
        &mut pen,
        &[
            Ev::Click(0.0, 0.0),
            Ev::Click(9.0, 9.0),
            Ev::Backspace,
            Ev::Backspace,
        ],
    );
    assert_eq!(s.doc, before, "⌫ down to nothing takes the path back");
    assert!(!pen.is_drawing());
}

#[test]
fn shift_keeps_the_segment_on_the_angle_step() {
    let mut s = session();
    let (_, path) = draw(
        &mut s,
        &[
            Ev::Click(0.0, 0.0),
            Ev::ShiftClick(100.0, 8.0),
            Ev::ShiftClick(160.0, 66.0),
            Ev::Finish,
        ],
    );
    let a = anchors(&path, 0);
    assert_eq!(a[1], (100.0, 0.0), "horizontal");
    let (dx, dy) = (a[2].0 - a[1].0, a[2].1 - a[1].1);
    assert!((dx - dy).abs() < 1e-9, "45°: {a:?}");
}

#[test]
fn clicking_an_open_end_continues_that_path() {
    let mut s = session();
    let (id, _) = draw(
        &mut s,
        &[Ev::Click(0.0, 0.0), Ev::Click(100.0, 0.0), Ev::Finish],
    );
    s.select(Vec::new());
    let mut pen = PenTool::default();
    assert!(matches!(
        pen.hover(&s, Point::new(1.0, 1.0), 1.0),
        Hover::End(_)
    ));
    // From the start: the path is drawn on from (0, 0).
    run(
        &mut s,
        &mut pen,
        &[Ev::Click(1.0, 1.0), Ev::Click(0.0, 100.0), Ev::Finish],
    );
    assert_eq!(s.selected(), vec![id], "the same shape");
    let path = s.path_of(id).expect("path");
    assert_eq!(
        anchors(path, 0),
        vec![(100.0, 0.0), (0.0, 0.0), (0.0, 100.0)]
    );
    assert_eq!(s.edit_menu().undo, Some(LABEL));
    s.undo();
    assert_eq!(s.path_of(id).expect("path").subpaths[0].nodes.len(), 2);
}

#[test]
fn backspace_never_eats_the_path_being_continued() {
    let mut s = session();
    let (id, before) = draw(
        &mut s,
        &[Ev::Click(0.0, 0.0), Ev::Click(100.0, 0.0), Ev::Finish],
    );
    let mut pen = PenTool::default();
    run(
        &mut s,
        &mut pen,
        &[
            Ev::Click(100.0, 0.0),
            Ev::Click(100.0, 50.0),
            Ev::Backspace,
            Ev::Backspace,
        ],
    );
    assert!(!pen.is_drawing());
    assert_eq!(s.path_of(id), Some(&before));
}

#[test]
fn clicking_another_paths_end_joins_the_two() {
    let mut s = session();
    let (a, _) = draw(
        &mut s,
        &[Ev::Click(0.0, 0.0), Ev::Click(100.0, 0.0), Ev::Finish],
    );
    let (b, _) = draw(
        &mut s,
        &[Ev::Click(0.0, 200.0), Ev::Click(100.0, 200.0), Ev::Finish],
    );
    s.select(Vec::new());
    let before = s.doc.clone();
    let mut pen = PenTool::default();
    run(
        &mut s,
        &mut pen,
        &[Ev::Click(100.0, 0.0), Ev::Click(100.0, 200.0)],
    );
    assert!(!pen.is_drawing(), "joining ends the path");
    assert!(!s.doc.nodes.contains_key(&b), "the other path moved in");
    let path = s.path_of(a).expect("path");
    assert_eq!(path.subpaths.len(), 1);
    assert_eq!(
        anchors(path, 0),
        vec![(0.0, 0.0), (100.0, 0.0), (100.0, 200.0), (0.0, 200.0)]
    );
    assert_eq!(s.edit_menu().undo, Some(LABEL));
    s.undo();
    assert_eq!(s.doc, before, "continue and join were one step");
}

#[test]
fn a_click_on_the_selected_paths_segment_inserts_a_node() {
    let mut s = session();
    let (id, before) = draw(
        &mut s,
        &[
            Ev::Click(0.0, 0.0),
            Ev::Click(100.0, 0.0),
            Ev::Click(100.0, 100.0),
            Ev::Finish,
        ],
    );
    let mut pen = PenTool::default();
    run(&mut s, &mut pen, &[Ev::Click(40.0, 2.0)]);
    assert!(!pen.is_drawing());
    let path = s.path_of(id).expect("path");
    assert_eq!(anchors(path, 0)[1], (40.0, 0.0));
    assert_eq!(
        s.editor.selection,
        Selection::PathNodes(vec![PathNodeRef {
            shape: id,
            subpath: 0,
            index: 1
        }])
    );
    assert_eq!(s.edit_menu().undo, Some(path_ops::ADD_NODE));
    s.undo();
    assert_eq!(s.path_of(id), Some(&before));
}

#[test]
fn another_edit_in_between_ends_the_path_without_losing_it() {
    let mut s = session();
    let mut pen = PenTool::default();
    run(
        &mut s,
        &mut pen,
        &[Ev::Click(0.0, 0.0), Ev::Click(50.0, 0.0)],
    );
    let [id] = s.selected()[..] else {
        panic!("selected");
    };
    s.apply(Command::SetTransform {
        ids: vec![id],
        transform: Transform::translate(5.0, 0.0),
    })
    .expect("moves");
    run(&mut s, &mut pen, &[Ev::Click(300.0, 300.0)]);
    assert_eq!(
        s.path_of(id).expect("kept").subpaths[0].nodes.len(),
        2,
        "the first path is left as it was"
    );
    assert!(pen.is_drawing(), "the click started a new path");
}

#[test]
fn drawing_inside_a_moved_layer_keeps_the_pointer_position() {
    let mut doc = new_document("a", 400.0, 400.0);
    let layer = doc.layers[0];
    doc.nodes.get_mut(&layer).expect("layer").transform = Transform::translate(100.0, 0.0);
    let mut s = Session::new(doc);
    let (id, path) = draw(
        &mut s,
        &[Ev::Click(10.0, 10.0), Ev::Click(50.0, 10.0), Ev::Finish],
    );
    let world = path_ops::world(&s, id);
    let first = path.subpaths[0].nodes[0].anchor;
    assert_eq!(world * Point::new(first.x, first.y), Point::new(10.0, 10.0));
}

#[test]
fn no_unlocked_layer_refuses_to_draw() {
    let mut s = session();
    let layer = s.doc.layers[0];
    s.doc.nodes.get_mut(&layer).expect("layer").locked = true;
    let mut pen = PenTool::default();
    assert!(pen.press(&mut s, Point::new(0.0, 0.0), NONE, 1.0).is_err());
    assert!(!pen.is_drawing());
}

#[test]
fn the_overlay_follows_the_draft() {
    let mut s = session();
    let mut pen = PenTool::default();
    assert!(pen.overlay().is_none());
    run(
        &mut s,
        &mut pen,
        &[Ev::Click(0.0, 0.0), Ev::Drag((50.0, 0.0), (60.0, 0.0))],
    );
    let o = pen.overlay().expect("drawing");
    assert_eq!(o.nodes, vec![Point::new(0.0, 0.0), Point::new(50.0, 0.0)]);
    assert_eq!(o.last_out, Some(Point::new(60.0, 0.0)));
    assert!(o.handles.is_none(), "only while dragging");
}

#[test]
fn clicking_the_last_node_again_adds_nothing_and_a_drag_reshapes_its_handle() {
    let mut s = session();
    let (_, path) = draw(
        &mut s,
        &[
            Ev::Click(0.0, 0.0),
            Ev::Drag((50.0, 0.0), (70.0, 0.0)),
            Ev::Click(50.0, 0.0),
            Ev::Drag((50.0, 0.0), (50.0, 30.0)),
            Ev::Finish,
        ],
    );
    let n = &path.subpaths[0].nodes;
    assert_eq!(n.len(), 2);
    assert_eq!(n[1].kind, NodeType::Corner);
    assert_eq!(n[1].handle_in, Some(Vec2::new(-20.0, 0.0)), "kept");
    assert_eq!(n[1].handle_out, Some(Vec2::new(0.0, 30.0)));
}

#[test]
fn ending_a_lone_first_node_reports_the_step_it_took_back() {
    let mut s = session();
    let (id, _) = draw(
        &mut s,
        &[Ev::Click(0.0, 0.0), Ev::Click(10.0, 0.0), Ev::Finish],
    );
    let mut pen = PenTool::default();
    run(&mut s, &mut pen, &[Ev::Click(200.0, 200.0)]);
    assert!(pen.finish(&mut s), "⌘Z ending it has done its undo");
    assert!(
        s.doc.nodes.contains_key(&id),
        "the earlier path is untouched"
    );
    assert_eq!(s.edit_menu().undo, Some(LABEL), "its step is still there");

    run(
        &mut s,
        &mut pen,
        &[Ev::Click(200.0, 200.0), Ev::Click(250.0, 200.0)],
    );
    assert!(!pen.finish(&mut s), "a real path is kept");
}

#[test]
fn dragging_out_a_node_previews_from_the_press_time_document() {
    let mut s = session();
    let mut pen = PenTool::default();
    pen.press(&mut s, Point::new(0.0, 0.0), NONE, 1.0)
        .expect("press");
    assert!(pen.preview().is_none(), "nothing until it moves");
    pen.drag_to(&mut s, Point::new(30.0, 0.0), NONE, 1.0)
        .expect("drag");
    let (before, id) = pen.preview().expect("dragging");
    assert!(
        before.nodes.contains_key(&id),
        "the node placed by the press"
    );
    pen.release(&mut s).expect("release");
    assert!(pen.preview().is_none());
}
