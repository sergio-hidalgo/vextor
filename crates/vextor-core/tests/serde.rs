//! serde round trip of every model type.

mod common;

use std::fmt::Debug;

use common::{fixture, gradient, pattern, sample_path};
use serde::Serialize;
use serde::de::DeserializeOwned;
use vextor_core::{
    Artboard, ArtboardId, ClipResource, ClipboardPayload, ColorFormat, Container, CornerRadii,
    Document, FillRule, Geometry, GradientKind, GradientStop, GradientUnits, GridSettings, IdMode,
    Image, ImageMime, ImageSource, LineCap, LineJoin, Node, NodeId, NodeKind, NodeType, Paint,
    PathNode, PathNodeRef, Point, Rect, ResourceId, Rgba, Selection, Shape, SnapSettings, Spread,
    Stroke, Style, Styling, SvgExportOptions, Swatch, ToolId, Transform, Vec2, View,
};

fn round_trip<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T) -> String {
    let json = serde_json::to_string(value).expect("serialises");
    let back: T = serde_json::from_str(&json).expect("deserialises");
    assert_eq!(&back, value, "{json}");
    json
}

#[test]
fn document_round_trips_with_its_allocator() {
    let fx = fixture();
    let json = round_trip(&fx.doc);
    let back: Document = serde_json::from_str(&json).expect("deserialises");
    assert_eq!(back.next_id(), fx.doc.next_id());
    back.validate().expect("still valid");
}

#[test]
fn ids_and_primitives() {
    round_trip(&NodeId(7));
    round_trip(&ResourceId(8));
    round_trip(&ArtboardId(9));
    round_trip(&Point::new(1.5, -2.0));
    round_trip(&Vec2::new(0.25, 3.0));
    round_trip(&Rect::new(0.0, 1.0, 2.0, 3.0));
    round_trip(&Transform([1.0, 0.5, -0.5, 1.0, 10.0, 20.0]));
}

#[test]
fn nodes_and_geometry() {
    let fx = fixture();
    for node in fx.doc.nodes.values() {
        round_trip(node);
    }
    round_trip(&Container {
        children: vec![NodeId(1)],
        clip: Some(NodeId(1)),
    });
    let geometries = [
        Geometry::Rect {
            rect: Rect::new(0.0, 0.0, 4.0, 4.0),
            radius: CornerRadii {
                tl: 1.0,
                tr: 2.0,
                br: 3.0,
                bl: 0.0,
            },
        },
        Geometry::Ellipse {
            center: Point::new(1.0, 1.0),
            rx: 2.0,
            ry: 3.0,
        },
        Geometry::Polygon {
            center: Point::new(0.0, 0.0),
            radius: 5.0,
            sides: 6,
            corner_radius: 1.0,
        },
        Geometry::Star {
            center: Point::new(0.0, 0.0),
            outer: 5.0,
            inner: 2.0,
            points: 5,
            corner_radius: 0.5,
        },
        Geometry::Line {
            from: Point::new(0.0, 0.0),
            to: Point::new(1.0, 1.0),
        },
        Geometry::Path(sample_path()),
    ];
    for g in &geometries {
        round_trip(g);
        round_trip(&Shape {
            geometry: g.clone(),
            style: Style::default(),
        });
    }
    round_trip(&CornerRadii::default());
}

#[test]
fn paths() {
    let p = sample_path();
    round_trip(&p);
    round_trip(&p.subpaths[0]);
    round_trip(&PathNode {
        corner_radius: 2.0,
        ..p.subpaths[0].nodes[1]
    });
    for kind in [NodeType::Corner, NodeType::Smooth, NodeType::Symmetric] {
        round_trip(&kind);
    }
    for rule in [FillRule::NonZero, FillRule::EvenOdd] {
        round_trip(&rule);
    }
    round_trip(&PathNodeRef {
        shape: NodeId(3),
        subpath: 1,
        index: 2,
    });
}

#[test]
fn style_and_paint() {
    for paint in [
        Paint::None,
        Paint::Solid(Rgba {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        }),
        Paint::CurrentColor,
        Paint::Gradient(ResourceId(1)),
        Paint::Pattern(ResourceId(2)),
    ] {
        round_trip(&paint);
    }
    for cap in [LineCap::Butt, LineCap::Round, LineCap::Square] {
        round_trip(&cap);
    }
    for join in [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel] {
        round_trip(&join);
    }
    let stroke = Stroke {
        dash: vec![2.0, 1.0],
        dash_offset: 0.5,
        ..Stroke::default()
    };
    round_trip(&stroke);
    round_trip(&Style {
        fill: Paint::CurrentColor,
        stroke: Some(stroke),
    });
    round_trip(&Swatch {
        name: Some("brand".into()),
        color: Rgba::rgb(30, 144, 255),
    });
}

#[test]
fn colours_are_saved_as_hex_by_default() {
    let half_blue = Rgba {
        r: 30,
        g: 144,
        b: 255,
        a: 128,
    };
    assert_eq!(round_trip(&half_blue), r##""#1e90ff80""##);
    assert_eq!(round_trip(&Rgba::rgb(30, 144, 255)), r##""#1e90ff""##);
    let fx = fixture();
    assert_eq!(fx.doc.color_format, ColorFormat::Hex);
    let json = round_trip(&fx.doc);
    assert!(
        json.contains(r##""#ff0000""##) && !json.contains(r#""r":"#),
        "{json}"
    );
}

#[test]
fn a_document_can_ask_for_rgba_colours() {
    let mut doc = fixture().doc;
    doc.color_format = ColorFormat::Rgba;
    let json = round_trip(&doc);
    assert!(json.contains(r#"{"r":255,"g":0,"b":0,"a":255}"#), "{json}");
    assert!(json.contains(r#""color_format":"rgba""#));
    assert!(!json.contains(r##""#ff0000""##));
    // the choice only applies inside that document
    assert_eq!(
        serde_json::to_string(&Rgba::BLACK).expect("serialises"),
        r##""#000000""##
    );
}

#[test]
fn colours_load_from_either_form() {
    let hex: Rgba = serde_json::from_str(r##""#1e90ff80""##).expect("hex");
    let rgba: Rgba = serde_json::from_str(r#"{"r":30,"g":144,"b":255,"a":128}"#).expect("rgba");
    assert_eq!(hex, rgba);
    for bad in [
        r##""#12345""##,
        r#"{"r":256,"g":0,"b":0,"a":0}"#,
        r#"{"r":1,"g":2,"b":3}"#,
        "17",
    ] {
        assert!(serde_json::from_str::<Rgba>(bad).is_err(), "{bad}");
    }
}

#[test]
fn files_without_a_colour_format_load_as_hex() {
    let json = serde_json::to_string(&fixture().doc).expect("serialises");
    let older = json.replace(r#""color_format":"hex","#, "");
    assert_ne!(older, json);
    let doc: Document = serde_json::from_str(&older).expect("loads");
    assert_eq!(doc.color_format, ColorFormat::Hex);
    for f in [ColorFormat::Hex, ColorFormat::Rgba] {
        round_trip(&f);
    }
}

#[test]
fn resources() {
    round_trip(&gradient());
    round_trip(&pattern(vec![NodeId(1)]));
    round_trip(&GradientKind::Radial {
        center: Point::new(0.5, 0.5),
        radius: 0.5,
        focal: Some(Point::new(0.4, 0.4)),
    });
    round_trip(&GradientStop {
        offset: 0.5,
        color: Rgba::BLACK,
    });
    for u in [GradientUnits::ObjectBoundingBox, GradientUnits::UserSpace] {
        round_trip(&u);
    }
    for s in [Spread::Pad, Spread::Reflect, Spread::Repeat] {
        round_trip(&s);
    }
}

#[test]
fn images_are_saved_as_base64() {
    let source = ImageSource::Embedded {
        mime: ImageMime::Png,
        bytes: vec![0, 1, 2, 250, 255],
    };
    let json = round_trip(&source);
    assert!(json.contains(r#""AAEC+v8=""#), "{json}");
    round_trip(&Image {
        rect: Rect::new(0.0, 0.0, 1.0, 1.0),
        source,
    });
    for m in [ImageMime::Png, ImageMime::Jpeg, ImageMime::Webp] {
        round_trip(&m);
    }
    let bad = r#"{"embedded":{"mime":"png","bytes":"not base64!"}}"#;
    assert!(serde_json::from_str::<ImageSource>(bad).is_err());
}

#[test]
fn settings_and_artboards() {
    round_trip(&GridSettings::default());
    round_trip(&SnapSettings::default());
    round_trip(&SvgExportOptions::default());
    for s in [Styling::Attributes, Styling::StyleElement] {
        round_trip(&s);
    }
    for m in [IdMode::None, IdMode::Readable, IdMode::UniquePrefixed] {
        round_trip(&m);
    }
    round_trip(&Artboard {
        id: ArtboardId(1),
        name: "icon-24".into(),
        rect: Rect::new(0.0, 0.0, 24.0, 24.0),
        background: Some(Rgba::rgb(255, 255, 255)),
    });
}

#[test]
fn editor_types() {
    for s in [
        Selection::None,
        Selection::Objects(vec![NodeId(1), NodeId(2)]),
        Selection::PathNodes(vec![PathNodeRef {
            shape: NodeId(1),
            subpath: 0,
            index: 1,
        }]),
        Selection::Segment {
            shape: NodeId(1),
            subpath: 0,
            index: 0,
        },
    ] {
        round_trip(&s);
    }
    round_trip(&ToolId::Select);
    round_trip(&View {
        zoom: 2.0,
        pan: Vec2::new(10.0, -5.0),
    });
    let node = Node::new(NodeId(1), NodeKind::Group(Container::default()));
    round_trip(&ClipboardPayload {
        nodes: vec![node.clone()],
        resources: vec![ClipResource {
            id: ResourceId(7),
            resource: pattern(vec![NodeId(1)]),
            content: vec![node],
        }],
    });
}
