//! Text layout (spec 05c) with the bundled OFL test fonts only — never the
//! machine's fonts. Noto Sans has 1 000 units per em, so at size 100 a
//! unit is 0.1 px: "H" advances 741 units (Regular) / 765 (Bold), the
//! space 260; ascent 1 069, descent 293.

use std::sync::Arc;

use kurbo::{Point, Rect};
use vextor_core::{
    CornerRadii, Document, FontSpec, Node, NodeId, NodeKind, Paint, Rgba, Text, TextAlign,
    TextFrame, Transform,
};
use vextor_geom::{
    FaceStyle, FontDb, Fonts, MissingFont, TextLayout, bounds, layout, pick, text_box, touching,
};

const REGULAR: &[u8] = include_bytes!("fixtures/fonts/NotoSans-Regular.ttf");
const BOLD: &[u8] = include_bytes!("fixtures/fonts/NotoSans-Bold.ttf");

const H: f64 = 74.1;
const H_BOLD: f64 = 76.5;
const SPACE: f64 = 26.0;

fn db() -> FontDb {
    FontDb::from_data([REGULAR.to_vec(), BOLD.to_vec()])
}

fn text(content: &str) -> Text {
    Text {
        size: 100.0,
        ..Text::new(content, "Noto Sans")
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

fn assert_close(a: f64, b: f64) {
    assert!(close(a, b), "{a} != {b}");
}

fn lay(t: &Text) -> TextLayout {
    layout(t, &db())
}

// ----- the font database -----

#[test]
fn families_and_styles_come_from_the_loaded_faces() {
    let db = db();
    assert_eq!(db.families(), ["Noto Sans"]);
    assert_eq!(db.family("noto SANS"), Some("Noto Sans"));
    assert_eq!(
        db.styles("Noto Sans"),
        [
            FaceStyle {
                weight: 400,
                italic: false
            },
            FaceStyle {
                weight: 700,
                italic: false
            },
        ]
    );
    assert_eq!(db.styles("Nope"), []);
}

#[test]
fn resolve_picks_the_nearest_face_of_the_family() {
    let db = db();
    let face = |weight, italic| {
        let f = db
            .resolve(&FontSpec {
                family: "noto sans".to_owned(),
                weight,
                italic,
            })
            .expect("resolves");
        assert_eq!(f.family, "Noto Sans");
        assert!(!f.missing);
        (f.style.weight, f.style.italic)
    };
    assert_eq!(face(400, false), (400, false));
    assert_eq!(face(700, false), (700, false));
    assert_eq!(face(300, false), (400, false));
    assert_eq!(face(600, false), (700, false));
    assert_eq!(face(900, false), (700, false));
    // No italic face: the upright one stands in.
    assert_eq!(face(400, true), (400, false));
}

#[test]
fn malformed_font_data_is_skipped() {
    let db = FontDb::from_data([b"not a font".to_vec(), vec![0; 64]]);
    assert!(db.is_empty());
    assert_eq!(db.resolve(&text("x").font), None);
    let l = layout(&text("Hi"), &db);
    assert!(l.glyphs.is_empty());
    assert_eq!(l.lines.len(), 1);
}

// ----- missing fonts -----

#[test]
fn a_missing_family_falls_back_and_says_so() {
    let mut t = text("HH");
    t.font.family = "Nope Display".to_owned();
    let l = lay(&t);
    assert_eq!(
        l.missing,
        Some(MissingFont {
            wanted: "Nope Display".to_owned(),
            shown: Some("Noto Sans".to_owned()),
        })
    );
    assert_close(l.bounds.width(), 2.0 * H);
    assert!(!l.glyphs.is_empty());
    // The text keeps the family it asked for.
    assert_eq!(t.font.family, "Nope Display");
}

#[test]
fn an_installed_family_is_not_missing() {
    assert_eq!(lay(&text("H")).missing, None);
}

#[test]
fn without_fonts_text_is_placed_with_estimated_metrics() {
    let l = layout(&text("Hi"), &FontDb::new());
    assert!(l.glyphs.is_empty());
    assert_eq!(l.ink, None);
    assert_close(l.bounds.width(), 100.0);
    assert_close(l.bounds.height(), 120.0);
    assert_eq!(
        l.missing,
        Some(MissingFont {
            wanted: "Noto Sans".to_owned(),
            shown: None,
        })
    );
}

// ----- metrics -----

#[test]
fn advance_widths_follow_the_font() {
    assert_close(lay(&text("H")).bounds.width(), H);
    assert_close(lay(&text("HH")).bounds.width(), 2.0 * H);
    assert_close(lay(&text("H H")).bounds.width(), 2.0 * H + SPACE);
    let mut bold = text("H");
    bold.font.weight = 700;
    assert_close(lay(&bold).bounds.width(), H_BOLD);
}

#[test]
fn advances_scale_with_size() {
    let mut t = text("HH");
    t.size = 18.0;
    assert_close(lay(&t).bounds.width(), 2.0 * H * 0.18);
}

#[test]
fn letter_spacing_follows_every_character() {
    let mut t = text("HH");
    t.letter_spacing = 10.0;
    let l = lay(&t);
    assert_close(l.bounds.width(), 2.0 * H + 20.0);
    let x: Vec<f64> = l.lines[0].carets.iter().map(|c| c.1).collect();
    assert_close(x[1], H + 10.0);
    t.letter_spacing = -5.0;
    assert_close(lay(&t).bounds.width(), 2.0 * H - 10.0);
}

#[test]
fn line_box_and_baseline_follow_line_height() {
    let l = lay(&text("H"));
    let line = &l.lines[0];
    assert_close(line.rect.y0, 0.0);
    assert_close(line.rect.height(), 120.0);
    // half leading (120 − (106.9 + 29.3)) / 2 = −8.1, plus the ascent
    assert_close(line.baseline, -8.1 + 106.9);
    let ink = l.ink.expect("inked");
    assert!(ink.y1 <= line.baseline + 1e-6, "H sits on the baseline");
    assert!(ink.y0 > line.rect.y0, "H is below the line top");
}

#[test]
fn lines_break_on_newline_only() {
    let mut t = text("H\nHH\n");
    t.line_height = 1.5;
    let l = lay(&t);
    assert_eq!(l.lines.len(), 3);
    assert_eq!(l.lines[0].range, 0..1);
    assert_eq!(l.lines[1].range, 2..4);
    assert_eq!(l.lines[2].range, 5..5);
    assert_close(l.lines[1].rect.y0, 150.0);
    assert_close(l.lines[2].rect.y0, 300.0);
    assert_close(l.lines[2].rect.width(), 0.0);
    assert_close(l.bounds.height(), 450.0);
    assert_close(l.bounds.width(), 2.0 * H);
    // A long line is never wrapped.
    let long = lay(&text(&"H".repeat(50)));
    assert_eq!(long.lines.len(), 1);
    assert_close(long.bounds.width(), 50.0 * H);
}

#[test]
fn alignment_lines_up_lines_within_the_widest() {
    let mut t = text("H\nHH");
    let x0 = |t: &Text| lay(t).lines[0].rect.x0;
    assert_close(x0(&t), 0.0);
    t.align = TextAlign::Centre;
    assert_close(x0(&t), H / 2.0);
    t.align = TextAlign::Right;
    assert_close(x0(&t), H);
    let l = lay(&t);
    assert_close(l.bounds.x0, 0.0);
    assert_close(l.bounds.width(), 2.0 * H);
    assert_close(l.lines[0].carets[0].1, H);
}

#[test]
fn empty_text_still_has_a_line_box() {
    let l = lay(&text(""));
    assert_eq!(l.lines.len(), 1);
    assert_eq!(l.lines[0].carets, [(0, 0.0)]);
    assert_close(l.bounds.height(), 120.0);
    assert_eq!(l.ink, None);
}

#[test]
fn ink_lies_within_the_layout_box_for_plain_letters() {
    let l = lay(&text("Hamburgefonstiv\nTAXI"));
    let ink = l.ink.expect("inked");
    let b = l.bounds;
    assert!(
        ink.x0 >= b.x0 - 1e-6 && ink.x1 <= b.x1 + 1e-6,
        "{ink:?} in {b:?}"
    );
    assert!(ink.y0 >= b.y0 && ink.y1 <= b.y1, "{ink:?} in {b:?}");
}

// ----- carets -----

#[test]
fn carets_sit_at_every_character_boundary() {
    let l = lay(&text("HH"));
    let carets = &l.lines[0].carets;
    assert_eq!(carets.len(), 3);
    for (got, want) in carets.iter().zip([(0, 0.0), (1, H), (2, 2.0 * H)]) {
        assert_eq!(got.0, want.0);
        assert_close(got.1, want.1);
    }
}

#[test]
fn carets_use_byte_indexes_of_multibyte_characters() {
    let l = lay(&text("éH"));
    let bytes: Vec<usize> = l.lines[0].carets.iter().map(|c| c.0).collect();
    assert_eq!(bytes, [0, 2, 3]);
}

#[test]
fn caret_and_index_at_map_both_ways() {
    let l = lay(&text("HH\nH"));
    let (line, x) = l.caret(1);
    assert_eq!(line, 0);
    assert_close(x, H);
    assert_eq!(l.caret(3), (1, 0.0));
    assert_eq!(l.caret(99).0, 1);
    assert_eq!(l.index_at(Point::new(H - 5.0, 10.0)), 1);
    assert_eq!(l.index_at(Point::new(-50.0, 10.0)), 0);
    assert_eq!(l.index_at(Point::new(500.0, 10.0)), 2);
    assert_eq!(l.index_at(Point::new(500.0, 130.0)), 4);
    assert_eq!(l.index_at(Point::new(0.0, 9_999.0)), 3);
}

// ----- frame, cache -----

fn framed(t: Text) -> Text {
    Text {
        frame: Some(TextFrame {
            padding: [4.0, 8.0, 6.0, 10.0],
            radius: CornerRadii::default(),
            fill: Paint::Solid(Rgba::rgb(0, 0, 255)),
            stroke: None,
        }),
        ..t
    }
}

#[test]
fn text_box_grows_by_the_frame_padding() {
    let t = text("H");
    let l = lay(&t);
    assert_eq!(text_box(&t, &l), l.bounds);
    let t = framed(t);
    let b = text_box(&t, &l);
    assert_close(b.x0, -10.0);
    assert_close(b.y0, -4.0);
    assert_close(b.x1, H + 8.0);
    assert_close(b.y1, 126.0);
}

#[test]
fn layouts_are_cached_until_the_text_changes() {
    let fonts = Fonts::new(Arc::new(db()));
    let id = NodeId(7);
    let mut t = text("HH");
    let a = fonts.layout(id, &t);
    assert!(Arc::ptr_eq(&a, &fonts.layout(id, &t)));
    t.style.fill = Paint::Solid(Rgba::rgb(255, 0, 0));
    assert!(
        Arc::ptr_eq(&a, &fonts.layout(id, &t)),
        "paint is not layout"
    );
    t.content.push('H');
    let b = fonts.layout(id, &t);
    assert!(!Arc::ptr_eq(&a, &b));
    assert_close(b.bounds.width(), 3.0 * H);
    t.size = 50.0;
    assert_close(fonts.layout(id, &t).bounds.width(), 1.5 * H);
}

// ----- hit-testing and bounds -----

/// A layer holding one framed text at (100, 50).
fn doc_with_text() -> (Document, NodeId) {
    let mut doc = Document::new("t");
    let layer = doc.layers[0];
    let id = doc.alloc_node_id();
    let mut node = Node::new(id, NodeKind::Text(framed(text("HH"))));
    node.transform = Transform([1.0, 0.0, 0.0, 1.0, 100.0, 50.0]);
    doc.apply(vextor_core::Command::Insert {
        parent: Some(layer),
        index: 0,
        nodes: vec![node],
    })
    .expect("inserts");
    (doc, id)
}

#[test]
fn text_bounds_are_its_box_in_document_space() {
    let (doc, id) = doc_with_text();
    let fonts = Fonts::new(Arc::new(db()));
    let b = bounds(&doc, &fonts, id).expect("visible");
    let want = Rect::new(90.0, 46.0, 100.0 + 2.0 * H + 8.0, 176.0);
    for (got, want) in [
        (b.x0, want.x0),
        (b.y0, want.y0),
        (b.x1, want.x1),
        (b.y1, want.y1),
    ] {
        assert_close(got, want);
    }
}

#[test]
fn a_label_is_picked_anywhere_in_its_box() {
    let (doc, id) = doc_with_text();
    let fonts = Fonts::new(Arc::new(db()));
    // Between the letters and inside the padding: no ink there.
    assert_eq!(
        pick(&doc, &fonts, Point::new(100.0 + H, 60.0), 0.0),
        Some(id)
    );
    assert_eq!(pick(&doc, &fonts, Point::new(92.0, 48.0), 0.0), Some(id));
    assert_eq!(pick(&doc, &fonts, Point::new(85.0, 48.0), 0.0), None);
    assert_eq!(
        touching(&doc, &fonts, Rect::new(0.0, 0.0, 95.0, 60.0)),
        vec![id]
    );
    assert_eq!(touching(&doc, &fonts, Rect::new(0.0, 0.0, 80.0, 40.0)), []);
}
