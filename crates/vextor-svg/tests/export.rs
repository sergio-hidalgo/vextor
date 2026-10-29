//! SVG export (spec 12): one snapshot per mapping row and per option that
//! changes the output. Review changes with `cargo insta review`.

mod common;

use common::*;
use vextor_core::{
    Container, FillRule, FontSpec, Geometry, Gradient, GradientKind, GradientStop, GradientUnits,
    IdMode, Image, ImageMime, ImageSource, LineCap, LineJoin, Node, NodeKind, Paint, Pattern, Rect,
    Resource, Rgba, Spread, Stroke, Style, Styling, Text, TextAlign, TextFrame, Transform,
};
use vextor_svg::{Format, Scope, export};

/// An artboard away from the origin with one of each shape kind.
fn shapes() -> Doc {
    let mut d = Doc::new(100.0, 50.0, 120.0, 60.0);
    d.shape(rect(100.0, 50.0, 20.0, 10.0, 0.0), fill(RED));
    d.shape(rect(130.0, 50.0, 20.0, 10.0, 3.0), fill(BLUE));
    d.shape(
        Geometry::Ellipse {
            center: point(170.0, 60.0),
            rx: 8.0,
            ry: 8.0,
        },
        Style::default(),
    );
    d.shape(
        Geometry::Ellipse {
            center: point(200.0, 60.0),
            rx: 10.0,
            ry: 5.5,
        },
        Style::default(),
    );
    d.shape(
        Geometry::Line {
            from: point(100.0, 80.0),
            to: point(140.0, 80.0),
        },
        stroked(Paint::Solid(Rgba::BLACK), 2.0),
    );
    d.shape(
        Geometry::Polygon {
            center: point(160.0, 90.0),
            radius: 10.0,
            sides: 6,
            corner_radius: 0.0,
        },
        Style::default(),
    );
    d.shape(
        Geometry::Star {
            center: point(190.0, 90.0),
            outer: 10.0,
            inner: 5.0,
            points: 5,
            corner_radius: 0.0,
        },
        Style::default(),
    );
    d.shape(
        polygon_path(
            &[(100.0, 95.0), (120.0, 95.0), (120.0, 105.0), (100.0, 105.0)],
            FillRule::EvenOdd,
        ),
        Style::default(),
    );
    d
}

#[test]
fn shapes_move_to_the_artboard_origin() {
    insta::assert_snapshot!(shapes().svg());
}

#[test]
fn paints_and_strokes() {
    let mut d = Doc::new(0.0, 0.0, 64.0, 32.0);
    d.shape(
        rect(0.0, 0.0, 10.0, 10.0, 0.0),
        fill(Rgba { a: 128, ..RED }),
    );
    d.shape(
        rect(12.0, 0.0, 10.0, 10.0, 0.0),
        Style {
            fill: Paint::CurrentColor,
            stroke: None,
        },
    );
    d.shape(
        rect(24.0, 0.0, 10.0, 10.0, 0.0),
        Style {
            fill: Paint::None,
            stroke: Some(Stroke {
                paint: Paint::Solid(Rgba { a: 51, ..BLUE }),
                width: 1.5,
                cap: LineCap::Round,
                join: LineJoin::Bevel,
                ..Stroke::default()
            }),
        },
    );
    d.shape(
        rect(36.0, 0.0, 10.0, 10.0, 0.0),
        Style {
            fill: Paint::None,
            stroke: Some(Stroke {
                paint: Paint::Solid(Rgba::BLACK),
                width: 1.0,
                cap: LineCap::Square,
                miter_limit: 10.0,
                dash: vec![4.0, 2.0],
                dash_offset: 1.0,
                ..Stroke::default()
            }),
        },
    );
    // Zero-width and all-zero dashes draw like the canvas: nothing / solid.
    d.shape(
        rect(48.0, 0.0, 10.0, 10.0, 0.0),
        stroked(Paint::Solid(RED), 0.0),
    );
    d.shape(
        rect(48.0, 12.0, 10.0, 10.0, 0.0),
        Style {
            fill: Paint::None,
            stroke: Some(Stroke {
                paint: Paint::Solid(RED),
                dash: vec![0.0, 0.0],
                ..Stroke::default()
            }),
        },
    );
    insta::assert_snapshot!(d.svg());
}

#[test]
fn groups_transforms_opacity_and_hidden_objects() {
    let mut d = Doc::new(0.0, 0.0, 100.0, 100.0);
    let (g, a, b, hidden, inner, nested) = (d.id(), d.id(), d.id(), d.id(), d.id(), d.id());
    let mut group = group_node(g, vec![a, b, hidden, inner]);
    group.name = Some("Close Button".to_owned());
    group.transform = moved(10.0, 20.0);
    let a_node = shape_node(a, rect(0.0, 0.0, 10.0, 10.0, 0.0), fill(RED));
    let mut b_node = shape_node(b, rect(20.0, 0.0, 10.0, 10.0, 0.0), fill(BLUE));
    // A quarter turn about the square's centre.
    b_node.transform = Transform::translate(-25.0, -5.0)
        .then(Transform([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]))
        .then(Transform::translate(25.0, 5.0));
    b_node.opacity = 0.5;
    let mut hidden_node = shape_node(hidden, rect(40.0, 0.0, 10.0, 10.0, 0.0), fill(RED));
    hidden_node.visible = false;
    // A scaled group keeps its transform; its child keeps coordinates.
    let mut inner_node = group_node(inner, vec![nested]);
    inner_node.transform = Transform::scale(2.0, 2.0);
    let nested_node = shape_node(nested, rect(5.0, 20.0, 4.0, 4.0, 0.0), Style::default());
    d.add(
        group,
        vec![a_node, b_node, hidden_node, inner_node, nested_node],
    );
    // An empty group is left out.
    let empty = d.id();
    d.add(group_node(empty, Vec::new()), Vec::new());
    insta::assert_snapshot!(d.svg());
}

#[test]
fn a_layer_with_opacity_becomes_a_group() {
    let mut d = Doc::new(0.0, 0.0, 20.0, 20.0);
    d.shape(rect(0.0, 0.0, 10.0, 10.0, 0.0), fill(RED));
    let layer = d.doc.layers[0];
    d.edit(layer, |n| {
        n.opacity = 0.25;
        n.name = Some("Layer 1".to_owned());
    });
    insta::assert_snapshot!(d.svg());
}

fn gradient(kind: GradientKind, units: GradientUnits, spread: Spread) -> Resource {
    Resource::Gradient(Gradient {
        name: None,
        kind,
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: RED,
            },
            GradientStop {
                offset: 0.5,
                color: Rgba { a: 128, ..BLUE },
            },
            GradientStop {
                offset: 1.0,
                color: Rgba::BLACK,
            },
        ],
        units,
        spread,
    })
}

#[test]
fn gradients_are_defined_once_in_order_of_first_use() {
    let mut d = Doc::new(0.0, 0.0, 100.0, 40.0);
    let linear = d.resource(gradient(
        GradientKind::Linear {
            start: point(0.0, 0.0),
            end: point(1.0, 0.0),
        },
        GradientUnits::ObjectBoundingBox,
        Spread::Pad,
    ));
    let user = d.resource(gradient(
        GradientKind::Linear {
            start: point(40.0, 0.0),
            end: point(60.0, 20.0),
        },
        GradientUnits::UserSpace,
        Spread::Reflect,
    ));
    let mut radial = gradient(
        GradientKind::Radial {
            center: point(0.5, 0.5),
            radius: 0.5,
            focal: Some(point(0.3, 0.3)),
        },
        GradientUnits::ObjectBoundingBox,
        Spread::Repeat,
    );
    if let Resource::Gradient(g) = &mut radial {
        g.name = Some("Sunset glow".to_owned());
    }
    let radial = d.resource(radial);
    let paint = |p| Style {
        fill: p,
        stroke: None,
    };
    d.shape(
        rect(0.0, 0.0, 20.0, 20.0, 0.0),
        paint(Paint::Gradient(user)),
    );
    d.shape(
        rect(30.0, 0.0, 20.0, 20.0, 0.0),
        paint(Paint::Gradient(linear)),
    );
    d.shape(
        rect(60.0, 0.0, 20.0, 20.0, 0.0),
        paint(Paint::Gradient(radial)),
    );
    // Used again: no second definition.
    d.shape(
        rect(30.0, 25.0, 20.0, 10.0, 0.0),
        stroked(Paint::Gradient(linear), 2.0),
    );
    // A user-space gradient keeps the object's translation as a transform.
    let moved_user = d.shape(
        rect(0.0, 0.0, 20.0, 10.0, 0.0),
        paint(Paint::Gradient(user)),
    );
    d.edit(moved_user, |n| n.transform = moved(0.0, 25.0));
    insta::assert_snapshot!(d.svg());
}

#[test]
fn patterns_hold_their_tile() {
    let mut d = Doc::new(0.0, 0.0, 40.0, 40.0);
    let dot = d.id();
    let dot_node = shape_node(
        dot,
        Geometry::Ellipse {
            center: point(2.0, 2.0),
            rx: 1.0,
            ry: 1.0,
        },
        fill(RED),
    );
    let pattern = d.doc.alloc_resource_id();
    d.doc
        .apply(vextor_core::Command::PutResource {
            id: pattern,
            resource: Resource::Pattern(Pattern {
                name: None,
                tile: Rect::new(0.0, 0.0, 4.0, 4.0),
                transform: Transform([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]),
                content: vec![dot],
            }),
            nodes: vec![dot_node],
        })
        .expect("adds the pattern");
    d.shape(
        rect(0.0, 0.0, 40.0, 40.0, 0.0),
        Style {
            fill: Paint::Pattern(pattern),
            stroke: None,
        },
    );
    insta::assert_snapshot!(d.svg());
}

#[test]
fn clip_groups_clip_with_a_clip_path() {
    let mut d = Doc::new(0.0, 0.0, 40.0, 40.0);
    let (g, clip, a) = (d.id(), d.id(), d.id());
    let mut group = Node::new(
        g,
        NodeKind::Group(Container {
            children: vec![a, clip],
            clip: Some(clip),
        }),
    );
    group.transform = moved(5.0, 5.0);
    let clip_node = shape_node(
        clip,
        polygon_path(&[(0.0, 0.0), (20.0, 0.0), (0.0, 20.0)], FillRule::EvenOdd),
        Style::default(),
    );
    let a_node = shape_node(a, rect(0.0, 0.0, 30.0, 30.0, 0.0), fill(RED));
    d.add(group, vec![a_node, clip_node]);
    insta::assert_snapshot!(d.svg());
}

#[test]
fn images_are_embedded_data_uris() {
    let mut d = Doc::new(0.0, 0.0, 20.0, 20.0);
    let id = d.id();
    let image = Node::new(
        id,
        NodeKind::Image(Image {
            rect: Rect::new(2.0, 2.0, 16.0, 8.0),
            source: ImageSource::Embedded {
                mime: ImageMime::Png,
                bytes: vec![0x89, b'P', b'N', b'G'],
            },
        }),
    );
    d.add(image, Vec::new());
    insta::assert_snapshot!(d.svg());
}

fn label() -> Text {
    Text {
        size: 18.0,
        align: TextAlign::Centre,
        font: FontSpec {
            family: "Noto Sans".to_owned(),
            weight: 700,
            italic: false,
        },
        style: Style {
            fill: Paint::Solid(Rgba::rgb(255, 255, 255)),
            stroke: None,
        },
        frame: Some(TextFrame {
            padding: [8.0, 16.0, 8.0, 16.0],
            radius: vextor_core::CornerRadii {
                tl: 6.0,
                tr: 6.0,
                br: 6.0,
                bl: 6.0,
            },
            fill: Paint::Solid(Rgba::rgb(30, 144, 255)),
            stroke: None,
        }),
        ..Text::new("Buy\nnow", "Noto Sans")
    }
}

fn text_doc(text: Text) -> Doc {
    let mut d = Doc::new(0.0, 0.0, 120.0, 80.0);
    let id = d.id();
    let mut node = Node::new(id, NodeKind::Text(text));
    node.transform = moved(20.0, 10.0);
    node.name = Some("CTA".to_owned());
    d.add(node, Vec::new());
    d
}

#[test]
fn text_is_live_with_its_frame_behind() {
    insta::assert_snapshot!(text_doc(label()).svg());
}

#[test]
fn text_as_outlines_needs_no_font() {
    let mut d = text_doc(Text {
        frame: None,
        ..label()
    });
    d.options(|o| o.text_as_outlines = true);
    let svg = d.svg();
    assert!(!svg.contains("<text"), "{svg}");
    insta::assert_snapshot!(svg);
}

#[test]
fn text_spacing_style_and_alignment() {
    let text = Text {
        letter_spacing: 1.5,
        align: TextAlign::Right,
        font: FontSpec {
            family: "Noto Sans".to_owned(),
            weight: 400,
            italic: true,
        },
        ..Text::new("Hi  there", "Noto Sans")
    };
    insta::assert_snapshot!(text_doc(text).svg());
}

#[test]
fn a_background_is_the_first_painted_child() {
    let mut d = shapes();
    let mut artboards = d.doc.artboards.clone();
    artboards[0].background = Some(Rgba {
        a: 200,
        ..Rgba::rgb(250, 250, 250)
    });
    d.doc
        .apply(vextor_core::Command::SetArtboards(artboards))
        .expect("valid");
    let svg = d.svg();
    let bg = svg.find(r#"<rect width="100%""#).expect("background");
    assert!(bg < svg.find("<rect width=\"20\"").expect("first shape"));
    insta::assert_snapshot!(svg);
}

#[test]
fn objects_on_another_artboard_are_left_out() {
    let mut d = Doc::new(0.0, 0.0, 24.0, 24.0);
    d.shape(rect(2.0, 2.0, 20.0, 20.0, 0.0), fill(RED));
    d.shape(rect(100.0, 2.0, 20.0, 20.0, 0.0), fill(BLUE));
    let svg = d.svg();
    assert!(!svg.contains("#00f"), "{svg}");
    insta::assert_snapshot!(svg);
}

/// Shapes with names, for the id and styling options.
fn named() -> Doc {
    let mut d = Doc::new(0.0, 0.0, 40.0, 20.0);
    for (i, name) in ["Dot", "Dot", "24 px"].into_iter().enumerate() {
        let id = d.shape(
            rect(
                f64::from(u8::try_from(i).expect("small")) * 12.0,
                0.0,
                10.0,
                10.0,
                0.0,
            ),
            fill(if i == 2 { BLUE } else { RED }),
        );
        d.edit(id, |n| n.name = Some(name.to_owned()));
    }
    d
}

#[test]
fn ids_readable_none_and_unique() {
    let mut d = named();
    let readable = d.svg();
    d.options(|o| o.ids = IdMode::None);
    let none = d.svg();
    d.options(|o| o.ids = IdMode::UniquePrefixed);
    let unique = d.svg();
    insta::assert_snapshot!(format!("{readable}\n{none}\n{unique}"));
}

#[test]
fn without_ids_only_definitions_keep_one_and_it_is_numbered() {
    let mut d = named();
    let mut sunset = gradient(
        GradientKind::Linear {
            start: point(0.0, 0.0),
            end: point(1.0, 0.0),
        },
        GradientUnits::ObjectBoundingBox,
        Spread::Pad,
    );
    if let Resource::Gradient(g) = &mut sunset {
        g.name = Some("Sunset".to_owned());
    }
    let sunset = d.resource(sunset);
    let id = d.doc.paint_order()[1];
    d.edit(id, |n| {
        if let NodeKind::Shape(s) = &mut n.kind {
            s.style.fill = Paint::Gradient(sunset);
        }
    });
    assert!(
        d.svg().contains(r#"id="sunset""#),
        "named with ids from names"
    );
    d.options(|o| o.ids = IdMode::None);
    let svg = d.svg();
    assert_eq!(svg.matches(" id=").count(), 1, "{svg}");
    assert!(svg.contains(r#"<linearGradient id="gradient-1">"#), "{svg}");
    assert!(svg.contains(r#"fill="url(#gradient-1)""#), "{svg}");
}

#[test]
fn style_element_moves_paint_into_classes() {
    let mut d = named();
    d.options(|o| {
        o.styling = Styling::StyleElement;
        o.ids = IdMode::UniquePrefixed;
    });
    insta::assert_snapshot!(d.svg());
}

#[test]
fn precision_size_and_minify_options() {
    let mut d = Doc::new(0.0, 0.0, 24.0, 24.0);
    d.shape(
        Geometry::Ellipse {
            center: point(12.345_678, 11.987_654),
            rx: 7.25,
            ry: 3.333_333,
        },
        fill(RED),
    );
    d.options(|o| o.precision = 0);
    let p0 = d.svg();
    d.options(|o| o.precision = 4);
    let p4 = d.svg();
    d.options(|o| {
        o.responsive = false;
        o.minify = true;
    });
    let sized = d.svg();
    assert!(!sized.contains('\n'), "{sized}");
    insta::assert_snapshot!(format!("{p0}\n{p4}\n{sized}"));
}

#[test]
fn current_color_replaces_the_dominant_colour() {
    let mut d = Doc::new(0.0, 0.0, 24.0, 24.0);
    d.shape(rect(0.0, 0.0, 4.0, 4.0, 0.0), fill(BLUE));
    d.shape(
        rect(6.0, 0.0, 4.0, 4.0, 0.0),
        stroked(Paint::Solid(RED), 2.0),
    );
    d.shape(rect(12.0, 0.0, 4.0, 4.0, 0.0), fill(Rgba { a: 128, ..RED }));
    d.options(|o| o.current_color = true);
    let svg = d.svg();
    assert!(!svg.contains("#f00"), "{svg}");
    assert!(svg.contains("#00f"), "{svg}");
    insta::assert_snapshot!(svg);
}

#[test]
fn inline_html_jsx_and_css_data_uri() {
    let mut d = Doc::new(0.0, 0.0, 24.0, 24.0);
    d.shape(
        rect(2.0, 2.0, 20.0, 20.0, 4.0),
        stroked(Paint::CurrentColor, 2.0),
    );
    let text = Text::new("{x}", "Noto Sans");
    let id = d.id();
    let mut node = Node::new(id, NodeKind::Text(text));
    node.transform = moved(4.0, 4.0);
    d.add(node, Vec::new());
    d.options(|o| o.styling = Styling::StyleElement);
    let html = d.export(Format::Html).code;
    let jsx = d.export(Format::Jsx).code;
    let css = d.export(Format::CssDataUri).code;
    assert!(!html.contains("xmlns"), "{html}");
    assert!(
        css.starts_with("url(\"data:image/svg+xml,%3Csvg%20xmlns="),
        "{css}"
    );
    insta::assert_snapshot!(format!("{html}\n{jsx}\n{css}"));
}

#[test]
fn a_selection_is_framed_by_what_it_paints() {
    let mut d = shapes();
    let ids: Vec<_> = d.doc.paint_order().into_iter().skip(1).step_by(4).collect();
    let svg = export(
        &d.doc,
        &fonts(),
        Scope::Selection(&ids),
        Format::Svg,
        &d.doc.export,
    )
    .expect("visible")
    .code;
    insta::assert_snapshot!(svg);
    d.edit(ids[0], |n| n.visible = false);
    let nothing = export(
        &d.doc,
        &fonts(),
        Scope::Selection(&ids[..1]),
        Format::Svg,
        &d.doc.export,
    );
    assert_eq!(nothing, None);
}

#[test]
fn a_selected_group_and_its_child_are_written_once() {
    let mut d = Doc::new(0.0, 0.0, 40.0, 40.0);
    let (g, a) = (d.id(), d.id());
    let a_node = shape_node(a, rect(0.0, 0.0, 10.0, 10.0, 0.0), fill(RED));
    d.add(group_node(g, vec![a]), vec![a_node]);
    let svg = export(
        &d.doc,
        &fonts(),
        Scope::Selection(&[a, g]),
        Format::Svg,
        &d.doc.export,
    )
    .expect("visible")
    .code;
    assert_eq!(svg.matches("<rect").count(), 1, "{svg}");
}

#[test]
fn spans_point_at_each_drawn_object() {
    let d = shapes();
    let e = d.export(Format::Jsx);
    let ids = d.doc.paint_order();
    let ellipse = ids[4];
    let (_, span) = e
        .spans
        .iter()
        .find(|(id, _)| *id == ellipse)
        .expect("drawn");
    assert_eq!(
        &e.code[span.clone()],
        r#"<ellipse cx="100" cy="10" rx="10" ry="5.5"/>"#
    );
    assert_eq!(e.spans.len(), 8);
}

#[test]
fn output_is_deterministic() {
    let d = shapes();
    assert_eq!(d.svg(), d.svg());
    let again = export(
        &d.doc.clone(),
        &fonts(),
        Scope::Artboard(d.artboard),
        Format::Svg,
        &d.doc.export,
    )
    .expect("exists");
    assert_eq!(again.code, d.svg());
}

#[test]
fn a_missing_artboard_exports_nothing() {
    let d = shapes();
    let other = vextor_core::ArtboardId(9_999);
    let none = export(
        &d.doc,
        &fonts(),
        Scope::Artboard(other),
        Format::Svg,
        &d.doc.export,
    );
    assert_eq!(none, None);
}

#[test]
fn a_saved_preview_never_reaches_the_export() {
    let mut d = shapes();
    let before = d.svg();
    let mut bytes = Vec::new();
    vextor_core::to_writer_with_preview(&mut bytes, &d.doc, Some(b"PREVIEW-PICTURE"))
        .expect("encodes");
    d.doc = vextor_core::from_bytes(&bytes).expect("loads");
    let after = d.svg();
    assert_eq!(after, before);
    assert!(!after.contains("preview"));
    for format in [Format::Html, Format::Jsx, Format::CssDataUri] {
        assert_eq!(d.export(format).code, shapes().export(format).code);
    }
}
