//! Every `Command`: apply → undo → redo ends in the expected document and
//! `validate()` passes after each step (see `common::check`).

mod common;

use common::{
    Fx, check, children_mut, curve_node, fixture, group, layer, node_mut, path_mut, pattern,
    rect_geom, rejects, shape,
};
use vextor_core::{
    Artboard, Command, CoreError, Geometry, GridSettings, NodeId, NodeKind, NodeType, Paint, Path,
    PathNode, PathNodeRef, Rgba, Shape, Style, SubPath, Transform, Vec2,
};

fn at(fx: &Fx, subpath: usize, index: usize) -> PathNodeRef {
    PathNodeRef {
        shape: fx.path,
        subpath,
        index,
    }
}

fn new_id(fx: &Fx, offset: u64) -> NodeId {
    NodeId(fx.doc.next_id() + offset)
}

// ----- structure -----

#[test]
fn insert_shape_into_layer() {
    let fx = fixture();
    let id = new_id(&fx, 0);
    let node = shape(id, rect_geom(1.0, 2.0, 3.0, 4.0));
    let cmd = Command::Insert {
        parent: Some(fx.layer),
        index: 1,
        nodes: vec![node.clone()],
    };
    check(&fx.doc, cmd, |d| {
        d.nodes.insert(id, node);
        children_mut(d, fx.layer).insert(1, id);
    });
}

#[test]
fn insert_layer_with_subtree_at_top_level() {
    let fx = fixture();
    let (l, g, s) = (new_id(&fx, 0), new_id(&fx, 1), new_id(&fx, 2));
    let nodes = vec![
        layer(l, vec![g]),
        group(g, vec![s]),
        shape(s, rect_geom(0.0, 0.0, 1.0, 1.0)),
    ];
    let cmd = Command::Insert {
        parent: None,
        index: 2,
        nodes: nodes.clone(),
    };
    check(&fx.doc, cmd, |d| {
        for n in nodes {
            d.nodes.insert(n.id, n);
        }
        d.layers.push(l);
    });
}

#[test]
fn insert_rejects_misplaced_layers_and_used_ids() {
    let fx = fixture();
    let id = new_id(&fx, 0);
    rejects(
        &fx.doc,
        Command::Insert {
            parent: Some(fx.group),
            index: 0,
            nodes: vec![layer(id, vec![])],
        },
    );
    let s = shape(id, rect_geom(0.0, 0.0, 1.0, 1.0));
    rejects(
        &fx.doc,
        Command::Insert {
            parent: None,
            index: 0,
            nodes: vec![s],
        },
    );
    let dup = shape(fx.rect, rect_geom(0.0, 0.0, 1.0, 1.0));
    rejects(
        &fx.doc,
        Command::Insert {
            parent: Some(fx.layer),
            index: 0,
            nodes: vec![dup],
        },
    );
    let s = shape(id, rect_geom(0.0, 0.0, 1.0, 1.0));
    rejects(
        &fx.doc,
        Command::Insert {
            parent: Some(fx.layer),
            index: 99,
            nodes: vec![s],
        },
    );
}

#[test]
fn delete_removes_whole_subtree() {
    let fx = fixture();
    let cmd = Command::Delete {
        ids: vec![fx.group, fx.rect],
    };
    check(&fx.doc, cmd, |d| {
        children_mut(d, fx.layer).retain(|&c| c != fx.group);
        for id in [fx.group, fx.rect, fx.ellipse] {
            d.nodes.remove(&id);
        }
    });
}

#[test]
fn delete_clears_a_clip_pointing_at_it() {
    let mut fx = fixture();
    node_mut(&mut fx.doc, fx.group)
        .container_mut()
        .expect("group")
        .clip = Some(fx.ellipse);
    fx.doc.validate().expect("clip set up");
    check(
        &fx.doc,
        Command::Delete {
            ids: vec![fx.ellipse],
        },
        |d| {
            let c = node_mut(d, fx.group).container_mut().expect("group");
            c.children.retain(|&c| c != fx.ellipse);
            c.clip = None;
            d.nodes.remove(&fx.ellipse);
        },
    );
}

#[test]
fn delete_rejects_pattern_content_and_missing_ids() {
    let fx = fixture();
    rejects(
        &fx.doc,
        Command::Delete {
            ids: vec![fx.pattern_rect],
        },
    );
    rejects(
        &fx.doc,
        Command::Delete {
            ids: vec![NodeId(9999)],
        },
    );
}

#[test]
fn reparent_moves_to_another_layer() {
    let fx = fixture();
    let cmd = Command::Reparent {
        ids: vec![fx.star, fx.path],
        new_parent: Some(fx.layer2),
        index: 0,
    };
    check(&fx.doc, cmd, |d| {
        children_mut(d, fx.layer).retain(|&c| c != fx.star && c != fx.path);
        children_mut(d, fx.layer2).splice(0..0, [fx.star, fx.path]);
    });
}

#[test]
fn reparent_reorders_layers() {
    let fx = fixture();
    let cmd = Command::Reparent {
        ids: vec![fx.layer],
        new_parent: None,
        index: 1,
    };
    check(&fx.doc, cmd, |d| d.layers = vec![fx.layer2, fx.layer]);
}

#[test]
fn reparent_rejects_moving_into_itself() {
    let fx = fixture();
    rejects(
        &fx.doc,
        Command::Reparent {
            ids: vec![fx.group],
            new_parent: Some(fx.group),
            index: 0,
        },
    );
    rejects(
        &fx.doc,
        Command::Reparent {
            ids: vec![fx.layer],
            new_parent: Some(fx.layer2),
            index: 0,
        },
    );
}

#[test]
fn group_wraps_siblings_where_the_topmost_was() {
    let fx = fixture();
    let g = new_id(&fx, 0);
    // layer children: [group, path, star]; star is listed first but path is below it.
    let cmd = Command::Group {
        ids: vec![fx.star, fx.path],
    };
    check(&fx.doc, cmd, |d| {
        d.nodes.insert(g, group(g, vec![fx.path, fx.star]));
        *children_mut(d, fx.layer) = vec![fx.group, g];
    });
}

#[test]
fn group_rejects_non_siblings_and_layers() {
    let fx = fixture();
    rejects(
        &fx.doc,
        Command::Group {
            ids: vec![fx.rect, fx.path],
        },
    );
    rejects(
        &fx.doc,
        Command::Group {
            ids: vec![fx.layer],
        },
    );
    rejects(&fx.doc, Command::Group { ids: vec![] });
}

#[test]
fn ungroup_folds_group_properties_into_children() {
    let mut fx = fixture();
    let g = node_mut(&mut fx.doc, fx.group);
    g.transform = Transform::translate(10.0, 0.0);
    g.opacity = 0.5;
    g.locked = true;
    node_mut(&mut fx.doc, fx.rect).transform = Transform::scale(2.0, 2.0);
    check(&fx.doc, Command::Ungroup { id: fx.group }, |d| {
        d.nodes.remove(&fx.group);
        *children_mut(d, fx.layer) = vec![fx.rect, fx.ellipse, fx.path, fx.star];
        let r = node_mut(d, fx.rect);
        r.transform = Transform([2.0, 0.0, 0.0, 2.0, 10.0, 0.0]);
        r.opacity = 0.5;
        r.locked = true;
        let e = node_mut(d, fx.ellipse);
        e.transform = Transform::translate(10.0, 0.0);
        e.opacity = 0.5;
        e.locked = true;
    });
}

#[test]
fn ungroup_rejects_layers() {
    let fx = fixture();
    rejects(&fx.doc, Command::Ungroup { id: fx.layer });
}

// ----- properties -----

#[test]
fn set_transform() {
    let fx = fixture();
    let t = Transform([1.0, 0.0, 0.0, 1.0, 3.0, 4.0]);
    check(
        &fx.doc,
        Command::SetTransform {
            ids: vec![fx.rect, fx.group],
            transform: t,
        },
        |d| {
            node_mut(d, fx.rect).transform = t;
            node_mut(d, fx.group).transform = t;
        },
    );
}

#[test]
fn set_transforms_gives_each_node_its_own() {
    let fx = fixture();
    let a = Transform::translate(1.0, 2.0);
    let b = Transform::scale(2.0, 3.0);
    check(
        &fx.doc,
        Command::SetTransforms {
            items: vec![(fx.rect, a), (fx.star, b)],
        },
        |d| {
            node_mut(d, fx.rect).transform = a;
            node_mut(d, fx.star).transform = b;
        },
    );
}

#[test]
fn set_transforms_rejects_non_finite_and_missing() {
    let fx = fixture();
    let nan = Transform([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]);
    rejects(
        &fx.doc,
        Command::SetTransforms {
            items: vec![(fx.rect, Transform::IDENTITY), (fx.star, nan)],
        },
    );
    rejects(
        &fx.doc,
        Command::SetTransforms {
            items: vec![(new_id(&fx, 0), Transform::IDENTITY)],
        },
    );
}

#[test]
fn set_transform_rejects_non_finite() {
    let fx = fixture();
    let t = Transform([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]);
    rejects(
        &fx.doc,
        Command::SetTransform {
            ids: vec![fx.rect],
            transform: t,
        },
    );
}

#[test]
fn set_geometry_replaces_shape_geometry() {
    let fx = fixture();
    let g = rect_geom(1.0, 2.0, 30.0, 40.0);
    check(
        &fx.doc,
        Command::SetGeometry {
            items: vec![(fx.star, g.clone())],
        },
        |d| {
            if let NodeKind::Shape(s) = &mut node_mut(d, fx.star).kind {
                s.geometry = g.clone();
            }
        },
    );
}

#[test]
fn set_geometry_rejects_non_shapes_and_bad_geometry() {
    let fx = fixture();
    rejects(
        &fx.doc,
        Command::SetGeometry {
            items: vec![(fx.group, rect_geom(0.0, 0.0, 1.0, 1.0))],
        },
    );
    rejects(
        &fx.doc,
        Command::SetGeometry {
            items: vec![(fx.star, rect_geom(0.0, 0.0, -1.0, 1.0))],
        },
    );
}

#[test]
fn set_style() {
    let fx = fixture();
    let style = Style {
        fill: Paint::Solid(Rgba::rgb(1, 2, 3)),
        stroke: None,
    };
    check(
        &fx.doc,
        Command::SetStyle {
            ids: vec![fx.star],
            style: style.clone(),
        },
        |d| {
            if let NodeKind::Shape(s) = &mut node_mut(d, fx.star).kind {
                s.style = style;
            }
        },
    );
}

#[test]
fn set_style_rejects_groups_and_missing_resources() {
    let fx = fixture();
    rejects(
        &fx.doc,
        Command::SetStyle {
            ids: vec![fx.group],
            style: Style::default(),
        },
    );
    let bad = Style {
        fill: Paint::Pattern(fx.gradient),
        stroke: None,
    };
    rejects(
        &fx.doc,
        Command::SetStyle {
            ids: vec![fx.star],
            style: bad,
        },
    );
}

#[test]
fn set_props() {
    let fx = fixture();
    let cmd = Command::SetProps {
        id: fx.star,
        name: Some("star".into()),
        visible: false,
        locked: true,
        opacity: 0.25,
    };
    check(&fx.doc, cmd, |d| {
        let n = node_mut(d, fx.star);
        n.name = Some("star".into());
        n.visible = false;
        n.locked = true;
        n.opacity = 0.25;
    });
}

#[test]
fn set_props_rejects_opacity_out_of_range() {
    let fx = fixture();
    let cmd = Command::SetProps {
        id: fx.star,
        name: None,
        visible: true,
        locked: false,
        opacity: 1.5,
    };
    rejects(&fx.doc, cmd);
}

// ----- paths -----

#[test]
fn add_path_node() {
    let fx = fixture();
    let node = PathNode::corner(30.0, 0.0);
    check(
        &fx.doc,
        Command::AddPathNode {
            at: at(&fx, 0, 3),
            node,
        },
        |d| {
            path_mut(d, fx.path).subpaths[0].nodes.push(node);
        },
    );
}

#[test]
fn add_path_node_starts_a_new_subpath() {
    let fx = fixture();
    let node = PathNode::corner(5.0, 5.0);
    check(
        &fx.doc,
        Command::AddPathNode {
            at: at(&fx, 2, 0),
            node,
        },
        |d| {
            path_mut(d, fx.path).subpaths.push(SubPath {
                nodes: vec![node],
                closed: false,
            });
        },
    );
    rejects(
        &fx.doc,
        Command::AddPathNode {
            at: at(&fx, 2, 1),
            node,
        },
    );
    rejects(
        &fx.doc,
        Command::AddPathNode {
            at: PathNodeRef {
                shape: fx.star,
                subpath: 0,
                index: 0,
            },
            node,
        },
    );
}

#[test]
fn move_path_nodes_moves_anchors_once() {
    let fx = fixture();
    let delta = Vec2::new(1.0, 2.0);
    let cmd = Command::MovePathNodes {
        nodes: vec![at(&fx, 0, 1), at(&fx, 0, 1), at(&fx, 1, 0)],
        delta,
    };
    check(&fx.doc, cmd, |d| {
        let p = path_mut(d, fx.path);
        p.subpaths[0].nodes[1].anchor = vextor_core::Point::new(11.0, 2.0);
        p.subpaths[1].nodes[0].anchor = vextor_core::Point::new(1.0, 12.0);
    });
}

#[test]
fn set_handles_mirrors_a_symmetric_node() {
    let fx = fixture();
    let cmd = Command::SetHandles {
        node: at(&fx, 0, 1),
        handle_in: Some(Vec2::new(-5.0, 0.0)),
        handle_out: Some(Vec2::new(3.0, 4.0)),
    };
    check(&fx.doc, cmd, |d| {
        let n = &mut path_mut(d, fx.path).subpaths[0].nodes[1];
        n.handle_out = Some(Vec2::new(3.0, 4.0));
        n.handle_in = Some(Vec2::new(-3.0, -4.0));
    });
}

#[test]
fn set_handles_keeps_the_edited_in_handle() {
    let fx = fixture();
    let cmd = Command::SetHandles {
        node: at(&fx, 0, 1),
        handle_in: Some(Vec2::new(0.0, -2.0)),
        handle_out: Some(Vec2::new(5.0, 0.0)),
    };
    check(&fx.doc, cmd, |d| {
        let n = &mut path_mut(d, fx.path).subpaths[0].nodes[1];
        n.handle_in = Some(Vec2::new(0.0, -2.0));
        n.handle_out = Some(Vec2::new(0.0, 2.0));
    });
}

#[test]
fn set_node_type_balances_handles() {
    let mut fx = fixture();
    let n = &mut path_mut(&mut fx.doc, fx.path).subpaths[0].nodes[1];
    n.kind = NodeType::Corner;
    n.handle_in = Some(Vec2::new(0.0, 2.0));
    n.handle_out = Some(Vec2::new(4.0, 0.0));
    let after = check(
        &fx.doc,
        Command::SetNodeType {
            node: at(&fx, 0, 1),
            kind: NodeType::Symmetric,
        },
        |d| {
            let mut n = path_mut(d, fx.path).subpaths[0].nodes[1];
            n.kind = NodeType::Symmetric;
            n.enforce_balanced();
            path_mut(d, fx.path).subpaths[0].nodes[1] = n;
        },
    );
    let node = match &after.nodes[&fx.path].kind {
        NodeKind::Shape(Shape {
            geometry: Geometry::Path(p),
            ..
        }) => p.subpaths[0].nodes[1],
        _ => unreachable!(),
    };
    assert!(node.satisfies_type());
}

#[test]
fn toggle_segment_straight_to_curve_and_back() {
    let fx = fixture();
    let cmd = Command::ToggleSegment {
        shape: fx.path,
        subpath: 1,
        index: 2,
    }; // closing segment
    let curved = check(&fx.doc, cmd.clone(), |d| {
        let sp = &mut path_mut(d, fx.path).subpaths[1];
        sp.nodes[2].handle_out = Some(Vec2::new(0.0, -10.0).scale(1.0 / 3.0));
        sp.nodes[0].handle_in = Some(Vec2::new(0.0, 10.0).scale(1.0 / 3.0));
    });
    check(&curved, cmd, |d| {
        let sp = &mut path_mut(d, fx.path).subpaths[1];
        sp.nodes[2].handle_out = None;
        sp.nodes[0].handle_in = None;
    });
    rejects(
        &fx.doc,
        Command::ToggleSegment {
            shape: fx.path,
            subpath: 0,
            index: 2,
        },
    );
}

#[test]
fn delete_path_nodes_drops_empty_subpaths() {
    let fx = fixture();
    let nodes = vec![at(&fx, 1, 0), at(&fx, 1, 1), at(&fx, 1, 2), at(&fx, 0, 0)];
    check(&fx.doc, Command::DeletePathNodes { nodes }, |d| {
        let p = path_mut(d, fx.path);
        p.subpaths.remove(1);
        p.subpaths[0].nodes.remove(0);
    });
}

#[test]
fn delete_path_nodes_opens_a_closed_subpath_left_with_one_node() {
    let fx = fixture();
    let nodes = vec![at(&fx, 1, 0), at(&fx, 1, 1)];
    check(&fx.doc, Command::DeletePathNodes { nodes }, |d| {
        let sp = &mut path_mut(d, fx.path).subpaths[1];
        sp.nodes.drain(0..2);
        sp.closed = false;
    });
}

#[test]
fn delete_every_path_node_removes_the_shape() {
    let fx = fixture();
    let nodes = (0..3)
        .flat_map(|i| [at(&fx, 0, i), at(&fx, 1, i)])
        .collect();
    check(&fx.doc, Command::DeletePathNodes { nodes }, |d| {
        d.nodes.remove(&fx.path);
        children_mut(d, fx.layer).retain(|&c| c != fx.path);
    });
}

#[test]
fn close_subpath() {
    let fx = fixture();
    check(
        &fx.doc,
        Command::CloseSubPath {
            shape: fx.path,
            subpath: 0,
        },
        |d| {
            path_mut(d, fx.path).subpaths[0].closed = true;
        },
    );
    rejects(
        &fx.doc,
        Command::CloseSubPath {
            shape: fx.path,
            subpath: 1,
        },
    );
}

#[test]
fn open_at_duplicates_the_node_at_both_ends() {
    let fx = fixture();
    check(
        &fx.doc,
        Command::OpenAt {
            node: at(&fx, 1, 1),
        },
        |d| {
            let sp = &mut path_mut(d, fx.path).subpaths[1];
            sp.nodes.rotate_left(1);
            let first = sp.nodes[0];
            sp.nodes.push(first);
            sp.closed = false;
        },
    );
    rejects(
        &fx.doc,
        Command::OpenAt {
            node: at(&fx, 0, 1),
        },
    );
}

#[test]
fn join_endpoints_of_two_subpaths() {
    let mut fx = fixture();
    // make subpath 1 open: (0,10) (10,20) (0,20)
    path_mut(&mut fx.doc, fx.path).subpaths[1].closed = false;
    // join start of subpath 0 to start of subpath 1: subpath 0 is reversed
    let cmd = Command::JoinEndpoints {
        a: at(&fx, 0, 0),
        b: at(&fx, 1, 0),
    };
    check(&fx.doc, cmd, |d| {
        let p = path_mut(d, fx.path);
        let mut first = p.subpaths[0].clone();
        first.reverse();
        first.nodes.extend(p.subpaths[1].nodes.clone());
        p.subpaths = vec![first];
    });
}

#[test]
fn join_merges_coincident_endpoints() {
    let mut fx = fixture();
    let p = path_mut(&mut fx.doc, fx.path);
    p.subpaths[1] = SubPath {
        nodes: vec![PathNode::corner(20.0, 0.0), PathNode::corner(30.0, 5.0)],
        closed: false,
    };
    let cmd = Command::JoinEndpoints {
        a: at(&fx, 0, 2),
        b: at(&fx, 1, 0),
    };
    check(&fx.doc, cmd, |d| {
        let p = path_mut(d, fx.path);
        p.subpaths[0].nodes.push(PathNode::corner(30.0, 5.0));
        p.subpaths.remove(1);
    });
}

#[test]
fn join_both_ends_of_one_subpath_closes_it() {
    let fx = fixture();
    check(
        &fx.doc,
        Command::JoinEndpoints {
            a: at(&fx, 0, 2),
            b: at(&fx, 0, 0),
        },
        |d| {
            path_mut(d, fx.path).subpaths[0].closed = true;
        },
    );
    rejects(
        &fx.doc,
        Command::JoinEndpoints {
            a: at(&fx, 0, 1),
            b: at(&fx, 0, 0),
        },
    );
    rejects(
        &fx.doc,
        Command::JoinEndpoints {
            a: at(&fx, 0, 0),
            b: PathNodeRef {
                shape: fx.star,
                subpath: 0,
                index: 0,
            },
        },
    );
}

#[test]
fn split_at_an_interior_node() {
    let fx = fixture();
    check(
        &fx.doc,
        Command::SplitAt {
            node: at(&fx, 0, 1),
        },
        |d| {
            let p = path_mut(d, fx.path);
            let nodes = p.subpaths[0].nodes.clone();
            let mut end = nodes[1];
            end.handle_out = None;
            let mut start = nodes[1];
            start.handle_in = None;
            p.subpaths[0].nodes = vec![nodes[0], end];
            p.subpaths.insert(
                1,
                SubPath {
                    nodes: vec![start, nodes[2]],
                    closed: false,
                },
            );
        },
    );
    rejects(
        &fx.doc,
        Command::SplitAt {
            node: at(&fx, 0, 0),
        },
    );
    rejects(
        &fx.doc,
        Command::SplitAt {
            node: at(&fx, 1, 1),
        },
    );
}

#[test]
fn reverse_swaps_order_and_handles() {
    let mut fx = fixture();
    path_mut(&mut fx.doc, fx.path).subpaths[0].nodes[0] = curve_node(0.0, 0.0, NodeType::Corner);
    check(
        &fx.doc,
        Command::Reverse {
            shape: fx.path,
            subpath: 0,
        },
        |d| {
            let sp = &mut path_mut(d, fx.path).subpaths[0];
            let mut nodes: Vec<PathNode> = sp.nodes.iter().rev().map(|n| n.reversed()).collect();
            std::mem::swap(&mut sp.nodes, &mut nodes);
        },
    );
}

#[test]
fn convert_to_path_replaces_geometry() {
    let fx = fixture();
    let p = Path {
        subpaths: vec![SubPath {
            nodes: vec![PathNode::corner(0.0, 0.0), PathNode::corner(1.0, 1.0)],
            closed: false,
        }],
        fill_rule: vextor_core::FillRule::EvenOdd,
    };
    check(
        &fx.doc,
        Command::ConvertToPath {
            items: vec![(fx.star, p.clone())],
        },
        |d| {
            if let NodeKind::Shape(s) = &mut node_mut(d, fx.star).kind {
                s.geometry = Geometry::Path(p);
            }
        },
    );
    let empty = Path {
        subpaths: vec![],
        fill_rule: vextor_core::FillRule::NonZero,
    };
    rejects(
        &fx.doc,
        Command::ConvertToPath {
            items: vec![(fx.star, empty)],
        },
    );
}

#[test]
fn replace_with_swaps_nodes_in_one_step() {
    let fx = fixture();
    let id = new_id(&fx, 0);
    let result = shape(id, rect_geom(0.0, 0.0, 50.0, 50.0));
    let cmd = Command::ReplaceWith {
        removed: vec![fx.path, fx.star],
        inserted: vec![result.clone()],
        parent: Some(fx.layer),
        index: 1,
    };
    check(&fx.doc, cmd, |d| {
        d.nodes.remove(&fx.path);
        d.nodes.remove(&fx.star);
        d.nodes.insert(id, result);
        *children_mut(d, fx.layer) = vec![fx.group, id];
    });
}

// ----- resources -----

#[test]
fn put_resource_adds_a_pattern_with_content() {
    let fx = fixture();
    let rid = vextor_core::ResourceId(fx.doc.next_id());
    let content = shape(new_id(&fx, 1), rect_geom(0.0, 0.0, 1.0, 1.0));
    let cmd = Command::PutResource {
        id: rid,
        resource: pattern(vec![content.id]),
        nodes: vec![content.clone()],
    };
    check(&fx.doc, cmd, |d| {
        d.resources.insert(rid, pattern(vec![content.id]));
        d.nodes.insert(content.id, content);
    });
}

#[test]
fn put_resource_replacing_a_pattern_drops_old_content() {
    let fx = fixture();
    check(
        &fx.doc,
        Command::PutResource {
            id: fx.pattern,
            resource: pattern(vec![]),
            nodes: vec![],
        },
        |d| {
            d.resources.insert(fx.pattern, pattern(vec![]));
            d.nodes.remove(&fx.pattern_rect);
        },
    );
}

#[test]
fn remove_resource_removes_pattern_content() {
    let fx = fixture();
    check(&fx.doc, Command::RemoveResource { id: fx.pattern }, |d| {
        d.resources.remove(&fx.pattern);
        d.nodes.remove(&fx.pattern_rect);
    });
}

#[test]
fn remove_resource_rejects_a_resource_in_use() {
    let fx = fixture();
    let mut d = fx.doc.clone();
    assert_eq!(
        d.apply(Command::RemoveResource { id: fx.gradient }),
        Err(CoreError::ResourceInUse(fx.gradient))
    );
    assert_eq!(d, fx.doc);
    rejects(
        &fx.doc,
        Command::PutResource {
            id: fx.gradient,
            resource: pattern(vec![]),
            nodes: vec![],
        },
    );
}

// ----- document -----

#[test]
fn set_artboards() {
    let fx = fixture();
    let a = Artboard {
        id: vextor_core::ArtboardId(fx.doc.next_id()),
        name: "icon-24".into(),
        rect: vextor_core::Rect::new(0.0, 0.0, 24.0, 24.0),
        background: None,
    };
    check(&fx.doc, Command::SetArtboards(vec![a.clone()]), |d| {
        d.artboards = vec![a];
    });
}

#[test]
fn set_color_format() {
    let fx = fixture();
    let cmd = Command::SetColorFormat(vextor_core::ColorFormat::Rgba);
    check(&fx.doc, cmd, |d| {
        d.color_format = vextor_core::ColorFormat::Rgba;
    });
}

#[test]
fn set_grid() {
    let fx = fixture();
    let grid = GridSettings {
        size: 4.0,
        visible: true,
        ..GridSettings::default()
    };
    check(&fx.doc, Command::SetGrid(grid.clone()), |d| d.grid = grid);
    rejects(
        &fx.doc,
        Command::SetGrid(GridSettings {
            size: 0.0,
            ..GridSettings::default()
        }),
    );
}

fn insert_shape(fx: &Fx, geometry: Geometry) -> Command {
    let node = shape(new_id(fx, 0), geometry);
    Command::Insert {
        parent: Some(fx.layer),
        index: 0,
        nodes: vec![node],
    }
}

fn star(points: u32) -> Geometry {
    let center = vextor_core::Point::new(0.0, 0.0);
    Geometry::Star {
        center,
        outer: 1.0,
        inner: 0.5,
        points,
        corner_radius: 0.0,
    }
}

fn polygon(sides: u32) -> Geometry {
    let center = vextor_core::Point::new(0.0, 0.0);
    Geometry::Polygon {
        center,
        radius: 1.0,
        sides,
        corner_radius: 0.0,
    }
}

#[test]
fn star_points_are_3_to_20() {
    let fx = fixture();
    for ok in [3, vextor_core::MAX_STAR_POINTS] {
        let mut d = fx.doc.clone();
        d.apply(insert_shape(&fx, star(ok)))
            .expect("allowed point count");
    }
    for bad in [2, 21, u32::MAX] {
        rejects(&fx.doc, insert_shape(&fx, star(bad)));
    }
}

#[test]
fn polygon_sides_are_3_to_20() {
    let fx = fixture();
    for ok in [3, vextor_core::MAX_POLYGON_SIDES] {
        let mut d = fx.doc.clone();
        d.apply(insert_shape(&fx, polygon(ok)))
            .expect("allowed side count");
    }
    for bad in [2, 21, u32::MAX] {
        rejects(&fx.doc, insert_shape(&fx, polygon(bad)));
    }
}

#[test]
fn invalid_error_message_survives_an_empty_list() {
    assert!(
        CoreError::Invalid(vec![])
            .to_string()
            .contains("0 document invariant")
    );
}

// ----- composite -----

#[test]
fn batch_is_one_step() {
    let fx = fixture();
    let t = Transform::translate(5.0, 0.0);
    let cmd = Command::Batch {
        label: "Tidy",
        commands: vec![
            Command::SetTransform {
                ids: vec![fx.star],
                transform: t,
            },
            Command::Delete { ids: vec![fx.path] },
        ],
    };
    assert_eq!(cmd.label(), "Tidy");
    check(&fx.doc, cmd, |d| {
        node_mut(d, fx.star).transform = t;
        d.nodes.remove(&fx.path);
        children_mut(d, fx.layer).retain(|&c| c != fx.path);
    });
}

#[test]
fn batch_applies_nothing_when_a_step_fails() {
    let fx = fixture();
    rejects(
        &fx.doc,
        Command::Batch {
            label: "Tidy",
            commands: vec![
                Command::Delete { ids: vec![fx.path] },
                Command::Delete {
                    ids: vec![new_id(&fx, 0)],
                },
            ],
        },
    );
}

#[test]
fn reparent_within_a_container_keeps_its_clip() {
    let mut fx = fixture();
    node_mut(&mut fx.doc, fx.group)
        .container_mut()
        .expect("group")
        .clip = Some(fx.rect);
    check(
        &fx.doc,
        Command::Reparent {
            ids: vec![fx.ellipse, fx.rect],
            new_parent: Some(fx.group),
            index: 0,
        },
        |d| *children_mut(d, fx.group) = vec![fx.ellipse, fx.rect],
    );
}
