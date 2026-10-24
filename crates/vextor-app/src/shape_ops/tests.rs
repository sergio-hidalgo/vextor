use std::sync::Arc;

use vextor_core::{
    Command, Container, CornerRadii, FillRule, Geometry, Image, ImageMime, ImageSource, Node,
    NodeId, NodeKind, Paint, Path, PathNode, Rect, Rgba, Shape, Stroke, Style, SubPath, Text,
    Transform,
};
use vextor_geom::{FontDb, Fonts, bounds};

use super::*;
use crate::new_doc::new_document;

const REGULAR: &[u8] =
    include_bytes!("../../../vextor-geom/tests/fixtures/fonts/NotoSans-Regular.ttf");

fn solid(r: u8, g: u8, b: u8) -> Style {
    Style {
        fill: Paint::Solid(Rgba::rgb(r, g, b)),
        stroke: None,
    }
}

fn rect_node(id: NodeId, x: f64, y: f64, w: f64, h: f64, style: Style) -> Node {
    Node::new(
        id,
        NodeKind::Shape(Shape {
            geometry: Geometry::Rect {
                rect: Rect::new(x, y, w, h),
                radius: CornerRadii::default(),
            },
            style,
        }),
    )
}

/// A session whose layer holds `nodes` (bottom → top), all selected.
fn session(make: impl FnOnce(&mut dyn FnMut() -> NodeId) -> Vec<Node>) -> (Session, Vec<NodeId>) {
    let mut doc = new_document("a", 400.0, 400.0);
    let layer = doc.layers[0];
    let nodes = make(&mut || doc.alloc_node_id());
    let roots: Vec<NodeId> = {
        let children: std::collections::BTreeSet<NodeId> = nodes
            .iter()
            .filter_map(Node::container)
            .flat_map(|c| c.children.iter().copied())
            .collect();
        nodes
            .iter()
            .map(|n| n.id)
            .filter(|id| !children.contains(id))
            .collect()
    };
    doc.apply(Command::Insert {
        parent: Some(layer),
        index: 0,
        nodes,
    })
    .expect("inserts");
    let mut s = Session::new(doc);
    s.fonts = Arc::new(Fonts::new(Arc::new(FontDb::from_data([REGULAR.to_vec()]))));
    s.select(roots.clone());
    (s, roots)
}

fn two_rects() -> (Session, Vec<NodeId>) {
    session(|id| {
        vec![
            rect_node(id(), 0.0, 0.0, 60.0, 60.0, solid(255, 0, 0)),
            rect_node(id(), 40.0, 40.0, 60.0, 60.0, solid(0, 0, 255)),
        ]
    })
}

fn layer_children(s: &Session) -> Vec<NodeId> {
    let layer = s.doc.layers[0];
    s.doc.nodes[&layer]
        .container()
        .expect("layer")
        .children
        .clone()
}

fn shape_of(s: &Session, id: NodeId) -> &Shape {
    match &s.doc.nodes[&id].kind {
        NodeKind::Shape(shape) => shape,
        other => panic!("not a shape: {other:?}"),
    }
}

fn path_of(s: &Session, id: NodeId) -> &Path {
    match &shape_of(s, id).geometry {
        Geometry::Path(p) => p,
        other => panic!("not a path: {other:?}"),
    }
}

fn world_bounds(s: &Session, id: NodeId) -> kurbo::Rect {
    bounds(&s.doc, &s.fonts, id).expect("bounds")
}

fn assert_rect(got: kurbo::Rect, want: (f64, f64, f64, f64)) {
    let want = kurbo::Rect::new(want.0, want.1, want.2, want.3);
    let d = [
        got.x0 - want.x0,
        got.y0 - want.y0,
        got.x1 - want.x1,
        got.y1 - want.y1,
    ];
    assert!(d.iter().all(|v| v.abs() < 0.05), "{got:?} vs {want:?}");
}

#[test]
fn union_replaces_the_objects_with_one_path_in_one_undo_step() {
    let (mut s, ids) = two_rects();
    ShapeOp::Union.run(&mut s).expect("unites");
    let [id] = s.selected()[..] else {
        panic!("the result is selected");
    };
    assert_eq!(layer_children(&s), [id]);
    assert_eq!(shape_of(&s, id).style, solid(255, 0, 0), "the bottom style");
    let p = path_of(&s, id);
    assert_eq!(p.subpaths.len(), 1);
    assert_eq!(
        p.subpaths[0].nodes.len(),
        8,
        "an L-shaped outline of 8 corners"
    );
    assert_rect(world_bounds(&s, id), (0.0, 0.0, 100.0, 100.0));
    assert_eq!(s.edit_menu().undo, Some("Union"));
    s.undo();
    assert_eq!(layer_children(&s), ids, "one step back to both rects");
}

#[test]
fn subtract_cuts_the_upper_objects_out_of_the_bottom_one() {
    let (mut s, _) = session(|id| {
        vec![
            rect_node(id(), 0.0, 0.0, 100.0, 100.0, solid(10, 20, 30)),
            rect_node(id(), 25.0, 25.0, 50.0, 50.0, solid(0, 0, 0)),
        ]
    });
    ShapeOp::Subtract.run(&mut s).expect("subtracts");
    let id = s.selected()[0];
    let p = path_of(&s, id);
    assert_eq!(p.subpaths.len(), 2, "an outline with a hole");
    assert_eq!(shape_of(&s, id).style, solid(10, 20, 30));
}

#[test]
fn an_empty_result_changes_nothing() {
    let (mut s, ids) = session(|id| {
        vec![
            rect_node(id(), 0.0, 0.0, 10.0, 10.0, solid(0, 0, 0)),
            rect_node(id(), 50.0, 50.0, 10.0, 10.0, solid(0, 0, 0)),
        ]
    });
    let e = ShapeOp::Intersect
        .run(&mut s)
        .expect_err("nothing in common");
    assert!(e.contains("empty"), "{e}");
    assert_eq!(layer_children(&s), ids);
    assert_eq!(s.edit_menu().undo, None);
}

#[test]
fn combining_needs_two_objects_and_no_images() {
    let (mut s, ids) = two_rects();
    s.select(vec![ids[0]]);
    assert!(!ShapeOp::Union.can(&s));
    assert!(ShapeOp::Union.run(&mut s).is_err());
    let (s, _) = session(|id| {
        vec![
            rect_node(id(), 0.0, 0.0, 10.0, 10.0, solid(0, 0, 0)),
            Node::new(
                id(),
                NodeKind::Image(Image {
                    rect: Rect::new(0.0, 0.0, 1.0, 1.0),
                    source: ImageSource::Embedded {
                        mime: ImageMime::Png,
                        bytes: vec![1, 2, 3],
                    },
                }),
            ),
        ]
    });
    for op in ShapeOp::COMBINE {
        assert!(!op.can(&s), "{op:?} with an image");
    }
}

#[test]
fn transformed_objects_and_groups_combine_where_they_are_drawn() {
    let (mut s, _) = session(|id| {
        let (inner, group) = (id(), id());
        let mut moved = rect_node(id(), 0.0, 0.0, 20.0, 20.0, solid(0, 0, 0));
        moved.transform = Transform([1.0, 0.0, 0.0, 1.0, 100.0, 0.0]);
        let mut g = Node::new(
            group,
            NodeKind::Group(Container {
                children: vec![inner],
                clip: None,
            }),
        );
        g.transform = Transform([1.0, 0.0, 0.0, 1.0, 0.0, 50.0]);
        vec![
            moved,
            g,
            rect_node(inner, 10.0, 0.0, 20.0, 20.0, solid(0, 0, 0)),
        ]
    });
    let before: Vec<kurbo::Rect> = s
        .selected()
        .iter()
        .map(|&id| world_bounds(&s, id))
        .collect();
    ShapeOp::Union.run(&mut s).expect("unites");
    let id = s.selected()[0];
    let want = before[0].union(before[1]);
    assert_rect(world_bounds(&s, id), (want.x0, want.y0, want.x1, want.y1));
    assert_eq!(
        s.doc.nodes[&id].transform,
        Transform([1.0, 0.0, 0.0, 1.0, 100.0, 0.0]),
        "the bottom object's transform"
    );
    assert_eq!(path_of(&s, id).subpaths.len(), 2, "two separate squares");
}

#[test]
fn a_boolean_with_text_combines_its_outlines_in_the_same_step() {
    let (mut s, ids) = session(|id| {
        let mut t = Node::new(
            id(),
            NodeKind::Text(Text {
                style: solid(0, 0, 0),
                ..Text::new("Hi", "Noto Sans")
            }),
        );
        t.transform = Transform([1.0, 0.0, 0.0, 1.0, 10.0, 10.0]);
        vec![
            rect_node(id(), 0.0, 0.0, 200.0, 100.0, solid(255, 255, 0)),
            t,
        ]
    });
    assert!(ShapeOp::Subtract.needs_outlines(&s));
    assert!(!ShapeOp::Offset.needs_outlines(&s));
    ShapeOp::Subtract.run(&mut s).expect("cuts the letters out");
    let id = s.selected()[0];
    assert_eq!(layer_children(&s), [id]);
    let p = path_of(&s, id);
    assert!(
        p.subpaths.len() >= 3,
        "the card and letter holes: {}",
        p.subpaths.len()
    );
    s.undo();
    assert_eq!(layer_children(&s), ids, "one step back to the text");
}

#[test]
fn divide_makes_a_group_of_pieces_painted_by_the_top_object() {
    let (mut s, ids) = two_rects();
    ShapeOp::Divide.run(&mut s).expect("divides");
    let [g] = s.selected()[..] else {
        panic!("the group is selected");
    };
    let pieces = s.doc.nodes[&g]
        .container()
        .expect("a group")
        .children
        .clone();
    assert_eq!(pieces.len(), 3);
    let fills: Vec<Paint> = pieces.iter().map(|&p| shape_of(&s, p).style.fill).collect();
    let blue = fills
        .iter()
        .filter(|f| **f == Paint::Solid(Rgba::rgb(0, 0, 255)))
        .count();
    assert_eq!(
        blue, 2,
        "the overlap and the top rect's own piece: {fills:?}"
    );
    s.undo();
    assert_eq!(layer_children(&s), ids);
}

#[test]
fn offset_grows_and_insets_each_shape_as_a_path() {
    let (mut s, ids) = two_rects();
    s.select(vec![ids[0]]);
    offset_selection(
        &mut s,
        OffsetSettings {
            distance: 10.0,
            join: vextor_core::LineJoin::Miter,
            miter_limit: 4.0,
        },
    )
    .expect("grows");
    assert_rect(world_bounds(&s, ids[0]), (-10.0, -10.0, 70.0, 70.0));
    assert_eq!(s.edit_menu().undo, Some(OFFSET));
    s.undo();
    let e = offset_selection(
        &mut s,
        OffsetSettings {
            distance: -40.0,
            ..OffsetSettings::default()
        },
    )
    .expect_err("an inset past the middle");
    assert!(e.contains("nothing"), "{e}");
    assert!(matches!(
        shape_of(&s, ids[0]).geometry,
        Geometry::Rect { .. }
    ));
}

/// A circle of `n` cubic nodes: far more than it needs.
fn over_noded(n: u32) -> Path {
    let step = std::f64::consts::TAU / f64::from(n);
    let k = 4.0 / 3.0 * (step / 4.0).tan() * 40.0;
    let nodes = (0..n)
        .map(|i| {
            let a = f64::from(i) * step;
            let (c, sn) = (a.cos(), a.sin());
            PathNode {
                handle_in: Some(vextor_core::Vec2::new(sn * k, -c * k)),
                handle_out: Some(vextor_core::Vec2::new(-sn * k, c * k)),
                ..PathNode::corner(50.0 + 40.0 * c, 50.0 + 40.0 * sn)
            }
        })
        .collect();
    Path {
        subpaths: vec![SubPath {
            nodes,
            closed: true,
        }],
        fill_rule: FillRule::NonZero,
    }
}

#[test]
fn simplify_reports_and_keeps_fewer_nodes() {
    let (mut s, ids) = session(|id| {
        vec![Node::new(
            id(),
            NodeKind::Shape(Shape {
                geometry: Geometry::Path(over_noded(48)),
                style: solid(0, 0, 0),
            }),
        )]
    });
    assert!(ShapeOp::Simplify.can(&s));
    let (before, after) = simplify_selection(&mut s, 0.25).expect("simplifies");
    assert_eq!(before, 48);
    assert!(after <= 8, "{after}");
    assert_eq!(path_of(&s, ids[0]).subpaths[0].nodes.len(), after);
    assert_rect(world_bounds(&s, ids[0]), (10.0, 10.0, 90.0, 90.0));
    // A rect is no path: Convert to Path first.
    let (s, _) = two_rects();
    assert!(!ShapeOp::Simplify.can(&s));
}

#[test]
fn outline_stroke_turns_a_line_into_a_filled_path() {
    let stroke = Stroke {
        width: 4.0,
        paint: Paint::Solid(Rgba::rgb(0, 128, 0)),
        ..Stroke::default()
    };
    let (mut s, _) = session(|id| {
        vec![Node::new(
            id(),
            NodeKind::Shape(Shape {
                geometry: Geometry::Line {
                    from: vextor_core::Point::new(0.0, 0.0),
                    to: vextor_core::Point::new(100.0, 0.0),
                },
                style: Style {
                    fill: Paint::None,
                    stroke: Some(stroke.clone()),
                },
            }),
        )]
    });
    ShapeOp::OutlineStroke.run(&mut s).expect("outlines");
    let id = s.selected()[0];
    let shape = shape_of(&s, id);
    assert_eq!(shape.style.fill, Paint::Solid(Rgba::rgb(0, 128, 0)));
    assert_eq!(shape.style.stroke, None);
    assert_rect(world_bounds(&s, id), (0.0, -2.0, 100.0, 2.0));
}

#[test]
fn outline_stroke_of_a_filled_shape_keeps_the_fill_below() {
    let (mut s, _) = session(|id| {
        let mut style = solid(255, 0, 0);
        style.stroke = Some(Stroke {
            width: 2.0,
            ..Stroke::default()
        });
        vec![rect_node(id(), 0.0, 0.0, 50.0, 50.0, style)]
    });
    ShapeOp::OutlineStroke.run(&mut s).expect("outlines");
    let g = s.selected()[0];
    let children = s.doc.nodes[&g]
        .container()
        .expect("a group")
        .children
        .clone();
    let names: Vec<_> = children
        .iter()
        .map(|c| s.doc.nodes[c].name.clone())
        .collect();
    assert_eq!(names, [Some("Fill".to_owned()), Some("Stroke".to_owned())]);
    assert_eq!(shape_of(&s, children[0]).style, solid(255, 0, 0));
    assert_eq!(path_of(&s, children[1]).subpaths.len(), 2, "a frame");
}

#[test]
fn make_and_release_compound_path() {
    let (mut s, ids) = session(|id| {
        vec![
            rect_node(id(), 0.0, 0.0, 100.0, 100.0, solid(1, 2, 3)),
            rect_node(id(), 25.0, 25.0, 50.0, 50.0, solid(9, 9, 9)),
        ]
    });
    assert!(!ShapeOp::ReleaseCompound.can(&s));
    ShapeOp::MakeCompound.run(&mut s).expect("makes");
    let id = s.selected()[0];
    let p = path_of(&s, id);
    assert_eq!((p.subpaths.len(), p.fill_rule), (2, FillRule::EvenOdd));
    assert_eq!(shape_of(&s, id).style, solid(1, 2, 3));
    assert!(ShapeOp::ReleaseCompound.can(&s));
    ShapeOp::ReleaseCompound.run(&mut s).expect("releases");
    assert_eq!(s.selected().len(), 2);
    assert_eq!(layer_children(&s).len(), 2);
    s.undo();
    s.undo();
    assert_eq!(layer_children(&s), ids, "each was one step");
}

#[test]
fn the_fill_rule_switch_is_one_undo_step() {
    let (mut s, _) = session(|id| {
        vec![Node::new(
            id(),
            NodeKind::Shape(Shape {
                geometry: Geometry::Path(over_noded(4)),
                style: solid(0, 0, 0),
            }),
        )]
    });
    let id = s.selected()[0];
    set_fill_rule(&mut s, id, FillRule::EvenOdd).expect("sets");
    assert_eq!(path_of(&s, id).fill_rule, FillRule::EvenOdd);
    assert_eq!(s.edit_menu().undo, Some(FILL_RULE));
    s.undo();
    assert_eq!(path_of(&s, id).fill_rule, FillRule::NonZero);
}
