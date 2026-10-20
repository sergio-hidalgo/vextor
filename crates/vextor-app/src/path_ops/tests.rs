//! Every path action is one undo step that undo reverts exactly.

use vextor_core::{
    Command, Document, Geometry, Node, NodeKind, NodeType, Path, PathNode, Shape, Style, SubPath,
    Vec2,
};

use super::*;
use crate::new_doc::new_document;

fn open_path(points: &[(f64, f64)]) -> SubPath {
    SubPath {
        nodes: points
            .iter()
            .map(|&(x, y)| PathNode::corner(x, y))
            .collect(),
        closed: false,
    }
}

fn square() -> SubPath {
    SubPath {
        closed: true,
        ..open_path(&[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)])
    }
}

/// A session with one path per entry of `paths`, the first selected.
fn session(paths: Vec<Vec<SubPath>>) -> (Session, Vec<NodeId>) {
    let mut doc = new_document("a", 400.0, 400.0);
    let layer = doc.layers[0];
    let mut ids = Vec::new();
    let nodes = paths
        .into_iter()
        .map(|subpaths| {
            let id = doc.alloc_node_id();
            ids.push(id);
            let shape = Shape {
                geometry: Geometry::Path(Path {
                    subpaths,
                    fill_rule: vextor_core::FillRule::NonZero,
                }),
                style: Style::default(),
            };
            Node::new(id, NodeKind::Shape(shape))
        })
        .collect();
    doc.apply(Command::Insert {
        parent: Some(layer),
        index: 0,
        nodes,
    })
    .expect("inserts");
    let mut s = Session::new(doc);
    s.select(vec![ids[0]]);
    (s, ids)
}

fn at(shape: NodeId, subpath: usize, index: usize) -> PathNodeRef {
    PathNodeRef {
        shape,
        subpath,
        index,
    }
}

/// Runs `action`, checks it made one step labelled `label` and that undo
/// brings back the document as it was.
fn one_undoable_step(
    s: &mut Session,
    label: &str,
    action: impl FnOnce(&mut Session) -> Result<(), String>,
) -> Document {
    let before = s.doc.clone();
    let earlier = s.edit_menu().undo;
    action(s).expect("applies");
    assert_ne!(s.doc, before, "{label}: something changed");
    assert_eq!(s.edit_menu().undo, Some(label));
    let after = s.doc.clone();
    assert!(s.undo());
    assert_eq!(s.doc, before, "{label}: undo restores");
    assert_eq!(s.edit_menu().undo, earlier, "{label}: one step");
    assert!(s.redo());
    assert_eq!(s.doc, after, "{label}: redo re-applies");
    after
}

fn path(s: &Session, id: NodeId) -> &Path {
    s.path_of(id).expect("path")
}

#[test]
fn close_and_open() {
    let (mut s, ids) = session(vec![vec![open_path(&[
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 10.0),
    ])]]);
    assert_eq!(close_target(&s), Some((ids[0], 0)));
    one_undoable_step(&mut s, "Close path", close);
    assert!(path(&s, ids[0]).subpaths[0].closed);

    select_nodes(&mut s, ids[0], vec![at(ids[0], 0, 1)]);
    assert_eq!(open_target(&s), Some(at(ids[0], 0, 1)));
    one_undoable_step(&mut s, "Open path", open);
    let sp = &path(&s, ids[0]).subpaths[0];
    assert!(!sp.closed);
    assert_eq!(sp.nodes.len(), 4, "the node is at both ends");
}

#[test]
fn split_at_an_interior_node() {
    let (mut s, ids) = session(vec![vec![open_path(&[
        (0.0, 0.0),
        (10.0, 0.0),
        (20.0, 0.0),
    ])]]);
    select_nodes(&mut s, ids[0], vec![at(ids[0], 0, 0)]);
    assert_eq!(split_target(&s), None, "an end cannot split");
    select_nodes(&mut s, ids[0], vec![at(ids[0], 0, 1)]);
    one_undoable_step(&mut s, "Split", split);
    assert_eq!(path(&s, ids[0]).subpaths.len(), 2);
}

#[test]
fn reverse_turns_every_subpath() {
    let (mut s, ids) = session(vec![vec![open_path(&[(0.0, 0.0), (10.0, 0.0)])]]);
    one_undoable_step(&mut s, REVERSE, reverse);
    assert_eq!(
        path(&s, ids[0]).subpaths[0].nodes[0].anchor,
        vextor_core::Point::new(10.0, 0.0)
    );
}

#[test]
fn join_two_ends_of_one_path_and_two_paths() {
    let (mut s, ids) = session(vec![vec![open_path(&[
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 10.0),
    ])]]);
    select_nodes(&mut s, ids[0], vec![at(ids[0], 0, 0), at(ids[0], 0, 2)]);
    assert!(matches!(join_target(&s), Some(Join::Endpoints(..))));
    one_undoable_step(&mut s, "Join", join);
    assert!(path(&s, ids[0]).subpaths[0].closed);

    let (mut s, ids) = session(vec![
        vec![open_path(&[(0.0, 0.0), (10.0, 0.0)])],
        vec![open_path(&[(50.0, 50.0), (12.0, 0.0)])],
    ]);
    s.select(ids.clone());
    assert_eq!(join_target(&s), Some(Join::Paths(ids[0], ids[1])));
    one_undoable_step(&mut s, JOIN, join);
    assert!(!s.doc.nodes.contains_key(&ids[1]));
    let anchors: Vec<_> = path(&s, ids[0]).subpaths[0]
        .nodes
        .iter()
        .map(|n| (n.anchor.x, n.anchor.y))
        .collect();
    assert_eq!(
        anchors,
        vec![(0.0, 0.0), (10.0, 0.0), (12.0, 0.0), (50.0, 50.0)]
    );
    assert_eq!(s.selected(), vec![ids[0]]);
}

#[test]
fn toggle_and_insert_on_a_segment() {
    let (mut s, ids) = session(vec![vec![square()]]);
    let segment = Selection::Segment {
        shape: ids[0],
        subpath: 0,
        index: 0,
    };
    s.editor.selection = segment.clone();
    one_undoable_step(&mut s, "Toggle segment", toggle_segment);
    assert!(path(&s, ids[0]).subpaths[0].nodes[0].handle_out.is_some());

    s.editor.selection = segment;
    let id = ids[0];
    one_undoable_step(&mut s, ADD_NODE, |s| {
        insert_in_segment(s)?;
        assert_eq!(
            s.editor.selection,
            Selection::PathNodes(vec![at(id, 0, 1)]),
            "the new node is selected"
        );
        Ok(())
    });
    assert_eq!(path(&s, ids[0]).subpaths[0].nodes.len(), 5);
}

#[test]
fn deleting_nodes_keeps_the_path_continuous_and_the_last_ones_remove_it() {
    let (mut s, ids) = session(vec![vec![square()]]);
    select_nodes(&mut s, ids[0], vec![at(ids[0], 0, 1)]);
    one_undoable_step(&mut s, "Delete nodes", delete_nodes);
    let sp = &path(&s, ids[0]).subpaths[0];
    assert_eq!(sp.nodes.len(), 3);
    assert!(sp.closed, "still one closed outline");
    assert_eq!(s.selected(), vec![ids[0]]);

    let (mut s, ids) = session(vec![vec![open_path(&[(0.0, 0.0), (10.0, 0.0)])]]);
    select_nodes(&mut s, ids[0], vec![at(ids[0], 0, 0)]);
    one_undoable_step(&mut s, "Delete nodes", delete_nodes);
    assert!(
        !s.doc.nodes.contains_key(&ids[0]),
        "a lone node left over goes too, and the shape with it"
    );
    assert_eq!(s.editor.selection, Selection::None);
}

#[test]
fn nudge_type_handles_position_and_radius() {
    let (mut s, ids) = session(vec![vec![square()]]);
    let r = at(ids[0], 0, 1);
    select_nodes(&mut s, ids[0], vec![r]);
    one_undoable_step(&mut s, "Move nodes", |s| nudge_nodes(s, 1.0, 0.0));
    assert_eq!(s.path_node(r).expect("node").anchor.x, 101.0);

    one_undoable_step(&mut s, NODE_TYPE, |s| set_node_type(s, r, NodeType::Smooth));
    assert_eq!(s.path_node(r).expect("node").kind, NodeType::Smooth);

    one_undoable_step(&mut s, MOVE_HANDLE, |s| {
        set_handle(
            s,
            r,
            vextor_core::Side::Out,
            Some(Vec2::new(0.0, 40.0)),
            true,
        )
    });
    let n = s.path_node(r).expect("node");
    assert_eq!(
        (n.kind, n.handle_out),
        (NodeType::Corner, Some(Vec2::new(0.0, 40.0)))
    );

    one_undoable_step(&mut s, "Move nodes", |s| {
        move_node_to(s, r, kurbo::Point::new(120.0, 5.0))
    });
    assert_eq!(
        s.path_node(r).expect("node").anchor,
        vextor_core::Point::new(120.0, 5.0)
    );

    let corner = at(ids[0], 0, 3);
    one_undoable_step(&mut s, ROUND, |s| set_corner_radius(s, &[corner], 8.0));
    assert_eq!(s.path_node(corner).expect("node").corner_radius, 8.0);

    one_undoable_step(&mut s, BEND, |s| {
        set_segment_handles(
            s,
            (ids[0], 0, 2),
            Vec2::new(0.0, 10.0),
            Vec2::new(0.0, 10.0),
        )
    });
}

#[test]
fn select_all_takes_every_node_of_the_edited_path() {
    let (mut s, ids) = session(vec![vec![square(), open_path(&[(5.0, 5.0)])]]);
    select_all_nodes(&mut s);
    assert_eq!(selected_nodes(&s).len(), 5);
    assert_eq!(edited(&s), Some(ids[0]));
    assert!(editing_nodes(&s));
}

#[test]
fn undo_restoring_stale_nodes_keeps_the_path_selected() {
    let (mut s, ids) = session(vec![vec![square()]]);
    select_nodes(&mut s, ids[0], vec![at(ids[0], 0, 3)]);
    delete_nodes(&mut s).expect("deletes");
    // Redo after an undo restores a selection pointing at node 3.
    s.undo();
    s.redo();
    assert!(s.path_node(at(ids[0], 0, 3)).is_none());
    assert_eq!(s.selected(), vec![ids[0]]);
}
