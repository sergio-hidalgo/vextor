//! proptest: random command sequences (mixed with undo, redo and gestures)
//! never break the document invariants; a rejected command changes
//! nothing; undoing everything returns to the initial document.

mod common;

use common::{fixture, gradient, group, layer, pattern, rect_geom, shape};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, TestRunner};
use vextor_core::{
    Command, Document, FillRule, Geometry, GridSettings, History, Node, NodeId, NodeKind, NodeType,
    Paint, Parent, Path, PathNode, PathNodeRef, Point, ResourceId, Rgba, Selection, Shape, Style,
    SubPath, Transform, Vec2,
};

/// Raw random material; [`build`] turns it into a step that mostly fits
/// the current document.
#[derive(Clone, Debug)]
struct Op {
    kind: u8,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    x: f64,
    y: f64,
    flag: bool,
}

enum Step {
    Cmd(Command),
    Undo,
    Redo,
    Begin,
    End,
}

fn op() -> impl Strategy<Value = Op> {
    (
        0u8..30,
        any::<usize>(),
        any::<usize>(),
        any::<usize>(),
        any::<usize>(),
        -50.0..50.0f64,
        -50.0..50.0f64,
        any::<bool>(),
    )
        .prop_map(|(kind, a, b, c, d, x, y, flag)| Op {
            kind,
            a,
            b,
            c,
            d,
            x,
            y,
            flag,
        })
}

fn pick<T: Copy>(items: &[T], i: usize, fallback: T) -> T {
    if items.is_empty() {
        fallback
    } else {
        items[i % items.len()]
    }
}

fn ids_where(doc: &Document, f: impl Fn(&Node) -> bool) -> Vec<NodeId> {
    doc.nodes.values().filter(|n| f(n)).map(|n| n.id).collect()
}

fn path_of(doc: &Document, id: NodeId) -> Option<&Path> {
    match &doc.nodes.get(&id)?.kind {
        NodeKind::Shape(Shape {
            geometry: Geometry::Path(p),
            ..
        }) => Some(p),
        _ => None,
    }
}

/// A node reference that is usually valid (and sometimes one past the end).
fn path_ref(doc: &Document, op: &Op, endpoint: bool) -> PathNodeRef {
    let paths = ids_where(doc, |n| path_of(doc, n.id).is_some());
    let shape = pick(&paths, op.a, NodeId(9999));
    let Some(p) = path_of(doc, shape) else {
        return PathNodeRef {
            shape,
            subpath: 0,
            index: 0,
        };
    };
    let subpath = op.b % (p.subpaths.len() + usize::from(op.d.is_multiple_of(10)));
    let len = p.subpaths.get(subpath).map_or(1, |s| s.nodes.len());
    let index = if endpoint {
        if op.flag { 0 } else { len - 1 }
    } else {
        op.c % (len + usize::from(op.d % 10 == 1))
    };
    PathNodeRef {
        shape,
        subpath,
        index,
    }
}

fn small_path(op: &Op) -> Path {
    Path {
        subpaths: vec![SubPath {
            nodes: vec![
                PathNode::corner(op.x, op.y),
                PathNode::corner(op.y, op.x),
                PathNode::corner(0.0, op.x),
            ],
            closed: op.flag,
        }],
        fill_rule: FillRule::EvenOdd,
    }
}

fn new_shape(doc: &mut Document, op: &Op) -> Node {
    let id = doc.alloc_node_id();
    let geometry = match op.c % 3 {
        0 => rect_geom(op.x, op.y, op.x.abs(), op.y.abs()),
        1 => Geometry::Path(small_path(op)),
        _ => Geometry::Ellipse {
            center: Point::new(op.x, op.y),
            rx: op.x.abs(),
            ry: 1.0,
        },
    };
    shape(id, geometry)
}

#[allow(clippy::too_many_lines)] // one arm per command kind
fn build(doc: &mut Document, op: &Op) -> Step {
    let all = ids_where(doc, |_| true);
    let containers = ids_where(doc, |n| n.container().is_some());
    let shapes = ids_where(doc, |n| matches!(n.kind, NodeKind::Shape(_)));
    let resources: Vec<ResourceId> = doc.resources.keys().copied().collect();
    let any = pick(&all, op.a, NodeId(9999));
    let container = pick(&containers, op.b, NodeId(9999));
    let container_len = doc
        .nodes
        .get(&container)
        .and_then(Node::container)
        .map_or(0, |c| c.children.len());
    let cmd = match op.kind {
        0 => Command::Insert {
            parent: Some(container),
            index: op.c % (container_len + 1),
            nodes: vec![new_shape(doc, op)],
        },
        1 => {
            let (g, s1, s2) = (doc.alloc_node_id(), new_shape(doc, op), new_shape(doc, op));
            let nodes = vec![group(g, vec![s1.id, s2.id]), s1, s2];
            Command::Insert {
                parent: Some(container),
                index: 0,
                nodes,
            }
        }
        2 => {
            let l = doc.alloc_node_id();
            Command::Insert {
                parent: None,
                index: op.c % (doc.layers.len() + 1),
                nodes: vec![layer(l, vec![])],
            }
        }
        3 => Command::Delete { ids: vec![any] },
        4 => {
            let target = if op.flag { None } else { Some(container) };
            Command::Reparent {
                ids: vec![any],
                new_parent: target,
                index: op.c % (container_len + 1),
            }
        }
        5 => match doc.parent_of(any) {
            Some(Parent::Node(p)) => {
                let siblings = doc.nodes[&p]
                    .container()
                    .map(|c| c.children.clone())
                    .unwrap_or_default();
                let mut ids = vec![any];
                let other = pick(&siblings, op.c, any);
                if other != any {
                    ids.push(other);
                }
                Command::Group { ids }
            }
            _ => Command::Group { ids: vec![any] },
        },
        6 => Command::Ungroup { id: any },
        7 => Command::SetTransform {
            ids: vec![any],
            transform: Transform([1.0, 0.0, op.x / 100.0, 1.0, op.x, op.y]),
        },
        8 => {
            let fill = match op.c % 4 {
                0 => Paint::Solid(Rgba::rgb(1, 2, 3)),
                1 => Paint::Gradient(pick(&resources, op.d, ResourceId(9999))),
                2 => Paint::Pattern(pick(&resources, op.d, ResourceId(9999))),
                _ => Paint::CurrentColor,
            };
            Command::SetStyle {
                ids: vec![pick(&shapes, op.a, any)],
                style: Style { fill, stroke: None },
            }
        }
        9 => Command::SetProps {
            id: any,
            name: op.flag.then(|| "n".into()),
            visible: op.flag,
            locked: !op.flag,
            opacity: (op.x / 50.0).abs(),
        },
        10 => Command::AddPathNode {
            at: path_ref(doc, op, false),
            node: PathNode::corner(op.x, op.y),
        },
        11 => Command::MovePathNodes {
            nodes: vec![path_ref(doc, op, false)],
            delta: Vec2::new(op.x, op.y),
        },
        12 => {
            let h = op.flag.then(|| Vec2::new(op.x, op.y));
            Command::SetHandles {
                node: path_ref(doc, op, false),
                handle_in: h.map(|v| Vec2::new(op.y, v.x)),
                handle_out: h,
            }
        }
        13 => {
            let kind = [NodeType::Corner, NodeType::Smooth, NodeType::Symmetric][op.c % 3];
            Command::SetNodeType {
                node: path_ref(doc, op, false),
                kind,
            }
        }
        14 => {
            let r = path_ref(doc, op, false);
            Command::ToggleSegment {
                shape: r.shape,
                subpath: r.subpath,
                index: r.index,
            }
        }
        15 => Command::DeletePathNodes {
            nodes: vec![path_ref(doc, op, false)],
        },
        16 => {
            let r = path_ref(doc, op, false);
            Command::CloseSubPath {
                shape: r.shape,
                subpath: r.subpath,
            }
        }
        17 => Command::OpenAt {
            node: path_ref(doc, op, false),
        },
        18 => {
            let a = path_ref(doc, op, true);
            let other = Op {
                b: op.c,
                flag: !op.flag,
                ..op.clone()
            };
            Command::JoinEndpoints {
                a,
                b: path_ref(doc, &other, true),
            }
        }
        19 => Command::SplitAt {
            node: path_ref(doc, op, false),
        },
        20 => {
            let r = path_ref(doc, op, false);
            Command::Reverse {
                shape: r.shape,
                subpath: r.subpath,
            }
        }
        21 => Command::ConvertToPath {
            items: vec![(pick(&shapes, op.a, any), small_path(op))],
        },
        22 => {
            let inserted = vec![new_shape(doc, op)];
            Command::ReplaceWith {
                removed: vec![any],
                inserted,
                parent: Some(container),
                index: 0,
            }
        }
        23 => {
            let id = if op.flag {
                doc.alloc_resource_id()
            } else {
                pick(&resources, op.d, ResourceId(9999))
            };
            if op.c.is_multiple_of(2) {
                Command::PutResource {
                    id,
                    resource: gradient(),
                    nodes: vec![],
                }
            } else {
                let content = new_shape(doc, op);
                Command::PutResource {
                    id,
                    resource: pattern(vec![content.id]),
                    nodes: vec![content],
                }
            }
        }
        24 => Command::RemoveResource {
            id: pick(&resources, op.d, ResourceId(9999)),
        },
        25 => Command::SetGrid(GridSettings {
            size: op.x,
            ..GridSettings::default()
        }),
        26 => return Step::Undo,
        27 => return Step::Redo,
        28 => return Step::Begin,
        _ => return Step::End,
    };
    Step::Cmd(cmd)
}

/// Runs a sequence; returns (commands accepted, commands rejected).
fn run_sequence(ops: &[Op]) -> Result<(usize, usize), TestCaseError> {
    let fx = fixture();
    let initial = fx.doc.clone();
    let mut doc = fx.doc;
    let mut h = History::new();
    let (mut ok, mut rejected) = (0, 0);
    for op in ops {
        match build(&mut doc, op) {
            Step::Cmd(cmd) => {
                let before = doc.clone();
                if h.apply(&mut doc, cmd, &Selection::None).is_ok() {
                    ok += 1;
                } else {
                    rejected += 1;
                    prop_assert_eq!(&doc, &before, "a rejected command changed the document");
                }
            }
            Step::Undo => {
                h.undo(&mut doc);
            }
            Step::Redo => {
                h.redo(&mut doc);
            }
            Step::Begin => h.begin_gesture(),
            Step::End => h.end_gesture(),
        }
        prop_assert!(
            doc.validate().is_ok(),
            "{:?} after {:?}",
            doc.validate(),
            op
        );
    }
    while h.redo(&mut doc) {}
    let last = doc.clone();
    while h.undo(&mut doc).is_some() {
        prop_assert!(doc.validate().is_ok(), "{:?} during undo", doc.validate());
    }
    prop_assert_eq!(&doc, &initial, "undo all");
    while h.redo(&mut doc) {}
    prop_assert_eq!(&doc, &last, "redo all");
    Ok((ok, rejected))
}

proptest! {
    #![proptest_config(Config::with_cases(512))]

    #[test]
    fn random_sequences_keep_invariants_and_undo_to_start(ops in prop::collection::vec(op(), 1..80)) {
        run_sequence(&ops)?;
    }
}

/// Guards the generator: if most commands were rejected the property
/// above would prove little.
#[test]
fn generator_mostly_produces_applicable_commands() {
    let mut runner = TestRunner::deterministic();
    let strategy = prop::collection::vec(op(), 40);
    let (mut ok, mut rejected) = (0, 0);
    for _ in 0..100 {
        let ops = strategy.new_tree(&mut runner).expect("generates").current();
        let (o, r) = run_sequence(&ops).expect("property holds");
        ok += o;
        rejected += r;
    }
    assert!(ok * 2 > rejected, "accepted {ok}, rejected {rejected}");
}
