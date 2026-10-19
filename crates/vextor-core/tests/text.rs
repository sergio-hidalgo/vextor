//! Text objects (spec 05c): commands apply → undo → redo, validation, and
//! paints that refer to resources.

mod common;

use common::{Fx, check, fixture, label_text, node_mut, rejects, text};
use vextor_core::{
    ClipboardPayload, Command, CoreError, CornerRadii, Document, FontSpec, MAX_FONT_FAMILY_CHARS,
    MAX_TEXT_CHARS, NodeId, NodeKind, Paint, Rgba, Stroke, Style, Text, TextAlign, TextFrame,
    Transform,
};

fn text_mut(d: &mut Document, id: NodeId) -> &mut Text {
    match &mut node_mut(d, id).kind {
        NodeKind::Text(t) => t,
        _ => panic!("{id:?} is text"),
    }
}

fn frame(fill: Paint) -> TextFrame {
    TextFrame {
        padding: [4.0, 8.0, 4.0, 8.0],
        radius: CornerRadii::default(),
        fill,
        stroke: None,
    }
}

/// The fixture's text replaced by `t`, applied as a command so the result
/// is checked like any edit; `Err` when the document rejects it.
fn with_text(fx: &Fx, t: Text) -> Result<Document, CoreError> {
    let mut d = fx.doc.clone();
    let next = NodeId(d.next_id());
    d.apply(Command::ReplaceWith {
        removed: vec![fx.text],
        inserted: vec![text(next, t)],
        parent: Some(fx.layer2),
        index: 1,
    })?;
    Ok(d)
}

// ----- commands -----

#[test]
fn set_text() {
    let fx = fixture();
    let cmd = Command::SetText {
        id: fx.text,
        content: "Sign up".to_owned(),
    };
    check(&fx.doc, cmd, |d| {
        text_mut(d, fx.text).content = "Sign up".to_owned();
    });
}

#[test]
fn set_text_may_empty_the_content() {
    let fx = fixture();
    let cmd = Command::SetText {
        id: fx.text,
        content: String::new(),
    };
    check(&fx.doc, cmd, |d| text_mut(d, fx.text).content.clear());
}

#[test]
fn set_text_rejects_shapes_and_content_over_the_limit() {
    let fx = fixture();
    rejects(
        &fx.doc,
        Command::SetText {
            id: fx.star,
            content: "x".to_owned(),
        },
    );
    rejects(
        &fx.doc,
        Command::SetText {
            id: fx.text,
            content: "é".repeat(MAX_TEXT_CHARS + 1),
        },
    );
}

#[test]
fn set_text_counts_characters_not_bytes() {
    let fx = fixture();
    let content = "é".repeat(MAX_TEXT_CHARS);
    check(
        &fx.doc,
        Command::SetText {
            id: fx.text,
            content: content.clone(),
        },
        |d| text_mut(d, fx.text).content = content,
    );
}

fn style_cmd(ids: Vec<NodeId>, font: FontSpec, size: f64) -> Command {
    Command::SetTextStyle {
        ids,
        font,
        size,
        line_height: 1.5,
        letter_spacing: -0.5,
        align: TextAlign::Right,
    }
}

fn helvetica() -> FontSpec {
    FontSpec {
        family: "Helvetica".to_owned(),
        weight: 500,
        italic: true,
    }
}

#[test]
fn set_text_style() {
    let fx = fixture();
    check(&fx.doc, style_cmd(vec![fx.text], helvetica(), 24.0), |d| {
        let t = text_mut(d, fx.text);
        t.font = helvetica();
        t.size = 24.0;
        t.line_height = 1.5;
        t.letter_spacing = -0.5;
        t.align = TextAlign::Right;
    });
}

#[test]
fn set_text_style_rejects_bad_fonts_and_metrics() {
    let fx = fixture();
    let font = |family: &str, weight| FontSpec {
        family: family.to_owned(),
        weight,
        italic: false,
    };
    let long = "a".repeat(MAX_FONT_FAMILY_CHARS + 1);
    for (f, size) in [
        (font("", 400), 16.0),
        (font("   ", 400), 16.0),
        (font(&long, 400), 16.0),
        (font("Inter", 50), 16.0),
        (font("Inter", 1000), 16.0),
        (font("Inter", 400), 0.0),
        (font("Inter", 400), -3.0),
        (font("Inter", 400), f64::NAN),
        (font("Inter", 400), f64::INFINITY),
    ] {
        rejects(&fx.doc, style_cmd(vec![fx.text], f, size));
    }
    for (line_height, letter_spacing) in [(0.0, 0.0), (f64::NAN, 0.0), (1.2, f64::INFINITY)] {
        rejects(
            &fx.doc,
            Command::SetTextStyle {
                ids: vec![fx.text],
                font: helvetica(),
                size: 16.0,
                line_height,
                letter_spacing,
                align: TextAlign::Left,
            },
        );
    }
    rejects(
        &fx.doc,
        style_cmd(vec![fx.text, fx.star], helvetica(), 16.0),
    );
}

#[test]
fn set_text_frame_adds_changes_and_removes() {
    let fx = fixture();
    let blue = frame(Paint::Solid(Rgba::rgb(0, 0, 255)));
    check(
        &fx.doc,
        Command::SetTextFrame {
            ids: vec![fx.text],
            frame: Some(blue.clone()),
        },
        |d| text_mut(d, fx.text).frame = Some(blue),
    );
    check(
        &fx.doc,
        Command::SetTextFrame {
            ids: vec![fx.text],
            frame: None,
        },
        |d| text_mut(d, fx.text).frame = None,
    );
}

#[test]
fn set_text_frame_rejects_bad_numbers_and_paints() {
    let fx = fixture();
    let mut negative = frame(Paint::None);
    negative.padding[2] = -1.0;
    let mut radius = frame(Paint::None);
    radius.radius.tr = f64::NAN;
    let wrong_resource = frame(Paint::Pattern(fx.gradient));
    let mut thin = frame(Paint::None);
    thin.stroke = Some(Stroke {
        width: -1.0,
        ..Stroke::default()
    });
    let mut bad_border_paint = frame(Paint::None);
    bad_border_paint.stroke = Some(Stroke {
        paint: Paint::Gradient(fx.pattern),
        ..Stroke::default()
    });
    for f in [negative, radius, wrong_resource, thin, bad_border_paint] {
        rejects(
            &fx.doc,
            Command::SetTextFrame {
                ids: vec![fx.text],
                frame: Some(f),
            },
        );
    }
    rejects(
        &fx.doc,
        Command::SetTextFrame {
            ids: vec![fx.image],
            frame: None,
        },
    );
}

#[test]
fn set_style_paints_text_glyphs() {
    let fx = fixture();
    let style = Style {
        fill: Paint::Gradient(fx.gradient),
        stroke: Some(Stroke::default()),
    };
    check(
        &fx.doc,
        Command::SetStyle {
            ids: vec![fx.text, fx.star],
            style: style.clone(),
        },
        |d| {
            text_mut(d, fx.text).style = style.clone();
            if let NodeKind::Shape(s) = &mut node_mut(d, fx.star).kind {
                s.style = style;
            }
        },
    );
}

#[test]
fn new_text_defaults() {
    let t = Text::new("Hi", "Inter");
    assert_eq!(t.font.weight, 400);
    assert!(!t.font.italic);
    assert!((t.size - 16.0).abs() < f64::EPSILON);
    assert!((t.line_height - 1.2).abs() < f64::EPSILON);
    assert_eq!(t.align, TextAlign::Left);
    assert_eq!(t.style, Style::default());
    assert_eq!(t.frame, None);
}

// ----- paints that refer to resources -----

#[test]
fn a_gradient_used_by_a_text_frame_is_in_use() {
    let fx = fixture();
    let mut t = label_text();
    t.frame = Some(frame(Paint::Gradient(fx.gradient)));
    let mut d = with_text(&fx, t).expect("valid");
    // The rect also uses the gradient; free it so only the text holds it.
    d.apply(Command::SetStyle {
        ids: vec![fx.rect],
        style: Style::default(),
    })
    .expect("restyle");
    assert!(d.resource_in_use(fx.gradient));
    assert_eq!(
        d.apply(Command::RemoveResource { id: fx.gradient }),
        Err(CoreError::ResourceInUse(fx.gradient))
    );
}

#[test]
fn a_text_border_paint_must_exist() {
    let fx = fixture();
    let mut t = label_text();
    let mut f = frame(Paint::None);
    f.stroke = Some(Stroke {
        paint: Paint::Gradient(vextor_core::ResourceId(9_999)),
        ..Stroke::default()
    });
    t.frame = Some(f);
    assert!(with_text(&fx, t).is_err());
}

#[test]
fn pasting_text_into_another_document_brings_its_frame_gradient() {
    let fx = fixture();
    let mut t = label_text();
    t.frame = Some(frame(Paint::Gradient(fx.gradient)));
    let src = with_text(&fx, t).expect("valid");
    let id = *src.nodes.keys().last().expect("the new text");
    let payload: ClipboardPayload = src.copy(&[id]).expect("copies");

    let mut dst = Document::new("other");
    let layer = dst.layers[0];
    let (cmd, roots) = dst
        .paste(&payload, layer, 0, Transform::IDENTITY, "Paste")
        .expect("builds");
    dst.apply(cmd).expect("pastes");
    let NodeKind::Text(pasted) = &dst.nodes[&roots[0]].kind else {
        panic!("pasted a text");
    };
    let Some(TextFrame {
        fill: Paint::Gradient(r),
        ..
    }) = pasted.frame
    else {
        panic!("frame keeps a gradient");
    };
    assert!(dst.resources.contains_key(&r));
    dst.validate().expect("valid");
}
