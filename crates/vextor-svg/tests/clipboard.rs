//! The clipboard SVG: exact output for each supported feature.

use vextor_core::{
    Command, Container, CornerRadii, Document, Geometry, Gradient, GradientKind, GradientStop,
    GradientUnits, LineCap, Node, NodeId, NodeKind, Paint, Point, Rect, Resource, Rgba, Shape,
    Spread, Stroke, Style, Transform,
};
use vextor_geom::Fonts;
use vextor_svg::clipboard_svg;

fn shape(id: NodeId, geometry: Geometry, style: Style) -> Node {
    Node::new(id, NodeKind::Shape(Shape { geometry, style }))
}

fn rect(x: f64, y: f64, width: f64, height: f64, radius: f64) -> Geometry {
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

/// Inserts `nodes` into the first layer and copies the roots.
fn copy(doc: &mut Document, nodes: Vec<Node>, roots: &[NodeId]) -> String {
    let layer = doc.layers[0];
    doc.apply(Command::Insert {
        parent: Some(layer),
        index: 0,
        nodes,
    })
    .expect("inserts");
    let payload = doc.copy(roots).expect("copies");
    clipboard_svg(&payload, &Fonts::default()).expect("something visible")
}

#[test]
fn shapes_paints_and_strokes() {
    let mut doc = Document::new("t");
    let (a, b, c) = (
        doc.alloc_node_id(),
        doc.alloc_node_id(),
        doc.alloc_node_id(),
    );
    let red = Style {
        fill: Paint::Solid(Rgba::rgb(255, 0, 0)),
        stroke: None,
    };
    let outline = Style {
        fill: Paint::None,
        stroke: Some(Stroke {
            paint: Paint::CurrentColor,
            width: 2.0,
            cap: LineCap::Round,
            ..Stroke::default()
        }),
    };
    let half = Style {
        fill: Paint::Solid(Rgba {
            r: 0x12,
            g: 0x34,
            b: 0x56,
            a: 128,
        }),
        stroke: None,
    };
    let svg = copy(
        &mut doc,
        vec![
            shape(a, rect(0.0, 0.0, 24.0, 24.0, 4.0), red),
            shape(
                b,
                Geometry::Ellipse {
                    center: Point::new(12.0, 12.0),
                    rx: 6.0,
                    ry: 6.0,
                },
                outline,
            ),
            shape(
                c,
                Geometry::Line {
                    from: Point::new(0.0, 0.0),
                    to: Point::new(24.0, 24.5),
                },
                half,
            ),
        ],
        &[a, b, c],
    );
    assert_eq!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24.5">
  <rect x="0" y="0" width="24" height="24" rx="4" fill="#f00"/>
  <circle cx="12" cy="12" r="6" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
  <line x1="0" y1="0" x2="24" y2="24.5"/>
</svg>
"##
    );
}

#[test]
fn groups_transforms_opacity_and_clips() {
    let mut doc = Document::new("t");
    let (g, clip, r) = (
        doc.alloc_node_id(),
        doc.alloc_node_id(),
        doc.alloc_node_id(),
    );
    let mut group = Node::new(
        g,
        NodeKind::Group(Container {
            children: vec![clip, r],
            clip: Some(clip),
        }),
    );
    group.transform = Transform::translate(10.0, 5.0);
    group.opacity = 0.5;
    let svg = copy(
        &mut doc,
        vec![
            group,
            shape(clip, rect(0.0, 0.0, 8.0, 8.0, 0.0), Style::default()),
            shape(r, rect(0.0, 0.0, 20.0, 20.0, 0.0), Style::default()),
        ],
        &[g],
    );
    assert_eq!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="10 5 8 8">
  <defs>
    <clipPath id="clip-1"><rect x="0" y="0" width="8" height="8"/></clipPath>
  </defs>
  <g transform="translate(10 5)" opacity=".5" clip-path="url(#clip-1)">
    <rect x="0" y="0" width="20" height="20"/>
  </g>
</svg>
"#
    );
}

#[test]
fn star_becomes_a_path_and_gradients_their_first_stop() {
    let mut doc = Document::new("t");
    let gid = doc.alloc_resource_id();
    doc.apply(Command::PutResource {
        id: gid,
        resource: Resource::Gradient(Gradient {
            name: None,
            kind: GradientKind::Linear {
                start: Point::new(0.0, 0.0),
                end: Point::new(1.0, 0.0),
            },
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: Rgba::rgb(0, 0x80, 0xff),
                },
                GradientStop {
                    offset: 1.0,
                    color: Rgba::BLACK,
                },
            ],
            units: GradientUnits::ObjectBoundingBox,
            spread: Spread::Pad,
        }),
        nodes: vec![],
    })
    .expect("adds gradient");
    let s = doc.alloc_node_id();
    let star = Geometry::Polygon {
        center: Point::new(10.0, 10.0),
        radius: 10.0,
        sides: 4,
        corner_radius: 0.0,
    };
    let style = Style {
        fill: Paint::Gradient(gid),
        stroke: None,
    };
    let svg = copy(&mut doc, vec![shape(s, star, style)], &[s]);
    assert!(
        svg.contains(r##"<path d="M10 0L20 10L10 20L0 10Z" fill="#0080ff"/>"##),
        "{svg}"
    );
}

#[test]
fn nothing_visible_gives_no_svg() {
    let mut doc = Document::new("t");
    let layer = doc.layers[0];
    let a = doc.alloc_node_id();
    let mut hidden = shape(a, rect(0.0, 0.0, 1.0, 1.0, 0.0), Style::default());
    hidden.visible = false;
    doc.apply(Command::Insert {
        parent: Some(layer),
        index: 0,
        nodes: vec![hidden],
    })
    .expect("inserts");
    assert_eq!(
        clipboard_svg(&doc.copy(&[a]).expect("copies"), &Fonts::default()),
        None
    );
}
