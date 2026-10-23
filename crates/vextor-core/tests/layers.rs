//! Layers panel edits (spec 10): every row action is a command that undoes
//! and redoes, and names follow the SVG id rules.

mod common;

use common::{check, fixture, node_mut, rejects};
use vextor_core::{
    Command, Document, History, MAX_NODE_NAME_CHARS, Node, NodeId, NodeKind, Point, Selection,
    Transform,
};

/// `SetProps` for `id` with its current values, changed by `edit`.
fn props(doc: &Document, id: NodeId, edit: impl FnOnce(&mut Node)) -> Command {
    let mut n = doc.nodes[&id].clone();
    edit(&mut n);
    Command::SetProps {
        id,
        name: n.name,
        visible: n.visible,
        locked: n.locked,
        opacity: n.opacity,
    }
}

/// Applies `cmd` (built on `doc`) and checks undo and redo bring back
/// exactly the documents before and after.
fn round_trip(doc: &Document, cmd: Command) -> Document {
    let mut after = doc.clone();
    after.apply(cmd.clone()).expect("command applies");
    check(doc, cmd, |d| *d = after.clone())
}

fn close(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
}

// ----- rename, visibility, lock, opacity -----

#[test]
fn rename_hide_lock_and_fade_a_layer_undo() {
    let fx = fixture();
    let edits: [fn(&mut Node); 4] = [
        |n| n.name = Some("Icons".into()),
        |n| n.visible = false,
        |n| n.locked = true,
        |n| n.opacity = 0.4,
    ];
    for edit in edits {
        check(&fx.doc, props(&fx.doc, fx.layer2, edit), |d| {
            edit(node_mut(d, fx.layer2));
        });
    }
}

#[test]
fn layer_opacity_leaves_other_layers_alone() {
    let mut fx = fixture();
    node_mut(&mut fx.doc, fx.layer).opacity = 0.5;
    let cmd = props(&fx.doc, fx.layer2, |n| n.opacity = 0.2);
    let after = round_trip(&fx.doc, cmd);
    assert!((after.nodes[&fx.layer].opacity - 0.5).abs() < 1e-12);
    assert!((after.nodes[&fx.layer2].opacity - 0.2).abs() < 1e-12);
}

#[test]
fn names_must_suit_an_svg_id() {
    let fx = fixture();
    let named = |name: &str| props(&fx.doc, fx.star, |n| n.name = Some(name.into()));
    rejects(&fx.doc, named(""));
    rejects(&fx.doc, named("   "));
    rejects(&fx.doc, named("a\nb"));
    rejects(&fx.doc, named(&"x".repeat(MAX_NODE_NAME_CHARS + 1)));
    // Shown as typed: spaces, capitals and punctuation are fine.
    round_trip(&fx.doc, named("Primary Button!"));
    round_trip(&fx.doc, named(&"é".repeat(MAX_NODE_NAME_CHARS)));
}

// ----- reorder and reparent -----

#[test]
fn reorder_layers_undoes() {
    let fx = fixture();
    let cmd = fx.doc.move_to(&[fx.layer2], None, 0).expect("builds");
    let after = round_trip(&fx.doc, cmd);
    assert_eq!(after.layers, vec![fx.layer2, fx.layer]);
}

#[test]
fn reorder_within_a_layer_counts_the_drop_point_before_the_move() {
    let fx = fixture();
    // Layer 1 is [group, path, star]: drop the group just under the star.
    let cmd = fx
        .doc
        .move_to(&[fx.group], Some(fx.layer), 2)
        .expect("builds");
    let after = round_trip(&fx.doc, cmd);
    assert_eq!(
        after.nodes[&fx.layer].container().expect("layer").children,
        vec![fx.path, fx.group, fx.star]
    );
}

#[test]
fn reparent_into_another_layer_keeps_the_object_in_place() {
    let mut fx = fixture();
    node_mut(&mut fx.doc, fx.layer).transform = Transform::translate(5.0, 0.0);
    node_mut(&mut fx.doc, fx.group).transform = Transform::scale(2.0, 2.0);
    node_mut(&mut fx.doc, fx.layer2).transform = Transform::translate(0.0, 7.0);
    let world = fx.doc.world_transform(fx.rect);
    let cmd = fx
        .doc
        .move_to(&[fx.rect], Some(fx.layer2), 2)
        .expect("builds");
    let after = round_trip(&fx.doc, cmd);
    assert_eq!(
        after.nodes[&fx.layer2].container().expect("layer").children,
        vec![fx.image, fx.text, fx.rect]
    );
    let p = Point::new(3.0, 4.0);
    assert!(close(
        after.world_transform(fx.rect).apply(p),
        world.apply(p)
    ));
}

#[test]
fn dropping_on_a_group_moves_into_it() {
    let fx = fixture();
    let cmd = fx
        .doc
        .move_to(&[fx.star], Some(fx.group), 2)
        .expect("builds");
    let after = round_trip(&fx.doc, cmd);
    assert_eq!(
        after.nodes[&fx.group].container().expect("group").children,
        vec![fx.rect, fx.ellipse, fx.star]
    );
}

#[test]
fn layers_stay_at_the_top_and_nothing_moves_into_itself() {
    let fx = fixture();
    let into_group = fx
        .doc
        .move_to(&[fx.layer2], Some(fx.group), 0)
        .expect("builds");
    rejects(&fx.doc, into_group);
    let to_top = fx.doc.move_to(&[fx.star], None, 0).expect("builds");
    rejects(&fx.doc, to_top);
    let into_itself = fx
        .doc
        .move_to(&[fx.group], Some(fx.group), 0)
        .expect("builds");
    rejects(&fx.doc, into_itself);
}

// ----- new, duplicate, delete -----

#[test]
fn new_layer_goes_above_and_takes_a_free_number() {
    let fx = fixture();
    let mut doc = fx.doc.clone();
    let (cmd, id) = doc.new_layer(Some(fx.layer));
    let after = round_trip(&doc, cmd);
    assert_eq!(after.layers, vec![fx.layer, id, fx.layer2]);
    assert_eq!(after.nodes[&id].name.as_deref(), Some("Layer 3"));
}

#[test]
fn duplicate_layer_copies_its_contents_with_fresh_ids() {
    let fx = fixture();
    let mut doc = fx.doc.clone();
    let (cmd, id) = doc.duplicate_layer(fx.layer).expect("builds");
    let after = round_trip(&doc, cmd);
    assert_eq!(after.layers, vec![fx.layer, id, fx.layer2]);
    assert_eq!(after.nodes[&id].name.as_deref(), Some("Layer 1 copy"));
    let copy = after.subtree(id);
    assert_eq!(copy.len(), after.subtree(fx.layer).len());
    assert!(copy.iter().all(|c| !fx.doc.nodes.contains_key(c)));
}

#[test]
fn delete_layer_takes_its_contents_and_undoes() {
    let fx = fixture();
    let cmd = fx.doc.delete_layer(fx.layer2).expect("builds");
    let after = round_trip(&fx.doc, cmd);
    assert_eq!(after.layers, vec![fx.layer]);
    assert!(!after.nodes.contains_key(&fx.text));
}

#[test]
fn the_last_layer_cannot_be_deleted() {
    let doc = Document::new("one");
    assert!(doc.delete_layer(doc.layers[0]).is_err());
    assert!(fixture().doc.delete_layer(fixture().group).is_err());
}

// ----- clipping -----

#[test]
fn make_clip_groups_and_clips_with_the_top_shape() {
    let fx = fixture();
    let cmd = Command::MakeClip {
        ids: vec![fx.path, fx.star],
    };
    let gid = NodeId(fx.doc.next_id());
    let after = round_trip(&fx.doc, cmd);
    let NodeKind::Group(c) = &after.nodes[&gid].kind else {
        panic!("a group was made");
    };
    assert_eq!(c.children, vec![fx.path, fx.star]);
    assert_eq!(c.clip, Some(fx.star));
}

#[test]
fn make_clip_needs_a_shape_on_top() {
    let fx = fixture();
    rejects(
        &fx.doc,
        Command::MakeClip {
            ids: vec![fx.image, fx.text],
        },
    );
}

#[test]
fn release_clip_draws_the_shape_again_and_undoes() {
    let fx = fixture();
    let mut doc = fx.doc.clone();
    let gid = NodeId(doc.next_id());
    History::new()
        .apply(
            &mut doc,
            Command::MakeClip {
                ids: vec![fx.path, fx.star],
            },
            &Selection::None,
        )
        .expect("clips");
    let after = check(&doc, Command::ReleaseClip { id: gid }, |d| {
        if let NodeKind::Group(c) = &mut node_mut(d, gid).kind {
            c.clip = None;
        }
    });
    assert_eq!(
        after.nodes[&gid].container().expect("group").children,
        vec![fx.path, fx.star]
    );
    rejects(&after, Command::ReleaseClip { id: gid });
}

#[test]
fn clean_name_makes_any_text_a_valid_name() {
    use vextor_core::clean_name;
    assert_eq!(
        clean_name("  Card\theader \n").as_deref(),
        Some("Card header")
    );
    assert_eq!(clean_name("\t\r\n "), None);
    assert_eq!(
        clean_name(&"é".repeat(MAX_NODE_NAME_CHARS + 5)).map(|n| n.chars().count()),
        Some(MAX_NODE_NAME_CHARS)
    );
}
