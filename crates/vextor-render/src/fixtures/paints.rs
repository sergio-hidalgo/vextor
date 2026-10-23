//! The [`paints`] fixture (spec 09): one object per paint kind.

use vextor_core::{
    Command, Document, Geometry, Gradient, GradientKind, GradientStop, GradientUnits, LineCap,
    Node, Paint, Pattern, Point, Rect, Resource, ResourceId, Rgba, Spread, Stroke, Style,
    Transform,
};

use super::{add_artboard, fill, insert, rect, rotation, shape};

/// Size of the [`paints`] artboard, in document px.
pub const PAINTS_SIZE: (f64, f64) = (200.0, 160.0);

/// Linear and radial gradients (bounding-box and user-space units, pad /
/// reflect / repeat, a focal point, a translucent stop), two patterns (one
/// with an offset tile and a rotation), a gradient stroke, a dotted stroke
/// and a half-transparent shape on one white 200×160 artboard. Its SVG
/// twin is `tests/fixtures/paints.svg`.
#[must_use]
#[allow(clippy::too_many_lines)] // builds one document step by step
pub fn paints() -> Document {
    let mut doc = Document::new("paints");
    let (w, h) = PAINTS_SIZE;
    add_artboard(&mut doc, "paints", w, h, Some(Rgba::rgb(255, 255, 255)));

    let diagonal = put(
        &mut doc,
        linear(
            (0.0, 0.0),
            (1.0, 1.0),
            &[(0.0, Rgba::rgb(255, 0, 0)), (1.0, rgba(0, 0, 255, 128))],
            GradientUnits::ObjectBoundingBox,
            Spread::Pad,
        ),
    );
    let stripes = put(
        &mut doc,
        linear(
            (80.0, 35.0),
            (100.0, 35.0),
            &[
                (0.0, Rgba::rgb(0xff, 0xd7, 0)),
                (1.0, Rgba::rgb(0x2e, 0x8b, 0x57)),
            ],
            GradientUnits::UserSpace,
            Spread::Reflect,
        ),
    );
    let rings = put(
        &mut doc,
        Gradient {
            name: None,
            kind: GradientKind::Radial {
                center: Point::new(0.5, 0.5),
                radius: 0.25,
                focal: Some(Point::new(0.4, 0.4)),
            },
            stops: stops(&[
                (0.0, Rgba::rgb(255, 255, 255)),
                (1.0, Rgba::rgb(0x1e, 0x90, 0xff)),
            ]),
            units: GradientUnits::ObjectBoundingBox,
            spread: Spread::Repeat,
        },
    );
    let glow = put(
        &mut doc,
        Gradient {
            name: None,
            kind: GradientKind::Radial {
                center: Point::new(35.0, 95.0),
                radius: 30.0,
                focal: None,
            },
            stops: stops(&[
                (0.2, Rgba::rgb(0xff, 0x63, 0x47)),
                (0.5, rgba(255, 255, 255, 128)),
                (1.0, Rgba::rgb(0x80, 0, 0x80)),
            ]),
            units: GradientUnits::UserSpace,
            spread: Spread::Pad,
        },
    );

    let dots = pattern(
        &mut doc,
        Rect::new(0.0, 0.0, 10.0, 10.0),
        Transform::IDENTITY,
        vec![
            (rect(0.0, 0.0, 5.0, 5.0), Rgba::rgb(0x22, 0x22, 0x22)),
            (
                Geometry::Ellipse {
                    center: Point::new(7.5, 7.5),
                    rx: 2.5,
                    ry: 2.5,
                },
                Rgba::rgb(0xff, 0x63, 0x47),
            ),
        ],
    );
    let bars = pattern(
        &mut doc,
        Rect::new(2.0, 2.0, 8.0, 8.0),
        rotation(45.0, 0.0, 0.0),
        vec![(rect(0.0, 0.0, 4.0, 8.0), Rgba::rgb(0x1e, 0x90, 0xff))],
    );

    let painted = |doc: &mut Document, geometry: Geometry, fill: Paint| {
        shape(doc.alloc_node_id(), geometry, Style { fill, stroke: None })
    };
    let circle = |cx: f64, cy: f64| Geometry::Ellipse {
        center: Point::new(cx, cy),
        rx: 25.0,
        ry: 25.0,
    };
    let mut nodes = vec![
        painted(
            &mut doc,
            rect(10.0, 10.0, 50.0, 50.0),
            Paint::Gradient(diagonal),
        ),
        painted(
            &mut doc,
            rect(70.0, 10.0, 50.0, 50.0),
            Paint::Gradient(stripes),
        ),
        painted(&mut doc, circle(155.0, 35.0), Paint::Gradient(rings)),
        painted(
            &mut doc,
            rect(10.0, 70.0, 50.0, 50.0),
            Paint::Gradient(glow),
        ),
        painted(&mut doc, rect(70.0, 70.0, 50.0, 50.0), Paint::Pattern(dots)),
        painted(&mut doc, circle(155.0, 95.0), Paint::Pattern(bars)),
    ];
    let framed = shape(
        doc.alloc_node_id(),
        rect(130.0, 130.0, 60.0, 20.0),
        Style {
            fill: Paint::None,
            stroke: Some(Stroke {
                paint: Paint::Gradient(diagonal),
                width: 6.0,
                ..Stroke::default()
            }),
        },
    );
    let dotted = shape(
        doc.alloc_node_id(),
        Geometry::Line {
            from: Point::new(10.0, 140.0),
            to: Point::new(90.0, 140.0),
        },
        Style {
            fill: Paint::None,
            stroke: Some(Stroke {
                paint: Paint::Solid(Rgba::rgb(0x22, 0x22, 0x22)),
                width: 4.0,
                cap: LineCap::Round,
                dash: vec![0.0, 8.0],
                ..Stroke::default()
            }),
        },
    );
    let mut faded = shape(
        doc.alloc_node_id(),
        rect(100.0, 128.0, 20.0, 24.0),
        fill(Rgba::rgb(255, 0, 0)),
    );
    faded.opacity = 0.5;
    nodes.extend([framed, dotted, faded]);
    insert(&mut doc, nodes);
    doc
}

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Rgba {
    Rgba { r, g, b, a }
}

fn stops(list: &[(f64, Rgba)]) -> Vec<GradientStop> {
    list.iter()
        .map(|&(offset, color)| GradientStop { offset, color })
        .collect()
}

fn linear(
    start: (f64, f64),
    end: (f64, f64),
    list: &[(f64, Rgba)],
    units: GradientUnits,
    spread: Spread,
) -> Gradient {
    Gradient {
        name: None,
        kind: GradientKind::Linear {
            start: Point::new(start.0, start.1),
            end: Point::new(end.0, end.1),
        },
        stops: stops(list),
        units,
        spread,
    }
}

/// Adds `gradient` as a resource.
fn put(doc: &mut Document, gradient: Gradient) -> ResourceId {
    let id = doc.alloc_resource_id();
    doc.apply(Command::PutResource {
        id,
        resource: Resource::Gradient(gradient),
        nodes: Vec::new(),
    })
    .expect("fixture gradient is valid: two sorted stops, finite geometry");
    id
}

/// Adds a pattern of solid-filled `content` shapes.
fn pattern(
    doc: &mut Document,
    tile: Rect,
    transform: Transform,
    content: Vec<(Geometry, Rgba)>,
) -> ResourceId {
    let nodes: Vec<Node> = content
        .into_iter()
        .map(|(geometry, color)| shape(doc.alloc_node_id(), geometry, fill(color)))
        .collect();
    let id = doc.alloc_resource_id();
    let resource = Resource::Pattern(Pattern {
        name: None,
        tile,
        transform,
        content: nodes.iter().map(|n| n.id).collect(),
    });
    doc.apply(Command::PutResource {
        id,
        resource,
        nodes,
    })
    .expect("fixture pattern is valid: a tile with an area, fresh content ids");
    id
}
