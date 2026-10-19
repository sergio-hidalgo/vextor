//! Small documents for geometry tests, built through commands.

#![allow(dead_code)] // each test binary uses a different part

use vextor_core::{
    Command, Container, CornerRadii, Document, Geometry, Node, NodeId, NodeKind, Paint, Rect,
    Shape, Stroke, Style, Transform,
};

/// A document with one layer; returns it with the layer id.
pub fn doc() -> (Document, NodeId) {
    let doc = Document::new("test");
    let layer = doc.layers[0];
    (doc, layer)
}

/// A filled `w`×`h` rect shape at `(x, y)`, no stroke.
pub fn rect(id: NodeId, x: f64, y: f64, w: f64, h: f64) -> Node {
    Node::new(
        id,
        NodeKind::Shape(Shape {
            geometry: Geometry::Rect {
                rect: Rect::new(x, y, w, h),
                radius: CornerRadii::default(),
            },
            style: Style::default(),
        }),
    )
}

/// `node` with no fill and a stroke `width` wide.
pub fn outlined(mut node: Node, width: f64) -> Node {
    if let NodeKind::Shape(s) = &mut node.kind {
        s.style = Style {
            fill: Paint::None,
            stroke: Some(Stroke {
                width,
                ..Stroke::default()
            }),
        };
    }
    node
}

/// A group of `children` with `transform`.
pub fn group(id: NodeId, children: Vec<NodeId>, transform: Transform) -> Node {
    let mut g = Node::new(
        id,
        NodeKind::Group(Container {
            children,
            clip: None,
        }),
    );
    g.transform = transform;
    g
}

/// Inserts `nodes` (roots go on top) into `parent`.
pub fn add(doc: &mut Document, parent: NodeId, nodes: Vec<Node>) {
    let index = doc.nodes[&parent]
        .container()
        .expect("container")
        .children
        .len();
    doc.apply(Command::Insert {
        parent: Some(parent),
        index,
        nodes,
    })
    .expect("inserts");
}

/// A fresh node id.
pub fn id(doc: &mut Document) -> NodeId {
    doc.alloc_node_id()
}
