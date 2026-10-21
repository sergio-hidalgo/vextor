//! Node tool events → the expected selection and path; each drag is one
//! undo step (spec 07 acceptance).

use kurbo::{ParamCurve as _, Point};
use vextor_core::{
    Command, Geometry, Node, NodeKind, NodeType, Path, PathNode, Selection, Shape, Style, SubPath,
    Transform, Vec2,
};

use super::*;
use crate::new_doc::new_document;

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

/// A session with one open path (0,0) → (100,0) → (100,100), selected.
fn session_with(sp: SubPath) -> (Session, NodeId) {
    let mut doc = new_document("a", 400.0, 400.0);
    let layer = doc.layers[0];
    let id = doc.alloc_node_id();
    let shape = Shape {
        geometry: Geometry::Path(Path {
            subpaths: vec![sp],
            fill_rule: vextor_core::FillRule::NonZero,
        }),
        style: Style::default(),
    };
    doc.apply(Command::Insert {
        parent: Some(layer),
        index: 0,
        nodes: vec![Node::new(id, NodeKind::Shape(shape))],
    })
    .expect("inserts");
    let mut s = Session::new(doc);
    s.select(vec![id]);
    s.editor.tool = vextor_core::ToolId::Node;
    (s, id)
}

fn elbow() -> SubPath {
    SubPath {
        nodes: vec![
            PathNode::corner(0.0, 0.0),
            PathNode::corner(100.0, 0.0),
            PathNode::corner(100.0, 100.0),
        ],
        closed: false,
    }
}

fn at(shape: NodeId, index: usize) -> PathNodeRef {
    PathNodeRef {
        shape,
        subpath: 0,
        index,
    }
}

fn node(s: &Session, r: PathNodeRef) -> PathNode {
    *s.path_node(r).expect("node")
}

fn click(t: &mut NodeTool, s: &mut Session, p: (f64, f64), mods: Mods) {
    t.press(s, Point::new(p.0, p.1), mods, 1.0);
    t.release(s);
}

fn drag(t: &mut NodeTool, s: &mut Session, from: (f64, f64), to: (f64, f64), mods: Mods) {
    t.press(s, Point::new(from.0, from.1), NONE, 1.0);
    let mid = Point::new(f64::midpoint(from.0, to.0), f64::midpoint(from.1, to.1));
    t.drag_to(s, mid, mods, 1.0).expect("drags");
    t.drag_to(s, Point::new(to.0, to.1), mods, 1.0)
        .expect("drags");
    t.release(s);
}

#[test]
fn clicks_and_marquee_select_nodes() {
    let (mut s, id) = session_with(elbow());
    let mut t = NodeTool::default();
    click(&mut t, &mut s, (99.0, 1.0), NONE);
    assert_eq!(s.editor.selection, Selection::PathNodes(vec![at(id, 1)]));
    click(&mut t, &mut s, (1.0, 0.0), SHIFT);
    assert_eq!(selected_nodes(&s), vec![at(id, 0), at(id, 1)]);
    click(&mut t, &mut s, (1.0, 0.0), SHIFT);
    assert_eq!(selected_nodes(&s), vec![at(id, 1)], "⇧ again removes");
    drag(&mut t, &mut s, (90.0, -10.0), (110.0, 110.0), NONE);
    assert_eq!(selected_nodes(&s), vec![at(id, 1), at(id, 2)]);
    assert_eq!(s.edit_menu().undo, None, "selecting is no edit");
    click(&mut t, &mut s, (300.0, 300.0), NONE);
    assert_eq!(
        s.editor.selection,
        Selection::None,
        "a click outside leaves"
    );
}

#[test]
fn dragging_nodes_moves_them_in_one_undo_step() {
    let (mut s, id) = session_with(elbow());
    let before = s.doc.clone();
    let mut t = NodeTool::default();
    click(&mut t, &mut s, (100.0, 0.0), NONE);
    click(&mut t, &mut s, (100.0, 100.0), SHIFT);
    drag(&mut t, &mut s, (100.0, 100.0), (130.0, 110.0), NONE);
    assert_eq!(
        node(&s, at(id, 1)).anchor,
        vextor_core::Point::new(130.0, 10.0)
    );
    assert_eq!(
        node(&s, at(id, 2)).anchor,
        vextor_core::Point::new(130.0, 110.0)
    );
    assert_eq!(
        node(&s, at(id, 0)).anchor,
        vextor_core::Point::new(0.0, 0.0)
    );
    assert_eq!(s.edit_menu().undo, Some("Move nodes"));
    assert!(s.undo());
    assert_eq!(s.doc, before, "the whole drag was one step");
}

#[test]
fn shift_keeps_a_node_move_on_the_angle_step() {
    let (mut s, id) = session_with(elbow());
    let mut t = NodeTool::default();
    t.press(&mut s, Point::new(0.0, 0.0), NONE, 1.0);
    t.drag_to(&mut s, Point::new(50.0, 4.0), SHIFT, 1.0)
        .expect("drags");
    t.release(&mut s);
    assert_eq!(
        node(&s, at(id, 0)).anchor,
        vextor_core::Point::new(50.0, 0.0)
    );
}

#[test]
fn a_symmetric_handle_drags_its_partner_and_alt_breaks_it() {
    let mut sp = elbow();
    sp.nodes[1] = PathNode {
        handle_in: Some(Vec2::new(-20.0, 0.0)),
        handle_out: Some(Vec2::new(20.0, 0.0)),
        kind: NodeType::Symmetric,
        ..sp.nodes[1]
    };
    let (mut s, id) = session_with(sp);
    let mut t = NodeTool::default();
    click(&mut t, &mut s, (100.0, 0.0), NONE);
    let shown = handles(&s);
    assert_eq!(shown.len(), 2, "the selected node's two handles");
    drag(&mut t, &mut s, (120.0, 0.0), (100.0, 30.0), NONE);
    let n = node(&s, at(id, 1));
    assert_eq!(n.handle_out, Some(Vec2::new(0.0, 30.0)));
    assert_eq!(n.handle_in, Some(Vec2::new(0.0, -30.0)), "partner follows");
    assert_eq!(s.edit_menu().undo, Some(path_ops::MOVE_HANDLE));

    drag(&mut t, &mut s, (100.0, 30.0), (130.0, 30.0), ALT);
    let n = node(&s, at(id, 1));
    assert_eq!(n.kind, NodeType::Corner);
    assert_eq!(n.handle_out, Some(Vec2::new(30.0, 30.0)));
    assert_eq!(n.handle_in, Some(Vec2::new(0.0, -30.0)), "left alone");
}

#[test]
fn clicking_a_segment_selects_it_and_dragging_bends_it() {
    let (mut s, id) = session_with(elbow());
    let before = s.doc.clone();
    let mut t = NodeTool::default();
    click(&mut t, &mut s, (50.0, 2.0), NONE);
    assert_eq!(
        s.editor.selection,
        Selection::Segment {
            shape: id,
            subpath: 0,
            index: 0
        }
    );
    drag(&mut t, &mut s, (50.0, 0.0), (50.0, 30.0), NONE);
    let sp = &s.path_of(id).expect("path").subpaths[0];
    let c = vextor_geom::segment(sp, 0).expect("segment");
    assert!(
        (c.eval(0.5) - Point::new(50.0, 30.0)).hypot() < 1e-6,
        "through the pointer"
    );
    assert_eq!(s.edit_menu().undo, Some(path_ops::BEND));
    s.undo();
    assert_eq!(s.doc, before);
}

#[test]
fn double_clicking_a_node_cycles_its_type() {
    let (mut s, id) = session_with(elbow());
    let p = Point::new(100.0, 0.0);
    NodeTool::double_click(&mut s, p, 1.0).expect("cycles");
    let n = node(&s, at(id, 1));
    assert_eq!(n.kind, NodeType::Smooth);
    assert!(
        n.handle_in.is_some() && n.handle_out.is_some(),
        "handles made"
    );
    NodeTool::double_click(&mut s, p, 1.0).expect("cycles");
    assert_eq!(node(&s, at(id, 1)).kind, NodeType::Symmetric);
    assert!(node(&s, at(id, 1)).satisfies_type());
    NodeTool::double_click(&mut s, p, 1.0).expect("cycles");
    let n = node(&s, at(id, 1));
    assert_eq!(n.kind, NodeType::Corner);
    assert_eq!((n.handle_in, n.handle_out), (None, None), "sharp again");
    assert_eq!(s.edit_menu().undo, Some(path_ops::NODE_TYPE));
    NodeTool::double_click(&mut s, Point::new(300.0, 300.0), 1.0).expect("leaves");
    assert_eq!(s.editor.tool, vextor_core::ToolId::Select);
}

#[test]
fn the_round_widget_rounds_the_selected_corner() {
    let (mut s, id) = session_with(elbow());
    let mut t = NodeTool::default();
    click(&mut t, &mut s, (100.0, 0.0), NONE);
    let [(r, widget)] = radius_widgets(&s, 1.0)[..] else {
        panic!("one widget");
    };
    assert_eq!(r, at(id, 1));
    assert_eq!(
        NodeTool::target(&s, widget, 1.0),
        NodeTarget::Radius(at(id, 1))
    );
    // Along the bisector, towards (50, 50).
    let to = widget + kurbo::Vec2::new(-10.0, 10.0).normalize() * 20.0;
    drag(&mut t, &mut s, (widget.x, widget.y), (to.x, to.y), NONE);
    let radius = node(&s, at(id, 1)).corner_radius;
    assert!(radius > 1.0, "rounded: {radius}");
    assert_eq!(s.edit_menu().undo, Some(path_ops::ROUND));
    s.undo();
    assert!(node(&s, at(id, 1)).corner_radius.abs() < 1e-12);
}

#[test]
fn nodes_of_a_moved_path_are_hit_and_moved_in_document_space() {
    let (mut s, id) = session_with(elbow());
    s.apply(Command::SetTransform {
        ids: vec![id],
        transform: Transform::translate(50.0, 0.0),
    })
    .expect("moves");
    s.select(vec![id]);
    let mut t = NodeTool::default();
    drag(&mut t, &mut s, (150.0, 0.0), (160.0, 5.0), NONE);
    assert_eq!(
        node(&s, at(id, 1)).anchor,
        vextor_core::Point::new(110.0, 5.0)
    );
}

#[test]
fn clicking_another_object_switches_the_edited_path() {
    let (mut s, id) = session_with(elbow());
    let mut t = NodeTool::default();
    click(&mut t, &mut s, (100.0, 0.0), NONE);
    assert_eq!(edited(&s), Some(id));
    // A second path, not selected: a click on its body switches to it.
    let other = s.doc.alloc_node_id();
    let mut far = elbow();
    for n in &mut far.nodes {
        n.anchor.y += 200.0;
    }
    let layer = s.doc.layers[0];
    let shape = Shape {
        geometry: Geometry::Path(Path {
            subpaths: vec![far],
            fill_rule: vextor_core::FillRule::NonZero,
        }),
        style: Style::default(),
    };
    s.apply(Command::Insert {
        parent: Some(layer),
        index: 1,
        nodes: vec![Node::new(other, NodeKind::Shape(shape))],
    })
    .expect("inserts");
    click(&mut t, &mut s, (50.0, 200.0), NONE);
    assert_eq!(edited(&s), Some(other));
}

#[test]
fn a_drag_keeps_the_press_time_document_for_the_fast_path() {
    let (mut s, id) = session_with(elbow());
    let mut t = NodeTool::default();
    let before = s.doc.clone();
    t.press(&mut s, Point::new(100.0, 0.0), NONE, 1.0);
    t.drag_to(&mut s, Point::new(120.0, 0.0), NONE, 1.0)
        .expect("drags");
    let (doc, shape) = t.preview(&s).expect("dragging");
    assert_eq!((doc, shape), (&before, id));
    t.release(&mut s);
    assert!(t.preview(&s).is_none());
}

/// A closed path whose node 1's outgoing handle tip lands right on node
/// 2's incoming handle tip, at (150, 50).
fn overlapping_handles() -> SubPath {
    let curved = |x, y, h_in: (f64, f64), h_out: (f64, f64)| PathNode {
        handle_in: Some(Vec2::new(h_in.0, h_in.1)),
        handle_out: Some(Vec2::new(h_out.0, h_out.1)),
        kind: NodeType::Corner,
        ..PathNode::corner(x, y)
    };
    SubPath {
        nodes: vec![
            curved(100.0, 0.0, (-20.0, 0.0), (20.0, 0.0)),
            curved(200.0, 0.0, (-20.0, 0.0), (-50.0, 50.0)),
            curved(100.0, 100.0, (50.0, -50.0), (-20.0, 0.0)),
        ],
        closed: true,
    }
}

#[test]
fn a_selected_nodes_own_handles_are_told_apart_from_its_neighbours() {
    let (mut s, id) = session_with(overlapping_handles());
    select_nodes(&mut s, id, vec![at(id, 1)]);
    let shown = handles(&s);
    let own: Vec<_> = shown
        .iter()
        .filter(|h| h.own)
        .map(|h| (h.node, h.side))
        .collect();
    assert_eq!(own, vec![(at(id, 1), Side::In), (at(id, 1), Side::Out)]);
    assert!(
        shown.iter().filter(|h| !h.own).all(|h| h.node != at(id, 1)),
        "the neighbours' handles are not own"
    );
    assert!(
        !shown[0].own,
        "neighbours come first, own handles draw on top"
    );
}

#[test]
fn where_handles_overlap_the_selected_nodes_own_handle_is_grabbed() {
    let (mut s, id) = session_with(overlapping_handles());
    let tip = Point::new(150.0, 50.0);
    select_nodes(&mut s, id, vec![at(id, 1)]);
    assert_eq!(
        NodeTool::target(&s, tip, 1.0),
        NodeTarget::Handle(at(id, 1), Side::Out)
    );
    select_nodes(&mut s, id, vec![at(id, 2)]);
    assert_eq!(
        NodeTool::target(&s, tip, 1.0),
        NodeTarget::Handle(at(id, 2), Side::In),
        "and the other way round"
    );
    let before = node(&s, at(id, 1));
    let mut t = NodeTool::default();
    drag(&mut t, &mut s, (150.0, 50.0), (130.0, 70.0), NONE);
    assert_eq!(node(&s, at(id, 1)), before, "the neighbour stays put");
    assert_eq!(node(&s, at(id, 2)).handle_in, Some(Vec2::new(30.0, -30.0)));
}
