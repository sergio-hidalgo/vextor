//! Edit operations that build commands: copy / paste, arrange, world
//! transforms.

mod common;

use std::collections::BTreeSet;

use common::{check, children_mut, fixture, node_mut, rect_geom, shape};
use vextor_core::{
    Arrange, ClipboardPayload, Command, Document, NodeId, NodeKind, Paint, Point, Selection,
    Transform,
};

fn close(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
}

/// The fill paint of a shape.
fn fill(doc: &Document, id: NodeId) -> Paint {
    match &doc.nodes[&id].kind {
        NodeKind::Shape(s) => s.style.fill,
        _ => panic!("{id:?} is not a shape"),
    }
}

/// Pastes and checks apply → undo → redo; returns the pasted roots.
fn paste(
    doc: &mut Document,
    payload: &ClipboardPayload,
    parent: NodeId,
    offset: Transform,
) -> Vec<NodeId> {
    let before = doc.clone();
    let (cmd, roots) = doc
        .paste(payload, parent, 0, offset, "Paste")
        .expect("paste builds");
    let mut probe = before.clone();
    probe.apply(cmd.clone()).expect("paste applies");
    let after = check(&before, cmd, |d| *d = probe.clone());
    // Keep the allocator that handed out the new ids.
    let next = doc.next_id();
    *doc = after;
    assert!(doc.next_id() >= next);
    roots
}

#[test]
fn world_transform_composes_ancestors() {
    let mut fx = fixture();
    node_mut(&mut fx.doc, fx.layer).transform = Transform::translate(100.0, 0.0);
    node_mut(&mut fx.doc, fx.group).transform = Transform::scale(2.0, 2.0);
    node_mut(&mut fx.doc, fx.rect).transform = Transform::translate(1.0, 1.0);
    let w = fx.doc.world_transform(fx.rect);
    assert!(close(w.apply(Point::new(0.0, 0.0)), Point::new(102.0, 2.0)));
    let p = fx.doc.parent_transform(fx.rect);
    assert!(close(p.apply(Point::new(1.0, 1.0)), Point::new(102.0, 2.0)));
}

#[test]
fn paint_order_is_bottom_to_top() {
    let fx = fixture();
    let order = fx.doc.paint_order();
    let pos = |id| order.iter().position(|&o| o == id).expect("listed");
    assert!(pos(fx.layer) < pos(fx.group));
    assert!(pos(fx.rect) < pos(fx.ellipse));
    assert!(pos(fx.ellipse) < pos(fx.path));
    assert!(pos(fx.star) < pos(fx.image));
    assert!(
        !order.contains(&fx.pattern_rect),
        "pattern content is not painted"
    );
    assert_eq!(
        fx.doc
            .sorted_by_paint_order(&[fx.star, fx.rect, NodeId(9_999)]),
        vec![fx.rect, fx.star]
    );
}

#[test]
fn pasted_nodes_have_fresh_ids() {
    let mut fx = fixture();
    let payload = fx.doc.copy(&[fx.group, fx.star]).expect("copies");
    let old: BTreeSet<NodeId> = fx.doc.nodes.keys().copied().collect();
    let roots = paste(&mut fx.doc, &payload, fx.layer, Transform::IDENTITY);
    assert_eq!(roots.len(), 2);
    let new: BTreeSet<NodeId> = fx.doc.nodes.keys().copied().collect();
    let added: BTreeSet<NodeId> = new.difference(&old).copied().collect();
    // group + rect + ellipse + star, all new.
    assert_eq!(added.len(), 4);
    assert!(roots.iter().all(|r| added.contains(r)));
    // The pasted group lists the pasted children, not the originals.
    let g = fx.doc.nodes[&roots[0]].container().expect("group");
    assert!(g.children.iter().all(|c| added.contains(c)));
}

#[test]
fn paste_reuses_an_identical_resource() {
    let mut fx = fixture();
    let payload = fx.doc.copy(&[fx.group]).expect("copies");
    assert_eq!(payload.resources.len(), 1, "the rect's gradient");
    let resources = fx.doc.resources.len();
    let roots = paste(&mut fx.doc, &payload, fx.layer, Transform::IDENTITY);
    assert_eq!(fx.doc.resources.len(), resources);
    let rect = fx.doc.nodes[&roots[0]].container().expect("group").children[0];
    assert_eq!(fill(&fx.doc, rect), Paint::Gradient(fx.gradient));
}

#[test]
fn paste_into_another_document_brings_resources() {
    let fx = fixture();
    // A pattern whose content paints with the gradient, used by the star.
    let mut source = fx.doc.clone();
    if let NodeKind::Shape(s) = &mut node_mut(&mut source, fx.star).kind {
        s.style.fill = Paint::Pattern(fx.pattern);
    }
    if let NodeKind::Shape(s) = &mut node_mut(&mut source, fx.pattern_rect).kind {
        s.style.fill = Paint::Gradient(fx.gradient);
    }
    source.validate().expect("valid source");
    let payload = source.copy(&[fx.star]).expect("copies");
    assert_eq!(
        payload.resources.len(),
        2,
        "pattern and, through it, gradient"
    );

    let mut target = Document::new("other");
    let layer = target.layers[0];
    let roots = paste(&mut target, &payload, layer, Transform::IDENTITY);
    target.validate().expect("valid target");
    assert_eq!(target.resources.len(), 2);
    let Paint::Pattern(p) = fill(&target, roots[0]) else {
        panic!("star keeps a pattern fill");
    };
    let content = target.resources[&p].content()[0];
    assert!(
        matches!(fill(&target, content), Paint::Gradient(g) if target.resources.contains_key(&g))
    );
}

#[test]
fn paste_adds_a_resource_whose_fresh_id_equals_the_copied_one() {
    // Both documents allocate the same first ids, so the copied gradient's
    // id is exactly the fresh id the target hands out.
    let mut source = Document::new("a");
    let gid = source.alloc_resource_id();
    source
        .apply(Command::PutResource {
            id: gid,
            resource: common::gradient(),
            nodes: vec![],
        })
        .expect("adds gradient");
    let s = source.alloc_node_id();
    let mut node = shape(s, rect_geom(0.0, 0.0, 4.0, 4.0));
    if let NodeKind::Shape(sh) = &mut node.kind {
        sh.style.fill = Paint::Gradient(gid);
    }
    let layer = source.layers[0];
    source
        .apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![node],
        })
        .expect("inserts");
    let payload = source.copy(&[s]).expect("copies");

    let mut target = Document::new("b");
    let layer = target.layers[0];
    let roots = paste(&mut target, &payload, layer, Transform::IDENTITY);
    let Paint::Gradient(g) = fill(&target, roots[0]) else {
        panic!("gradient fill");
    };
    assert!(target.resources.contains_key(&g));
}

#[test]
fn paste_keeps_the_world_position_under_another_parent() {
    let mut fx = fixture();
    node_mut(&mut fx.doc, fx.group).transform = Transform::translate(30.0, 0.0);
    let world_before = fx.doc.world_transform(fx.rect);
    let payload = fx.doc.copy(&[fx.rect]).expect("copies");
    // Into layer 2, which is moved and scaled, offset by (10, 10).
    node_mut(&mut fx.doc, fx.layer2).transform =
        Transform::scale(2.0, 2.0).then(Transform::translate(-5.0, 7.0));
    let roots = paste(
        &mut fx.doc,
        &payload,
        fx.layer2,
        Transform::translate(10.0, 10.0),
    );
    let world_after = fx.doc.world_transform(roots[0]);
    let p = Point::new(3.0, 4.0);
    let expect = world_before.apply(p);
    assert!(close(
        world_after.apply(p),
        Point::new(expect.x + 10.0, expect.y + 10.0)
    ));
}

#[test]
fn paste_rejects_an_empty_payload() {
    let mut fx = fixture();
    let empty = ClipboardPayload {
        nodes: vec![],
        resources: vec![],
    };
    assert!(
        fx.doc
            .paste(&empty, fx.layer, 0, Transform::IDENTITY, "Paste")
            .is_err()
    );
}

#[test]
fn arrange_moves_within_siblings() {
    let fx = fixture();
    // Layer children: [group, path, star].
    let cases = [
        (
            Arrange::Forward,
            vec![fx.group],
            vec![fx.path, fx.group, fx.star],
        ),
        (
            Arrange::Backward,
            vec![fx.star],
            vec![fx.group, fx.star, fx.path],
        ),
        (
            Arrange::Front,
            vec![fx.group, fx.path],
            vec![fx.star, fx.group, fx.path],
        ),
        (
            Arrange::Back,
            vec![fx.path, fx.star],
            vec![fx.path, fx.star, fx.group],
        ),
    ];
    for (how, ids, expected) in cases {
        let cmd = fx.doc.arrange(&ids, how).expect("something moves");
        check(&fx.doc, cmd, |d| {
            *children_mut(d, fx.layer) = expected.clone();
        });
    }
}

#[test]
fn arrange_at_the_edge_does_nothing() {
    let fx = fixture();
    assert_eq!(fx.doc.arrange(&[fx.star], Arrange::Front), None);
    assert_eq!(fx.doc.arrange(&[fx.group], Arrange::Backward), None);
    // Already on top as a block.
    assert_eq!(fx.doc.arrange(&[fx.path, fx.star], Arrange::Forward), None);
}

#[test]
fn arrange_handles_several_parents_in_one_step() {
    let fx = fixture();
    let cmd = fx
        .doc
        .arrange(&[fx.rect, fx.path], Arrange::Front)
        .expect("moves");
    let mut d = fx.doc.clone();
    let mut h = vextor_core::History::new();
    h.apply(&mut d, cmd, &Selection::None).expect("applies");
    assert_eq!(h.undo_len(), 1);
    assert_eq!(
        d.nodes[&fx.group].container().expect("group").children,
        vec![fx.ellipse, fx.rect]
    );
    assert_eq!(
        *children_mut(&mut d, fx.layer),
        vec![fx.group, fx.star, fx.path]
    );
}

#[test]
fn copy_of_solid_shapes_carries_no_resources() {
    let mut fx = fixture();
    let id = NodeId(fx.doc.next_id());
    fx.doc
        .apply(Command::Insert {
            parent: Some(fx.layer),
            index: 0,
            nodes: vec![shape(id, rect_geom(0.0, 0.0, 1.0, 1.0))],
        })
        .expect("inserts");
    assert_eq!(fx.doc.copy(&[id]).expect("copies").resources.len(), 0);
}
