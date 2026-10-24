//! Building small documents for export tests, and exporting them.

#![allow(dead_code)] // each test file uses part of it

use std::sync::Arc;

use vextor_core::{
    Artboard, ArtboardId, Command, Container, CornerRadii, Document, FillRule, Geometry, Node,
    NodeId, NodeKind, Paint, Path, PathNode, Point, Rect, Resource, ResourceId, Rgba, Shape,
    Stroke, Style, SubPath, SvgExportOptions, Transform,
};
use vextor_geom::{FontDb, Fonts};
use vextor_svg::{Export, Format, Scope, export};

const REGULAR: &[u8] =
    include_bytes!("../../../vextor-geom/tests/fixtures/fonts/NotoSans-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../../vextor-geom/tests/fixtures/fonts/NotoSans-Bold.ttf");

/// The bundled OFL test fonts (Noto Sans Regular and Bold).
pub fn fonts() -> Fonts {
    Fonts::new(Arc::new(FontDb::from_data([
        REGULAR.to_vec(),
        BOLD.to_vec(),
    ])))
}

/// A document with one artboard and helpers to fill it.
#[allow(clippy::struct_field_names)] // `d.doc` reads as "the document"
pub struct Doc {
    pub doc: Document,
    pub artboard: ArtboardId,
    layer: NodeId,
}

impl Doc {
    /// A `width` × `height` artboard at (`x`, `y`).
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        let mut doc = Document::new("Sample");
        let artboard = doc.alloc_artboard_id();
        doc.apply(Command::SetArtboards(vec![Artboard {
            id: artboard,
            name: "icon-24".to_owned(),
            rect: Rect::new(x, y, width, height),
            background: None,
        }]))
        .expect("adds the artboard");
        let layer = doc.layers[0];
        Self {
            doc,
            artboard,
            layer,
        }
    }

    pub fn id(&mut self) -> NodeId {
        self.doc.alloc_node_id()
    }

    /// Adds `node` (and `inner`, its descendants) on top of the layer.
    pub fn add(&mut self, node: Node, inner: Vec<Node>) -> NodeId {
        let id = node.id;
        let index = self.doc.nodes[&self.layer]
            .container()
            .map_or(0, |c| c.children.len());
        let mut nodes = vec![node];
        nodes.extend(inner);
        self.doc
            .apply(Command::Insert {
                parent: Some(self.layer),
                index,
                nodes,
            })
            .expect("inserts");
        id
    }

    /// Adds a shape on top of the layer.
    pub fn shape(&mut self, geometry: Geometry, style: Style) -> NodeId {
        let id = self.id();
        self.add(shape_node(id, geometry, style), Vec::new())
    }

    pub fn resource(&mut self, resource: Resource) -> ResourceId {
        let id = self.doc.alloc_resource_id();
        self.doc
            .apply(Command::PutResource {
                id,
                resource,
                nodes: Vec::new(),
            })
            .expect("adds the resource");
        id
    }

    /// Changes the node with `f`, as a command.
    pub fn edit(&mut self, id: NodeId, f: impl FnOnce(&mut Node)) {
        let mut node = self.doc.nodes[&id].clone();
        f(&mut node);
        self.doc.nodes.insert(id, node);
        assert!(self.doc.validate().is_ok(), "{:?}", self.doc.validate());
    }

    pub fn options(&mut self, f: impl FnOnce(&mut SvgExportOptions)) {
        let mut o = self.doc.export.clone();
        f(&mut o);
        self.doc
            .apply(Command::SetExport(o))
            .expect("valid options");
    }

    /// The artboard as `format`.
    pub fn export(&self, format: Format) -> Export {
        export(
            &self.doc,
            &fonts(),
            Scope::Artboard(self.artboard),
            format,
            &self.doc.export,
        )
        .expect("the artboard exists")
    }

    /// The artboard as an SVG file.
    pub fn svg(&self) -> String {
        self.export(Format::Svg).code
    }
}

pub fn shape_node(id: NodeId, geometry: Geometry, style: Style) -> Node {
    Node::new(id, NodeKind::Shape(Shape { geometry, style }))
}

pub fn group_node(id: NodeId, children: Vec<NodeId>) -> Node {
    Node::new(
        id,
        NodeKind::Group(Container {
            children,
            clip: None,
        }),
    )
}

pub fn rect(x: f64, y: f64, width: f64, height: f64, radius: f64) -> Geometry {
    Geometry::Rect {
        rect: Rect::new(x, y, width, height),
        radius: CornerRadii {
            tl: radius,
            tr: radius,
            br: radius,
            bl: radius,
        },
    }
}

/// A closed straight-sided path through `points`.
pub fn polygon_path(points: &[(f64, f64)], fill_rule: FillRule) -> Geometry {
    Geometry::Path(Path {
        subpaths: vec![SubPath {
            nodes: points
                .iter()
                .map(|&(x, y)| PathNode::corner(x, y))
                .collect(),
            closed: true,
        }],
        fill_rule,
    })
}

pub fn fill(c: Rgba) -> Style {
    Style {
        fill: Paint::Solid(c),
        stroke: None,
    }
}

pub fn stroked(paint: Paint, width: f64) -> Style {
    Style {
        fill: Paint::None,
        stroke: Some(Stroke {
            paint,
            width,
            ..Stroke::default()
        }),
    }
}

pub const RED: Rgba = Rgba::rgb(255, 0, 0);
pub const BLUE: Rgba = Rgba::rgb(0, 0, 255);

pub fn point(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

pub fn moved(x: f64, y: f64) -> Transform {
    Transform::translate(x, y)
}
