//! Undo history: limit, redo, gestures, selection, paste ids.

mod common;

use common::{fixture, path_mut};
use vextor_core::{
    ClipboardPayload, Command, History, NodeId, PathNodeRef, Point, Selection, Transform, Vec2,
};

fn move_cmd(shape: NodeId, dx: f64) -> Command {
    Command::MovePathNodes {
        nodes: vec![PathNodeRef {
            shape,
            subpath: 0,
            index: 0,
        }],
        delta: Vec2::new(dx, 0.0),
    }
}

#[test]
fn keeps_at_most_the_limit() {
    let mut fx = fixture();
    let mut h = History::with_limit(3);
    for i in 0..5 {
        let t = Transform::translate(f64::from(i), 0.0);
        h.apply(
            &mut fx.doc,
            Command::SetTransform {
                ids: vec![fx.star],
                transform: t,
            },
            &Selection::None,
        )
        .expect("applies");
    }
    assert_eq!(h.undo_len(), 3);
    while h.undo(&mut fx.doc).is_some() {}
    assert_eq!(
        fx.doc.nodes[&fx.star].transform,
        Transform::translate(1.0, 0.0)
    );
}

#[test]
fn default_limit_is_200() {
    let mut fx = fixture();
    let mut h = History::new();
    for i in 0..210 {
        h.apply(
            &mut fx.doc,
            move_cmd(fx.path, 1.0 + f64::from(i)),
            &Selection::None,
        )
        .expect("applies");
    }
    assert_eq!(h.undo_len(), vextor_core::DEFAULT_LIMIT);
}

#[test]
fn a_new_command_clears_redo() {
    let mut fx = fixture();
    let mut h = History::new();
    h.apply(&mut fx.doc, move_cmd(fx.path, 1.0), &Selection::None)
        .expect("applies");
    h.undo(&mut fx.doc);
    assert_eq!(h.redo_len(), 1);
    assert_eq!(h.redo_label(), Some("Move nodes"));
    h.apply(&mut fx.doc, move_cmd(fx.path, 2.0), &Selection::None)
        .expect("applies");
    assert_eq!(h.redo_len(), 0);
    assert!(!h.redo(&mut fx.doc));
}

#[test]
fn one_drag_is_one_undo_step() {
    let mut fx = fixture();
    let start = fx.doc.clone();
    let mut h = History::new();
    h.begin_gesture();
    for _ in 0..10 {
        h.apply(&mut fx.doc, move_cmd(fx.path, 1.0), &Selection::None)
            .expect("applies");
    }
    h.end_gesture();
    assert_eq!(h.undo_len(), 1);
    assert_eq!(
        path_mut(&mut fx.doc, fx.path).subpaths[0].nodes[0].anchor,
        Point::new(10.0, 0.0)
    );
    let dragged = fx.doc.clone();
    h.undo(&mut fx.doc);
    assert_eq!(fx.doc, start);
    h.redo(&mut fx.doc);
    assert_eq!(fx.doc, dragged);
}

#[test]
fn gestures_do_not_merge_across_targets_or_gestures() {
    let mut fx = fixture();
    let mut h = History::new();
    h.begin_gesture();
    h.apply(&mut fx.doc, move_cmd(fx.path, 1.0), &Selection::None)
        .expect("applies");
    let other = Command::SetTransform {
        ids: vec![fx.star],
        transform: Transform::translate(1.0, 1.0),
    };
    h.apply(&mut fx.doc, other, &Selection::None)
        .expect("applies");
    h.end_gesture();
    h.begin_gesture();
    h.apply(&mut fx.doc, move_cmd(fx.path, 1.0), &Selection::None)
        .expect("applies");
    h.end_gesture();
    h.apply(&mut fx.doc, move_cmd(fx.path, 1.0), &Selection::None)
        .expect("applies");
    h.apply(&mut fx.doc, move_cmd(fx.path, 1.0), &Selection::None)
        .expect("applies");
    assert_eq!(h.undo_len(), 5);
}

#[test]
fn undo_restores_the_selection_before_the_command() {
    let mut fx = fixture();
    let mut h = History::new();
    let selected = Selection::Objects(vec![fx.star]);
    h.apply(
        &mut fx.doc,
        Command::Delete { ids: vec![fx.star] },
        &selected,
    )
    .expect("applies");
    assert_eq!(h.undo_label(), Some("Delete"));
    assert_eq!(h.undo(&mut fx.doc), Some(selected));
    assert_eq!(h.undo(&mut fx.doc), None);
}

#[test]
fn a_change_that_changes_nothing_is_not_recorded() {
    let mut fx = fixture();
    let mut h = History::new();
    let same = fx.doc.nodes[&fx.star].transform;
    h.apply(
        &mut fx.doc,
        Command::SetTransform {
            ids: vec![fx.star],
            transform: same,
        },
        &Selection::None,
    )
    .expect("applies");
    assert_eq!(h.undo_len(), 0);
}

#[test]
fn a_rejected_command_is_not_recorded() {
    let mut fx = fixture();
    let before = fx.doc.clone();
    let mut h = History::new();
    assert!(
        h.apply(
            &mut fx.doc,
            Command::Ungroup { id: fx.star },
            &Selection::None
        )
        .is_err()
    );
    assert_eq!(h.undo_len(), 0);
    assert_eq!(fx.doc, before);
}

#[test]
fn paste_and_duplicate_get_unique_ids() {
    let mut fx = fixture();
    let clipboard = ClipboardPayload {
        nodes: fx.doc.copy_subtrees(&[fx.group, fx.star]).expect("exists"),
        resources: vec![],
    };
    assert_eq!(clipboard.nodes.len(), 4); // group, rect, ellipse, star
    let mut h = History::new();
    for _ in 0..2 {
        let copies = fx.doc.fresh_copies(&clipboard.nodes);
        for c in &copies {
            assert!(!fx.doc.nodes.contains_key(&c.id), "{:?} reused", c.id);
        }
        let cmd = Command::Insert {
            parent: Some(fx.layer2),
            index: 0,
            nodes: copies,
        };
        h.apply(&mut fx.doc, cmd, &Selection::None)
            .expect("paste applies");
    }
    fx.doc.validate().expect("valid after two pastes");
    assert_eq!(fx.doc.nodes.len(), 9 + 8);
    // undo does not hand out the same ids again
    h.undo(&mut fx.doc);
    let id = fx.doc.alloc_node_id();
    assert!(id.0 >= 18, "allocator went back to {id:?}");
}
