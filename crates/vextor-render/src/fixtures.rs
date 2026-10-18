//! Sample documents for render tests, benchmarks and the app's debug
//! menu, built through the same [`Command`]s the editor uses.

use vextor_core::{
    Artboard, Command, Container, CornerRadii, Document, Geometry, LineCap, LineJoin, Node, NodeId,
    NodeKind, NodeType, Paint, Path, PathNode, Point, Rect, Rgba, Shape, Stroke, Style, SubPath,
    Transform, Vec2,
};

/// Size of the [`basic_shapes`] artboard, in document px.
pub const BASIC_SHAPES_SIZE: (f64, f64) = (200.0, 120.0);

/// Rects, ellipses, paths, strokes (caps, dashes), a half-transparent
/// group and a rotated shape on one white 200×120 artboard. Its SVG twin
/// is `tests/fixtures/basic_shapes.svg`.
#[must_use]
#[allow(clippy::too_many_lines)] // builds one document step by step
pub fn basic_shapes() -> Document {
    let mut doc = Document::new("basic shapes");
    let (w, h) = BASIC_SHAPES_SIZE;
    add_artboard(
        &mut doc,
        "basic-shapes",
        w,
        h,
        Some(Rgba::rgb(255, 255, 255)),
    );

    let add = |doc: &mut Document, geometry: Geometry, style: Style| {
        shape(doc.alloc_node_id(), geometry, style)
    };
    let rounded = add(
        &mut doc,
        Geometry::Rect {
            rect: Rect::new(10.0, 10.0, 60.0, 40.0),
            radius: corners(8.0),
        },
        fill(Rgba::rgb(0x1e, 0x90, 0xff)),
    );
    let ellipse = add(
        &mut doc,
        Geometry::Ellipse {
            center: Point::new(120.0, 30.0),
            rx: 30.0,
            ry: 20.0,
        },
        Style {
            fill: Paint::Solid(Rgba::rgb(0xff, 0x63, 0x47)),
            stroke: Some(Stroke {
                width: 3.0,
                ..Stroke::default()
            }),
        },
    );
    let triangle = add(
        &mut doc,
        polygon_path(&[(20.0, 70.0), (70.0, 110.0), (10.0, 110.0)]),
        fill(Rgba::rgb(0x2e, 0x8b, 0x57)),
    );
    let curve = add(
        &mut doc,
        Geometry::Path(Path {
            subpaths: vec![SubPath {
                nodes: vec![
                    PathNode {
                        handle_out: Some(Vec2::new(20.0, -40.0)),
                        ..PathNode::corner(90.0, 110.0)
                    },
                    PathNode {
                        handle_in: Some(Vec2::new(-20.0, 0.0)),
                        kind: NodeType::Corner,
                        ..PathNode::corner(140.0, 70.0)
                    },
                ],
                closed: false,
            }],
            fill_rule: vextor_core::FillRule::NonZero,
        }),
        Style {
            fill: Paint::None,
            stroke: Some(Stroke {
                paint: Paint::Solid(Rgba::rgb(0x22, 0x22, 0x22)),
                width: 4.0,
                cap: LineCap::Round,
                join: LineJoin::Round,
                ..Stroke::default()
            }),
        },
    );
    let dashed = add(
        &mut doc,
        Geometry::Line {
            from: Point::new(150.0, 70.0),
            to: Point::new(190.0, 110.0),
        },
        Style {
            fill: Paint::None,
            stroke: Some(Stroke {
                paint: Paint::Solid(Rgba::rgb(0x80, 0x00, 0x80)),
                width: 2.0,
                dash: vec![6.0, 4.0],
                ..Stroke::default()
            }),
        },
    );
    let red = add(
        &mut doc,
        rect(155.0, 10.0, 25.0, 25.0),
        fill(Rgba::rgb(0xff, 0, 0)),
    );
    let blue = add(
        &mut doc,
        rect(167.0, 22.0, 25.0, 25.0),
        fill(Rgba::rgb(0, 0, 0xff)),
    );
    let mut rotated = add(
        &mut doc,
        rect(-10.0, -5.0, 20.0, 10.0),
        fill(Rgba::rgb(0xff, 0xa5, 0x00)),
    );
    rotated.transform = rotation(30.0, 80.0, 62.0);

    let mut group = Node::new(
        doc.alloc_node_id(),
        NodeKind::Group(Container {
            children: vec![red.id, blue.id],
            clip: None,
        }),
    );
    group.opacity = 0.5;

    let roots = vec![
        rounded, ellipse, triangle, curve, dashed, group, red, blue, rotated,
    ];
    insert(&mut doc, roots);
    doc
}

/// `count` shapes (rects, ellipses, stars and stroked paths in turn) laid
/// out on a grid, ten per group, on one artboard sized to hold them.
#[must_use]
pub fn many_nodes(count: usize) -> Document {
    const COLS: usize = 25;
    const CELL: f64 = 40.0;
    let mut doc = Document::new(&format!("{count} nodes"));
    let rows = count.div_ceil(COLS).max(1);
    #[allow(clippy::cast_precision_loss)] // small counts
    let (width, height) = (COLS as f64 * CELL, rows as f64 * CELL);
    add_artboard(
        &mut doc,
        "many-nodes",
        width,
        height,
        Some(Rgba::rgb(255, 255, 255)),
    );

    let mut nodes = Vec::with_capacity(count + count / 10 + 1);
    let mut group_children = Vec::new();
    for i in 0..count {
        #[allow(clippy::cast_precision_loss)] // small counts
        let (x, y) = ((i % COLS) as f64 * CELL, (i / COLS) as f64 * CELL);
        let centre = Point::new(x + CELL / 2.0, y + CELL / 2.0);
        let hue = palette(i);
        let (geometry, style) = match i % 4 {
            0 => (
                Geometry::Rect {
                    rect: Rect::new(x + 4.0, y + 4.0, CELL - 8.0, CELL - 8.0),
                    radius: corners(6.0),
                },
                fill(hue),
            ),
            1 => (
                Geometry::Ellipse {
                    center: centre,
                    rx: 16.0,
                    ry: 12.0,
                },
                Style {
                    fill: Paint::Solid(hue),
                    stroke: Some(Stroke {
                        width: 2.0,
                        ..Stroke::default()
                    }),
                },
            ),
            2 => (
                Geometry::Star {
                    center: centre,
                    outer: 17.0,
                    inner: 8.0,
                    points: 5,
                    corner_radius: 0.0,
                },
                fill(hue),
            ),
            _ => (
                polygon_path(&[
                    (x + 4.0, y + 36.0),
                    (x + 20.0, y + 4.0),
                    (x + 36.0, y + 36.0),
                ]),
                Style {
                    fill: Paint::None,
                    stroke: Some(Stroke {
                        paint: Paint::Solid(hue),
                        width: 3.0,
                        join: LineJoin::Round,
                        ..Stroke::default()
                    }),
                },
            ),
        };
        let node = shape(doc.alloc_node_id(), geometry, style);
        group_children.push(node.id);
        nodes.push(node);
        if group_children.len() == 10 || i + 1 == count {
            let children = std::mem::take(&mut group_children);
            nodes.push(Node::new(
                doc.alloc_node_id(),
                NodeKind::Group(Container {
                    children,
                    clip: None,
                }),
            ));
        }
    }
    if !nodes.is_empty() {
        insert(&mut doc, nodes);
    }
    doc
}

fn add_artboard(doc: &mut Document, name: &str, w: f64, h: f64, background: Option<Rgba>) {
    let artboard = Artboard {
        id: doc.alloc_artboard_id(),
        name: name.to_owned(),
        rect: Rect::new(0.0, 0.0, w, h),
        background,
    };
    doc.apply(Command::SetArtboards(vec![artboard]))
        .expect("fixture artboard is valid: finite, positive size");
}

/// Inserts `nodes` (roots and their descendants) at the top of the first layer.
fn insert(doc: &mut Document, nodes: Vec<Node>) {
    let layer = doc.layers[0];
    doc.apply(Command::Insert {
        parent: Some(layer),
        index: 0,
        nodes,
    })
    .expect("fixture nodes are valid: fresh ids, finite geometry");
}

fn shape(id: NodeId, geometry: Geometry, style: Style) -> Node {
    Node::new(id, NodeKind::Shape(Shape { geometry, style }))
}

fn fill(color: Rgba) -> Style {
    Style {
        fill: Paint::Solid(color),
        stroke: None,
    }
}

fn corners(r: f64) -> CornerRadii {
    CornerRadii {
        tl: r,
        tr: r,
        br: r,
        bl: r,
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Geometry {
    Geometry::Rect {
        rect: Rect::new(x, y, w, h),
        radius: CornerRadii::default(),
    }
}

fn polygon_path(points: &[(f64, f64)]) -> Geometry {
    Geometry::Path(Path {
        subpaths: vec![SubPath {
            nodes: points
                .iter()
                .map(|&(x, y)| PathNode::corner(x, y))
                .collect(),
            closed: true,
        }],
        fill_rule: vextor_core::FillRule::NonZero,
    })
}

/// Rotation by `degrees` about the origin, then a move to `(tx, ty)`.
fn rotation(degrees: f64, tx: f64, ty: f64) -> Transform {
    let (sin, cos) = degrees.to_radians().sin_cos();
    Transform([cos, sin, -sin, cos, tx, ty])
}

fn palette(i: usize) -> Rgba {
    const COLORS: [Rgba; 5] = [
        Rgba::rgb(0x1e, 0x90, 0xff),
        Rgba::rgb(0xff, 0x63, 0x47),
        Rgba::rgb(0x2e, 0x8b, 0x57),
        Rgba::rgb(0xff, 0xa5, 0x00),
        Rgba::rgb(0x80, 0x00, 0x80),
    ];
    COLORS[i % COLORS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_shapes_is_valid() {
        let doc = basic_shapes();
        assert!(doc.validate().is_ok());
        assert_eq!(doc.artboards.len(), 1);
    }

    #[test]
    fn many_nodes_has_the_requested_shape_count() {
        let doc = many_nodes(500);
        assert!(doc.validate().is_ok());
        let shapes = doc
            .nodes
            .values()
            .filter(|n| matches!(n.kind, NodeKind::Shape(_)))
            .count();
        assert_eq!(shapes, 500);
    }

    #[test]
    fn many_nodes_zero_is_an_empty_artboard() {
        let doc = many_nodes(0);
        assert!(doc.validate().is_ok());
        assert_eq!(doc.nodes.len(), 1, "only the default layer");
    }
}
