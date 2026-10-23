//! Edit operations that build commands: copy / paste, arrange, world
//! transforms.

mod common;

use std::collections::BTreeSet;

use common::{check, children_mut, fixture, node_mut, rect_geom, shape};
use vextor_core::{
    Arrange, ClipboardPayload, Command, Document, History, NodeId, NodeKind, Paint, Point, Rect,
    Resource, Selection, Transform,
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
        let cmd = fx
            .doc
            .arrange(&ids, how, |_, _| false)
            .expect("something moves");
        check(&fx.doc, cmd, |d| {
            *children_mut(d, fx.layer) = expected.clone();
        });
    }
}

#[test]
fn arrange_at_the_edge_does_nothing() {
    let fx = fixture();
    assert_eq!(
        fx.doc.arrange(&[fx.star], Arrange::Front, |_, _| true),
        None
    );
    assert_eq!(
        fx.doc.arrange(&[fx.group], Arrange::Backward, |_, _| true),
        None
    );
    // Already on top as a block.
    assert_eq!(
        fx.doc
            .arrange(&[fx.path, fx.star], Arrange::Forward, |_, _| true),
        None
    );
}

/// Spec 08c: a layer holding A, D, B, C (bottom → top) where A, B and C
/// pile up and D lies far away; `overlap` says which pairs touch.
struct Pile {
    doc: Document,
    layer: NodeId,
    a: NodeId,
    d: NodeId,
    b: NodeId,
    c: NodeId,
}

impl Pile {
    fn new() -> Self {
        let mut doc = Document::new("pile");
        let layer = doc.layers[0];
        let [a, d, b, c] = [(); 4].map(|()| doc.alloc_node_id());
        let nodes = vec![
            shape(a, rect_geom(0.0, 0.0, 10.0, 10.0)),
            shape(d, rect_geom(500.0, 0.0, 10.0, 10.0)),
            shape(b, rect_geom(5.0, 5.0, 10.0, 10.0)),
            shape(c, rect_geom(8.0, 8.0, 10.0, 10.0)),
        ];
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes,
        })
        .expect("pile builds");
        Self {
            doc,
            layer,
            a,
            d,
            b,
            c,
        }
    }

    fn overlap(&self) -> impl Fn(NodeId, NodeId) -> bool + use<> {
        let d = self.d;
        move |x, y| x != d && y != d
    }

    /// Arranges `ids` with the pile's overlaps and checks the new order,
    /// one undo step that undoes and redoes exactly.
    fn arranges(&self, ids: &[NodeId], how: Arrange, expected: &[NodeId]) {
        let cmd = self
            .doc
            .arrange(ids, how, self.overlap())
            .expect("something moves");
        check(&self.doc, cmd, |d| {
            *children_mut(d, self.layer) = expected.to_vec();
        });
    }
}

#[test]
fn forward_and_backward_pass_objects_that_do_not_overlap() {
    let pile = Pile::new();
    let (a, d, b, c) = (pile.a, pile.d, pile.b, pile.c);
    // A skips the far-away D and lands just above B.
    pile.arranges(&[a], Arrange::Forward, &[d, b, a, c]);
    // C lands just below B.
    pile.arranges(&[c], Arrange::Backward, &[a, d, c, b]);
    // D overlaps nothing: one step, as before.
    pile.arranges(&[d], Arrange::Forward, &[a, b, d, c]);
    pile.arranges(&[d], Arrange::Backward, &[d, a, b, c]);
    // Front and back are unchanged.
    pile.arranges(&[a], Arrange::Front, &[d, b, c, a]);
    pile.arranges(&[c], Arrange::Back, &[c, a, d, b]);
}

#[test]
fn arranging_two_keeps_their_relative_order() {
    let pile = Pile::new();
    let (a, d, b, c) = (pile.a, pile.d, pile.b, pile.c);
    // B passes C; A passes D and C but stays below B.
    pile.arranges(&[a, b], Arrange::Forward, &[d, c, a, b]);
    // A is at the bottom; B overlaps nothing below it but A: one step.
    pile.arranges(&[b, a], Arrange::Backward, &[a, b, d, c]);
}

#[test]
fn clips_and_layers_never_move() {
    let mut fx = fixture();
    node_mut(&mut fx.doc, fx.group)
        .container_mut()
        .expect("group")
        .clip = Some(fx.rect);
    // Group children: [rect (clip), ellipse].
    assert_eq!(
        fx.doc.arrange(&[fx.rect], Arrange::Forward, |_, _| true),
        None
    );
    assert_eq!(
        fx.doc.arrange(&[fx.layer], Arrange::Back, |_, _| true),
        None
    );
    assert!(!fx.doc.can_arrange(&[fx.rect], Arrange::Front));
    assert!(!fx.doc.can_arrange(&[fx.layer2], Arrange::Back));
}

#[test]
fn can_arrange_knows_the_top_and_the_bottom() {
    let p = Pile::new();
    let can = |ids: &[NodeId], how| p.doc.can_arrange(ids, how);
    assert!(can(&[p.a], Arrange::Forward) && can(&[p.a], Arrange::Front));
    assert!(!can(&[p.a], Arrange::Backward) && !can(&[p.a], Arrange::Back));
    assert!(!can(&[p.c], Arrange::Forward) && !can(&[p.c], Arrange::Front));
    assert!(can(&[p.c], Arrange::Backward));
    // Already on top as a block.
    assert!(!can(&[p.b, p.c], Arrange::Forward));
    assert!(!can(&[], Arrange::Forward));
}

#[test]
fn arrange_handles_several_parents_in_one_step() {
    let fx = fixture();
    let cmd = fx
        .doc
        .arrange(&[fx.rect, fx.path], Arrange::Front, |_, _| false)
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

// ----- make pattern (spec 09) -----

#[test]
fn make_pattern_copies_the_objects_relative_to_the_tile() {
    let fx = fixture();
    let mut doc = fx.doc.clone();
    node_mut(&mut doc, fx.path).transform = Transform::translate(10.0, 20.0);
    let before = doc.clone();
    let tile = Rect::new(5.0, 5.0, 40.0, 30.0);
    let (cmd, id) = doc
        .make_pattern(&[fx.path, fx.star], tile)
        .expect("pattern made");
    let mut h = History::new();
    h.apply(&mut doc, cmd, &Selection::None).expect("applies");
    doc.validate().expect("valid");

    let Some(Resource::Pattern(p)) = doc.resources.get(&id) else {
        panic!("a pattern under {id:?}");
    };
    assert_eq!(p.tile, Rect::new(0.0, 0.0, 40.0, 30.0));
    assert_eq!(p.content.len(), 2);
    // The path copy sits where the path did, measured from the tile corner.
    let copy = &doc.nodes[&p.content[0]];
    let origin = copy.transform.apply(Point::new(0.0, 0.0));
    assert!(close(origin, Point::new(5.0, 15.0)), "{origin:?}");
    // The originals are untouched and the copies have new ids.
    assert_eq!(doc.nodes[&fx.path], before.nodes[&fx.path]);
    assert!(!p.content.contains(&fx.path) && !p.content.contains(&fx.star));

    assert!(h.undo(&mut doc).is_some());
    assert_eq!(doc, before, "undo removes the pattern and its content");
}

#[test]
fn make_pattern_rejects_nothing_layers_and_empty_tiles() {
    let fx = fixture();
    let mut doc = fx.doc.clone();
    let tile = Rect::new(0.0, 0.0, 10.0, 10.0);
    assert!(doc.make_pattern(&[], tile).is_err());
    assert!(doc.make_pattern(&[fx.layer], tile).is_err());
    let flat = Rect::new(0.0, 0.0, 10.0, 0.0);
    assert!(doc.make_pattern(&[fx.path], flat).is_err());
    assert_eq!(doc, fx.doc);
}
