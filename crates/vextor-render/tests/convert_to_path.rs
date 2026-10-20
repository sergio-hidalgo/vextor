//! Convert to Path (spec 06): a shape turned into a path draws exactly the
//! same pixels as the parametric shape it was.

use vextor_core::{
    Command, CornerRadii, Document, Geometry, LineCap, LineJoin, Node, NodeKind, Paint, Point,
    Rect, Rgba, Shape, Stroke, Style, Transform, Vec2, View,
};
use vextor_geom::{Fonts, geometry_to_path};
use vextor_render::{Viewport, render};

/// Every parametric kind, filled and stroked, some rounded or rotated.
fn shapes() -> Vec<(Geometry, Transform)> {
    let flat = Transform::IDENTITY;
    let turned = Transform([0.866, 0.5, -0.5, 0.866, 20.0, -10.0]);
    vec![
        (
            Geometry::Rect {
                rect: Rect::new(10.0, 10.0, 60.0, 40.0),
                radius: CornerRadii {
                    tl: 12.0,
                    tr: 0.0,
                    br: 30.0,
                    bl: 4.0,
                },
            },
            flat,
        ),
        (
            Geometry::Rect {
                rect: Rect::new(90.0, 10.0, 30.0, 30.0),
                radius: CornerRadii::default(),
            },
            turned,
        ),
        (
            Geometry::Ellipse {
                center: Point::new(170.0, 35.0),
                rx: 30.0,
                ry: 18.0,
            },
            flat,
        ),
        (
            Geometry::Polygon {
                center: Point::new(40.0, 100.0),
                radius: 28.0,
                sides: 6,
                corner_radius: 6.0,
            },
            flat,
        ),
        (
            Geometry::Polygon {
                center: Point::new(100.0, 100.0),
                radius: 25.0,
                sides: 3,
                corner_radius: 0.0,
            },
            turned,
        ),
        (
            Geometry::Star {
                center: Point::new(160.0, 100.0),
                outer: 30.0,
                inner: 12.0,
                points: 5,
                corner_radius: 3.0,
            },
            flat,
        ),
        (
            Geometry::Line {
                from: Point::new(10.0, 150.0),
                to: Point::new(190.0, 170.0),
            },
            flat,
        ),
    ]
}

fn style(i: usize) -> Style {
    Style {
        fill: Paint::Solid(Rgba::rgb(0x1e, 0x90, 0xff)),
        stroke: Some(Stroke {
            paint: Paint::Solid(Rgba::rgb(0x22, 0x22, 0x22)),
            width: 3.0,
            // Both join kinds: miters show any extra or missing node.
            join: if i.is_multiple_of(2) {
                LineJoin::Miter
            } else {
                LineJoin::Round
            },
            cap: LineCap::Round,
            ..Stroke::default()
        }),
    }
}

fn document() -> Document {
    let mut doc = Document::new("convert");
    let layer = doc.layers[0];
    let nodes: Vec<Node> = shapes()
        .into_iter()
        .enumerate()
        .map(|(i, (geometry, transform))| {
            let mut n = Node::new(
                doc.alloc_node_id(),
                NodeKind::Shape(Shape {
                    geometry,
                    style: style(i),
                }),
            );
            n.transform = transform;
            n
        })
        .collect();
    doc.apply(Command::Insert {
        parent: Some(layer),
        index: 0,
        nodes,
    })
    .expect("inserts");
    doc
}

/// The document at `zoom` (a whole number) over a 200×180 area.
fn pixels(doc: &Document, zoom: u32) -> Vec<u8> {
    let viewport = Viewport {
        view: View {
            zoom: f64::from(zoom),
            pan: Vec2::new(0.0, 0.0),
        },
        width: 200 * zoom,
        height: 180 * zoom,
        pixels_per_point: 1.0,
    };
    render(doc, &Fonts::default(), &viewport)
        .expect("valid viewport")
        .data()
        .to_vec()
}

#[test]
fn converted_shapes_render_pixel_identical() {
    let mut doc = document();
    let items: Vec<_> = doc
        .nodes
        .values()
        .filter_map(|n| match &n.kind {
            NodeKind::Shape(s) => Some((n.id, geometry_to_path(&s.geometry))),
            _ => None,
        })
        .collect();
    assert_eq!(items.len(), shapes().len());
    let before: Vec<Vec<u8>> = [1, 3].map(|z| pixels(&doc, z)).into();
    doc.apply(Command::ConvertToPath { items })
        .expect("converts");
    assert!(doc.nodes.values().all(|n| match &n.kind {
        NodeKind::Shape(s) => matches!(s.geometry, Geometry::Path(_)),
        _ => true,
    }));
    for (zoom, want) in [1, 3].into_iter().zip(before) {
        let got = pixels(&doc, zoom);
        let differing = got.iter().zip(&want).filter(|(a, b)| a != b).count();
        assert_eq!(
            differing, 0,
            "zoom {zoom}: {differing} channel values differ"
        );
    }
}
