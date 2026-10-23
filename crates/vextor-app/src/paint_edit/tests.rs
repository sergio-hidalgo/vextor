use vextor_core::{
    Command, Document, Geometry, GradientKind, LineCap, Node, NodeId, NodeKind, Paint, Point, Rect,
    Resource, Rgba, Shape, Stroke, Style, Text,
};

use super::*;
use crate::new_doc::new_document;

/// A document with a red rect and a text label; their ids.
fn doc() -> (Document, NodeId, NodeId) {
    let mut doc = new_document("paint", 100.0, 100.0);
    let rect = doc.alloc_node_id();
    let text = doc.alloc_node_id();
    let nodes = vec![
        Node::new(
            rect,
            NodeKind::Shape(Shape {
                geometry: Geometry::Rect {
                    rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                    radius: vextor_core::CornerRadii::default(),
                },
                style: Style {
                    fill: Paint::Solid(Rgba::rgb(255, 0, 0)),
                    stroke: None,
                },
            }),
        ),
        Node::new(text, NodeKind::Text(Text::new("Hi", "Noto Sans"))),
    ];
    doc.apply(Command::Insert {
        parent: Some(doc.layers[0]),
        index: 0,
        nodes,
    })
    .expect("valid nodes");
    (doc, rect, text)
}

/// Applies the command `build` makes from `doc`.
fn run(doc: &mut Document, build: impl FnOnce(&Document) -> Option<Command>) {
    let cmd = build(doc).expect("something changes");
    doc.apply(cmd).expect("command applies");
}

#[test]
fn fill_applies_to_shapes_and_text_glyphs() {
    let (mut d, rect, text) = doc();
    let blue = Paint::Solid(Rgba::rgb(0, 0, 255));
    run(&mut d, |d| {
        command(d, &[rect, text], Target::Fill, &Edit::Paint(blue))
    });
    for id in [rect, text] {
        assert_eq!(paint(&d.nodes[&id], Target::Fill), Some(blue));
    }
    // The same paint again changes nothing.
    assert_eq!(command(&d, &[rect], Target::Fill, &Edit::Paint(blue)), None);
}

#[test]
fn a_stroke_is_added_by_its_paint_and_removed_by_none() {
    let (mut d, rect, _) = doc();
    let black = Paint::Solid(Rgba::BLACK);
    run(&mut d, |d| {
        command(d, &[rect], Target::Stroke, &Edit::Paint(black))
    });
    let s = stroke(&d.nodes[&rect], Target::Stroke).expect("a stroke");
    assert_eq!((s.paint, s.width), (black, 1.0));
    run(&mut d, |d| {
        command(d, &[rect], Target::Stroke, &Edit::Width(4.0))
    });
    run(&mut d, |d| {
        command(d, &[rect], Target::Stroke, &Edit::Cap(LineCap::Round))
    });
    let s = stroke(&d.nodes[&rect], Target::Stroke).expect("a stroke");
    assert_eq!((s.width, s.cap), (4.0, LineCap::Round));
    run(&mut d, |d| {
        command(d, &[rect], Target::Stroke, &Edit::Paint(Paint::None))
    });
    assert_eq!(stroke(&d.nodes[&rect], Target::Stroke), None);
    // Width without a stroke does nothing.
    assert_eq!(
        command(&d, &[rect], Target::Stroke, &Edit::Width(2.0)),
        None
    );
}

#[test]
fn background_and_border_only_apply_to_text_and_make_the_frame() {
    let (mut d, rect, text) = doc();
    assert_eq!(targets(&d, &[rect, text], Target::Background), vec![text]);
    let grey = Paint::Solid(Rgba::rgb(0xe7, 0xe7, 0xe7));
    run(&mut d, |d| {
        command(d, &[rect, text], Target::Background, &Edit::Paint(grey))
    });
    let NodeKind::Text(t) = &d.nodes[&text].kind else {
        unreachable!("text")
    };
    let frame = t.frame.as_ref().expect("frame made");
    assert_eq!((frame.fill, frame.padding), (grey, empty_frame().padding));
    // Turning the background off again removes the frame.
    let none = Edit::Paint(Paint::None);
    run(&mut d, |d| command(d, &[text], Target::Background, &none));
    let NodeKind::Text(t) = &d.nodes[&text].kind else {
        unreachable!("text")
    };
    assert_eq!(t.frame, None);
}

#[test]
fn a_new_gradient_is_one_undo_step_with_the_paint() {
    let (mut d, rect, _) = doc();
    let before = d.clone();
    let g = new_gradient(GradientShape::Radial, Rgba::rgb(255, 0, 0), None);
    let cmd = command_with_resource(
        &mut d,
        &[rect],
        Target::Fill,
        Resource::Gradient(g),
        Paint::Gradient,
    );
    let mut h = vextor_core::History::new();
    h.apply(&mut d, cmd, &vextor_core::Selection::None)
        .expect("applies");
    let Some(Paint::Gradient(id)) = paint(&d.nodes[&rect], Target::Fill) else {
        panic!("gradient fill");
    };
    assert_eq!(users(&d, id), 1);
    assert!(h.undo(&mut d).is_some());
    assert_eq!(d, before, "undo removes the gradient and restores the fill");
}

#[test]
fn new_gradients_fill_the_box_and_keep_stops_they_copy() {
    let red = Rgba::rgb(255, 0, 0);
    let linear = new_gradient(GradientShape::Linear, red, None);
    assert_eq!(linear.stops[0].color, red);
    assert!(
        matches!(linear.kind, GradientKind::Linear { start, .. } if start == Point::new(0.0, 0.5))
    );
    let mut with_spread = linear.clone();
    with_spread.spread = vextor_core::Spread::Repeat;
    let radial = new_gradient(GradientShape::Radial, Rgba::BLACK, Some(&with_spread));
    assert_eq!(radial.stops, linear.stops);
    assert_eq!(radial.spread, vextor_core::Spread::Repeat);
}

#[test]
fn stops_are_added_moved_and_removed_in_order() {
    let g = new_gradient(GradientShape::Linear, Rgba::rgb(0, 0, 0), None);
    let (g, i) = add_stop(&g, 0.5);
    assert_eq!(i, 1);
    // Halfway from black to white.
    assert_eq!(g.stops[1].color, Rgba::rgb(128, 128, 128));
    let (g, i) = move_stop(&g, 1, 2.0).expect("stop exists");
    assert_eq!((i, g.stops[i].offset), (2, 1.0), "clamped and re-sorted");
    assert!(g.stops.windows(2).all(|w| w[0].offset <= w[1].offset));
    let g = remove_stop(&g, 0).expect("three stops");
    assert_eq!(remove_stop(&g, 0), None, "two stops stay");
    assert_eq!(move_stop(&g, 9, 0.5), None);
}

#[test]
fn dash_presets_round_trip_and_custom_text_parses() {
    let mut s = Stroke {
        width: 2.0,
        ..Stroke::default()
    };
    assert_eq!(dash_preset(&s), DashPreset::Solid);
    for preset in [DashPreset::Dashed, DashPreset::Dotted] {
        let (dash, cap) = preset_dash(preset, s.width);
        s.dash = dash;
        s.cap = cap.unwrap_or(LineCap::Butt);
        assert_eq!(dash_preset(&s), preset);
    }
    s.dash = vec![5.0, 1.0];
    assert_eq!(dash_preset(&s), DashPreset::Custom);
    assert_eq!(parse_dash("6 3"), Ok(vec![6.0, 3.0]));
    assert_eq!(parse_dash(" 4, 2,1 "), Ok(vec![4.0, 2.0, 1.0]));
    assert_eq!(parse_dash(""), Ok(vec![]));
    assert!(parse_dash("4 -2").is_err());
    assert!(parse_dash("four").is_err());
    assert!(parse_dash(&"1 ".repeat(17)).is_err());
    assert_eq!(format_dash(&[6.0, 2.5]), "6 2.5");
}

#[test]
fn tile_transform_parts_round_trip() {
    let t = tile_transform(1.5, 30.0, (4.0, -2.0));
    let (scale, degrees, offset) = tile_parts(t);
    assert!((scale - 1.5).abs() < 1e-9);
    assert!((degrees - 30.0).abs() < 1e-9);
    assert_eq!(offset, (4.0, -2.0));
}

#[test]
fn opacity_applies_to_every_object_keeping_other_props() {
    let (mut d, rect, text) = doc();
    run(&mut d, |d| opacity_command(d, &[rect, text], 0.25));
    assert_eq!(d.nodes[&rect].opacity, 0.25);
    assert_eq!(d.nodes[&text].opacity, 0.25);
    assert!(d.nodes[&rect].visible);
    assert_eq!(opacity_command(&d, &[rect], 0.25), None);
}

#[test]
fn gradient_points_move_like_their_handles() {
    let linear = new_gradient(GradientShape::Linear, Rgba::BLACK, None);
    let points = gradient_points(&linear);
    assert_eq!(points[0], (GradientPoint::Start, Point::new(0.0, 0.5)));
    let moved = move_point(&linear, GradientPoint::End, Point::new(1.0, 1.0)).expect("linear end");
    assert!(matches!(moved.kind, GradientKind::Linear { end, .. } if end == Point::new(1.0, 1.0)));
    assert_eq!(
        move_point(&linear, GradientPoint::Focal, Point::new(0.0, 0.0)),
        None
    );

    let radial = new_gradient(GradientShape::Radial, Rgba::BLACK, None);
    let r = move_point(&radial, GradientPoint::Radius, Point::new(0.5, 0.8)).expect("radius");
    assert!(matches!(r.kind, GradientKind::Radial { radius, .. } if (radius - 0.3).abs() < 1e-9));
    let f = move_point(&r, GradientPoint::Focal, Point::new(0.4, 0.5)).expect("focal");
    let c = move_point(&f, GradientPoint::Center, Point::new(0.6, 0.5)).expect("centre");
    let GradientKind::Radial { focal, .. } = c.kind else {
        unreachable!("radial")
    };
    let focal = focal.expect("focal kept");
    assert!(
        (focal.x - 0.5).abs() < 1e-9,
        "the focal point moves with the centre"
    );
    assert_eq!(
        move_point(&radial, GradientPoint::Center, Point::new(f64::NAN, 0.0)),
        None
    );
}

#[test]
fn each_paint_uses_its_swatch_list() {
    assert_eq!(Target::Fill.swatch_kind(), SwatchKind::Fill);
    assert_eq!(Target::Background.swatch_kind(), SwatchKind::Fill);
    assert_eq!(Target::Stroke.swatch_kind(), SwatchKind::Stroke);
    assert_eq!(Target::Border.swatch_kind(), SwatchKind::Stroke);
}

#[test]
fn make_pattern_tiles_the_selection_with_its_stroke() {
    let (mut d, rect, _) = doc();
    run(&mut d, |d| {
        command(
            d,
            &[rect],
            Target::Stroke,
            &Edit::Paint(Paint::Solid(Rgba::BLACK)),
        )
    });
    run(&mut d, |d| {
        command(d, &[rect], Target::Stroke, &Edit::Width(2.0))
    });
    let mut s = crate::session::Session::new(d);
    s.select(vec![rect]);
    make_pattern(&mut s).expect("pattern made");
    let pattern = s.doc.resources.values().find_map(|r| match r {
        Resource::Pattern(p) => Some(p.clone()),
        Resource::Gradient(_) => None,
    });
    let p = pattern.expect("a pattern");
    // The 10 × 10 rect plus half its 2 px stroke on each side.
    assert_eq!(p.tile, Rect::new(0.0, 0.0, 12.0, 12.0));
    assert_eq!(s.edit_menu().undo, Some("Make pattern"));
    s.select(vec![]);
    assert!(make_pattern(&mut s).is_err(), "nothing selected");
}

#[test]
fn a_pattern_is_renamed_and_its_name_cleared() {
    let (mut d, rect, _) = doc();
    let (cmd, pid) = d
        .make_pattern(&[rect], Rect::new(0.0, 0.0, 10.0, 10.0))
        .expect("pattern");
    d.apply(cmd).expect("made");
    let name = |d: &Document| match &d.resources[&pid] {
        Resource::Pattern(p) => p.name.clone(),
        Resource::Gradient(_) => unreachable!("pattern"),
    };
    run(&mut d, |d| rename_resource(d, pid, "  dots  "));
    assert_eq!(name(&d).as_deref(), Some("dots"));
    assert_eq!(rename_resource(&d, pid, "dots"), None, "no change");
    run(&mut d, |d| rename_resource(d, pid, " "));
    assert_eq!(name(&d), None, "blank clears the name");
}

#[test]
fn deleting_a_pattern_unpaints_its_users_in_one_undo_step() {
    let (mut d, rect, text) = doc();
    let (cmd, pid) = d
        .make_pattern(&[rect], Rect::new(0.0, 0.0, 10.0, 10.0))
        .expect("pattern");
    d.apply(cmd).expect("made");
    let patterned = Edit::Paint(Paint::Pattern(pid));
    // The rect's fill and stroke, and the text's background, use it.
    run(&mut d, |d| command(d, &[rect], Target::Fill, &patterned));
    run(&mut d, |d| command(d, &[rect], Target::Stroke, &patterned));
    run(&mut d, |d| {
        command(d, &[text], Target::Background, &patterned)
    });
    let before = d.clone();
    let mut h = vextor_core::History::new();
    h.apply(
        &mut d,
        delete_resource(&before, pid).expect("exists"),
        &vextor_core::Selection::None,
    )
    .expect("applies");
    assert!(!d.resources.contains_key(&pid));
    assert_eq!(users(&d, pid), 0);
    assert_eq!(paint(&d.nodes[&rect], Target::Fill), Some(Paint::None));
    assert_eq!(stroke(&d.nodes[&rect], Target::Stroke), None);
    let NodeKind::Text(t) = &d.nodes[&text].kind else {
        unreachable!("text")
    };
    assert_eq!(t.frame, None, "no background or border left");
    assert_eq!(h.undo_label(), Some("Delete pattern"));
    assert!(h.undo(&mut d).is_some());
    assert_eq!(d, before, "undo brings the pattern and its paints back");
}

#[test]
fn deleting_an_unused_gradient_just_removes_it() {
    let (mut d, ..) = doc();
    let g = new_gradient(GradientShape::Linear, Rgba::BLACK, None);
    let id = d.alloc_resource_id();
    d.apply(Command::PutResource {
        id,
        resource: Resource::Gradient(g),
        nodes: vec![],
    })
    .expect("gradient");
    run(&mut d, |d| delete_resource(d, id));
    assert!(!d.resources.contains_key(&id));
    assert_eq!(delete_resource(&d, id), None, "already gone");
}
