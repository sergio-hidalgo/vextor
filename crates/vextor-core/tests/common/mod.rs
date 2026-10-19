//! Shared fixture: a small document using every node kind and resource.

#![allow(dead_code)] // each test binary uses a different part

use vextor_core::{Command, FillRule};
use vextor_core::{
    Container, CornerRadii, Document, Geometry, Gradient, GradientKind, GradientStop,
    GradientUnits, History, Image, ImageMime, ImageSource, Node, NodeId, NodeKind, NodeType, Paint,
    Path, PathNode, Pattern, Point, Rect, Resource, ResourceId, Rgba, Selection, Shape, Spread,
    Stroke, Style, SubPath, Text, TextAlign, TextFrame, Vec2,
};

/// The fixture document and the ids of everything in it.
pub struct Fx {
    pub doc: Document,
    pub layer: NodeId,
    pub layer2: NodeId,
    pub group: NodeId,
    pub rect: NodeId,
    pub ellipse: NodeId,
    pub path: NodeId,
    pub star: NodeId,
    pub image: NodeId,
    pub text: NodeId,
    pub gradient: ResourceId,
    pub pattern: ResourceId,
    pub pattern_rect: NodeId,
}

pub fn shape(id: NodeId, geometry: Geometry) -> Node {
    Node::new(
        id,
        NodeKind::Shape(Shape {
            geometry,
            style: Style::default(),
        }),
    )
}

/// "Buy now" over two lines, centred, bold, with a framed background.
pub fn label_text() -> Text {
    Text {
        size: 18.0,
        align: TextAlign::Centre,
        frame: Some(TextFrame {
            padding: [8.0, 16.0, 8.0, 16.0],
            radius: CornerRadii {
                tl: 6.0,
                tr: 6.0,
                br: 6.0,
                bl: 6.0,
            },
            fill: Paint::Solid(Rgba::rgb(30, 144, 255)),
            stroke: Some(Stroke::default()),
        }),
        font: vextor_core::FontSpec {
            family: "Inter".to_owned(),
            weight: 700,
            italic: false,
        },
        ..Text::new("Buy\nnow", "Inter")
    }
}

pub fn text(id: NodeId, text: Text) -> Node {
    Node::new(id, NodeKind::Text(text))
}

pub fn group(id: NodeId, children: Vec<NodeId>) -> Node {
    Node::new(
        id,
        NodeKind::Group(Container {
            children,
            clip: None,
        }),
    )
}

pub fn layer(id: NodeId, children: Vec<NodeId>) -> Node {
    Node::new(
        id,
        NodeKind::Layer(Container {
            children,
            clip: None,
        }),
    )
}

pub fn rect_geom(x: f64, y: f64, w: f64, h: f64) -> Geometry {
    Geometry::Rect {
        rect: Rect::new(x, y, w, h),
        radius: CornerRadii::default(),
    }
}

pub fn curve_node(x: f64, y: f64, kind: NodeType) -> PathNode {
    PathNode {
        handle_in: Some(Vec2::new(-5.0, 0.0)),
        handle_out: Some(Vec2::new(5.0, 0.0)),
        kind,
        ..PathNode::corner(x, y)
    }
}

/// Open subpath 0 (0,0)→(10,0)→(20,0) with a symmetric middle node;
/// closed triangle subpath 1.
pub fn sample_path() -> Path {
    Path {
        subpaths: vec![
            SubPath {
                nodes: vec![
                    PathNode::corner(0.0, 0.0),
                    curve_node(10.0, 0.0, NodeType::Symmetric),
                    PathNode::corner(20.0, 0.0),
                ],
                closed: false,
            },
            SubPath {
                nodes: vec![
                    PathNode::corner(0.0, 10.0),
                    PathNode::corner(10.0, 20.0),
                    PathNode::corner(0.0, 20.0),
                ],
                closed: true,
            },
        ],
        fill_rule: FillRule::NonZero,
    }
}

pub fn gradient() -> Resource {
    Resource::Gradient(Gradient {
        name: Some("brand".to_owned()),
        kind: GradientKind::Linear {
            start: Point::new(0.0, 0.0),
            end: Point::new(1.0, 0.0),
        },
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: Rgba::rgb(255, 0, 0),
            },
            GradientStop {
                offset: 1.0,
                color: Rgba::rgb(0, 0, 255),
            },
        ],
        units: GradientUnits::ObjectBoundingBox,
        spread: Spread::Pad,
    })
}

pub fn pattern(content: Vec<NodeId>) -> Resource {
    Resource::Pattern(Pattern {
        name: None,
        tile: Rect::new(0.0, 0.0, 4.0, 4.0),
        transform: vextor_core::Transform::IDENTITY,
        content,
    })
}

/// Layer 1: group(rect, ellipse), path, star. Layer 2: image, text.
/// Rect is filled with a gradient; an unused pattern holds one rect.
#[allow(clippy::too_many_lines)] // builds one document step by step
pub fn fixture() -> Fx {
    let mut doc = Document::new("fixture");
    let layer = doc.layers[0];
    let ids: Vec<NodeId> = (0..9).map(|_| doc.alloc_node_id()).collect();
    let [
        layer2,
        group_id,
        rect,
        ellipse,
        path,
        star,
        image,
        pattern_rect,
        text_id,
    ] = ids.try_into().expect("9 ids");
    let gradient_id = doc.alloc_resource_id();
    let pattern_id = doc.alloc_resource_id();

    let mut h = History::new();
    let mut run =
        |doc: &mut Document, cmd| h.apply(doc, cmd, &Selection::None).expect("fixture builds");
    run(
        &mut doc,
        Command::PutResource {
            id: gradient_id,
            resource: gradient(),
            nodes: vec![],
        },
    );
    run(
        &mut doc,
        Command::PutResource {
            id: pattern_id,
            resource: pattern(vec![pattern_rect]),
            nodes: vec![shape(pattern_rect, rect_geom(0.0, 0.0, 2.0, 2.0))],
        },
    );

    let mut rect_node = shape(rect, rect_geom(0.0, 0.0, 10.0, 10.0));
    if let NodeKind::Shape(s) = &mut rect_node.kind {
        s.style = Style {
            fill: Paint::Gradient(gradient_id),
            stroke: Some(Stroke::default()),
        };
    }
    let ellipse_node = shape(
        ellipse,
        Geometry::Ellipse {
            center: Point::new(5.0, 5.0),
            rx: 3.0,
            ry: 2.0,
        },
    );
    run(
        &mut doc,
        Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![
                group(group_id, vec![rect, ellipse]),
                rect_node,
                ellipse_node,
            ],
        },
    );
    let star_geom = Geometry::Star {
        center: Point::new(50.0, 50.0),
        outer: 10.0,
        inner: 4.0,
        points: 5,
        corner_radius: 0.0,
    };
    run(
        &mut doc,
        Command::Insert {
            parent: Some(layer),
            index: 1,
            nodes: vec![
                shape(path, Geometry::Path(sample_path())),
                shape(star, star_geom),
            ],
        },
    );
    let image_node = Node::new(
        image,
        NodeKind::Image(Image {
            rect: Rect::new(0.0, 0.0, 8.0, 8.0),
            source: ImageSource::Embedded {
                mime: ImageMime::Png,
                bytes: vec![0x89, b'P', b'N', b'G'],
            },
        }),
    );
    run(
        &mut doc,
        Command::Insert {
            parent: None,
            index: 1,
            nodes: vec![
                self::layer(layer2, vec![image, text_id]),
                image_node,
                text(text_id, label_text()),
            ],
        },
    );

    doc.validate().expect("fixture is valid");
    Fx {
        doc,
        layer,
        layer2,
        group: group_id,
        rect,
        ellipse,
        path,
        star,
        image,
        text: text_id,
        gradient: gradient_id,
        pattern: pattern_id,
        pattern_rect,
    }
}

/// Mutable access to a node of `doc`.
pub fn node_mut(doc: &mut Document, id: NodeId) -> &mut Node {
    doc.nodes.get_mut(&id).expect("node exists")
}

/// Children of a layer or group.
pub fn children_mut(doc: &mut Document, id: NodeId) -> &mut Vec<NodeId> {
    &mut node_mut(doc, id)
        .container_mut()
        .expect("container")
        .children
}

/// Mutable path geometry of a shape.
pub fn path_mut(doc: &mut Document, id: NodeId) -> &mut Path {
    match &mut node_mut(doc, id).kind {
        NodeKind::Shape(Shape {
            geometry: Geometry::Path(p),
            ..
        }) => p,
        _ => panic!("{id:?} is not a path"),
    }
}

/// Applies `cmd` to a copy of `doc` and checks apply → undo → redo:
/// the result equals `doc` edited by `expect`, undo restores `doc`, redo
/// gives the result again, and every step is valid. Returns the result.
pub fn check(doc: &Document, cmd: Command, expect: impl FnOnce(&mut Document)) -> Document {
    let mut expected = doc.clone();
    expect(&mut expected);
    let mut d = doc.clone();
    let mut h = History::new();
    h.apply(&mut d, cmd, &Selection::None)
        .expect("command applies");
    assert_eq!(d, expected, "after apply");
    d.validate().expect("valid after apply");
    assert!(h.undo(&mut d).is_some());
    assert_eq!(&d, doc, "after undo");
    d.validate().expect("valid after undo");
    assert!(h.redo(&mut d));
    assert_eq!(d, expected, "after redo");
    d.validate().expect("valid after redo");
    d
}

/// `cmd` is rejected and the document is left exactly as it was.
pub fn rejects(doc: &Document, cmd: Command) {
    let mut d = doc.clone();
    assert!(d.apply(cmd).is_err(), "command should be rejected");
    assert_eq!(&d, doc);
}
