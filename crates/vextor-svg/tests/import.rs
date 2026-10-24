//! SVG and image import (spec 13): import → export → import is stable for
//! every fixture (the second export is byte-identical to the first), and
//! the report says what was left out.

mod common;

use std::path::{Path, PathBuf};

use common::fonts;
use vextor_core::{
    Document, GradientUnits, IdMode, ImageMime, NodeKind, Paint, Resource, Styling,
    SvgExportOptions, TextAlign,
};
use vextor_geom::Fonts;
use vextor_svg::{
    Dropped, Format, Imported, Scope, export, grouped, import_image, import_svg, into_document,
};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/import")
}

fn read(name: &str) -> Vec<u8> {
    std::fs::read(fixture_dir().join(name)).expect("fixture exists")
}

/// Every non-hostile SVG fixture, by file name.
fn fixtures() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixture_dir())
        .expect("fixture folder")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| {
            Path::new(n).extension().is_some_and(|e| e == "svg") && !n.starts_with("hostile_")
        })
        .collect();
    names.sort();
    names
}

fn import(bytes: &[u8], fonts: &Fonts) -> (Imported, Document) {
    let imported = import_svg(bytes, fonts).expect("imports");
    let doc = into_document(&imported, "fixture").expect("places");
    doc.validate().expect("an import is a valid document");
    (imported, doc)
}

fn export_svg(doc: &Document, fonts: &Fonts, options: &SvgExportOptions) -> String {
    let artboard = doc.artboards[0].id;
    export(doc, fonts, Scope::Artboard(artboard), Format::Svg, options)
        .expect("the artboard exports")
        .code
}

/// The export options the round trip is checked with.
fn option_sets() -> Vec<(&'static str, SvgExportOptions)> {
    let base = SvgExportOptions::default();
    vec![
        ("default", base.clone()),
        (
            "no ids",
            SvgExportOptions {
                ids: IdMode::None,
                ..base.clone()
            },
        ),
        (
            "unique ids",
            SvgExportOptions {
                ids: IdMode::UniquePrefixed,
                ..base.clone()
            },
        ),
        (
            "style element",
            SvgExportOptions {
                styling: Styling::StyleElement,
                ..base.clone()
            },
        ),
        // Not "current colour": it turns a colour into `currentColor`, which
        // comes back as `currentColor`, so the next export picks another.
        (
            "minified, sized",
            SvgExportOptions {
                minify: true,
                responsive: false,
                ..base.clone()
            },
        ),
        (
            "precision 4",
            SvgExportOptions {
                precision: 4,
                ..base.clone()
            },
        ),
        (
            "text as outlines",
            SvgExportOptions {
                text_as_outlines: true,
                ..base
            },
        ),
    ]
}

#[test]
fn import_export_import_is_byte_stable_for_every_fixture() {
    let fonts = fonts();
    let names = fixtures();
    assert!(names.len() >= 10, "fixtures found: {names:?}");
    for name in &names {
        for (label, options) in option_sets() {
            let (_, doc) = import(&read(name), &fonts);
            let first = export_svg(&doc, &fonts, &options);
            let (_, again) = import(first.as_bytes(), &fonts);
            let second = export_svg(&again, &fonts, &options);
            assert_eq!(first, second, "{name} ({label}) is not stable");
        }
    }
}

#[test]
fn first_exports_match_their_snapshots() {
    let fonts = fonts();
    for name in fixtures() {
        let (_, doc) = import(&read(&name), &fonts);
        let code = export_svg(&doc, &fonts, &SvgExportOptions::default());
        let snapshot = format!("import_{}", name.trim_end_matches(".svg"));
        insta::assert_snapshot!(snapshot, code);
    }
}

/// What the import of `name` holds, by kind of node.
fn kinds(doc: &Document) -> Vec<&'static str> {
    doc.nodes
        .values()
        .map(|n| match &n.kind {
            NodeKind::Layer(_) => "layer",
            NodeKind::Group(_) => "group",
            NodeKind::Shape(_) => "shape",
            NodeKind::Image(_) => "image",
            NodeKind::Text(_) => "text",
        })
        .collect()
}

#[test]
fn the_viewport_becomes_the_artboard() {
    let fonts = fonts();
    let (imported, doc) = import(&read("shapes.svg"), &fonts);
    assert_eq!(imported.size, (120.0, 80.0));
    let r = doc.artboards[0].rect;
    assert_eq!((r.x, r.y, r.width, r.height), (0.0, 0.0, 120.0, 80.0));
    assert!(imported.report.is_clean(), "{:?}", imported.report);
    assert_eq!(imported.report.objects, 9);
}

#[test]
fn names_come_from_ids_and_groups_stay_groups() {
    let fonts = fonts();
    let (_, doc) = import(&read("groups_and_clip.svg"), &fonts);
    let named = |name: &str| {
        doc.nodes
            .values()
            .find(|n| n.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("{name} imported"))
    };
    let badge = named("badge");
    assert!((badge.opacity - 0.5).abs() < 1e-6);
    let NodeKind::Group(c) = &named("clipped").kind else {
        panic!("a group")
    };
    let clip = c.clip.expect("keeps its clip");
    assert!(matches!(doc.nodes[&clip].kind, NodeKind::Shape(_)));
    // Two rects clip as one shape (their union).
    let clips = doc
        .nodes
        .values()
        .filter_map(|n| n.container().and_then(|c| c.clip))
        .count();
    assert_eq!(clips, 2);
    assert!(kinds(&doc).contains(&"group"));
}

#[test]
fn gradients_keep_their_units_and_identical_ones_are_shared() {
    let fonts = fonts();
    let (imported, doc) = import(&read("gradients.svg"), &fonts);
    let gradients: Vec<_> = doc
        .resources
        .values()
        .filter_map(|r| match r {
            Resource::Gradient(g) => Some(g),
            Resource::Pattern(_) => None,
        })
        .collect();
    let sky: Vec<_> = gradients
        .iter()
        .filter(|g| g.name.as_deref() == Some("sky"))
        .collect();
    assert_eq!(sky.len(), 1, "two rects share one sky gradient");
    assert_eq!(sky[0].units, GradientUnits::ObjectBoundingBox);
    let user = gradients
        .iter()
        .find(|g| g.name.as_deref() == Some("user"))
        .expect("user gradient");
    assert_eq!(user.units, GradientUnits::UserSpace);
    assert_eq!(imported.report.approximated, 0, "{:?}", imported.report);
}

#[test]
fn patterns_keep_their_tile_and_content() {
    let fonts = fonts();
    let (_, doc) = import(&read("pattern.svg"), &fonts);
    let patterns: Vec<_> = doc
        .resources
        .values()
        .filter_map(|r| match r {
            Resource::Pattern(p) => Some(p),
            Resource::Gradient(_) => None,
        })
        .collect();
    assert_eq!(patterns.len(), 2);
    let dots = patterns
        .iter()
        .find(|p| p.name.as_deref() == Some("dots"))
        .expect("dots");
    assert_eq!((dots.tile.width, dots.tile.height), (10.0, 10.0));
    assert_eq!(dots.content.len(), 2);
}

#[test]
fn embedded_images_become_image_objects() {
    let fonts = fonts();
    let (_, doc) = import(&read("image.svg"), &fonts);
    let images: Vec<_> = doc
        .nodes
        .values()
        .filter_map(|n| match &n.kind {
            NodeKind::Image(i) => Some((n, i)),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 2);
    let (node, image) = images
        .iter()
        .find(|(n, _)| n.name.as_deref() == Some("photo"))
        .expect("photo");
    // The image's own size, not its pixels; a move goes into its rect.
    let r = image.rect;
    assert_eq!((r.x, r.y, r.width, r.height), (4.0, 6.0, 20.0, 10.0));
    assert_eq!(node.transform, vextor_core::Transform::IDENTITY);
    let vextor_core::ImageSource::Embedded { mime, bytes } = &image.source;
    assert_eq!(*mime, ImageMime::Png);
    assert_eq!(bytes, &read("pixel_2x2.png"));
}

#[test]
fn single_style_text_in_an_installed_font_stays_text() {
    let fonts = fonts();
    let (imported, doc) = import(&read("text.svg"), &fonts);
    let texts: Vec<_> = doc
        .nodes
        .values()
        .filter_map(|n| match &n.kind {
            NodeKind::Text(t) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(texts.len(), 3, "{:?}", imported.report);
    let title = texts
        .iter()
        .find(|t| t.content == "Buy now")
        .expect("title");
    assert_eq!(title.font.family, "Noto Sans");
    assert_eq!(title.font.weight, 700);
    assert!((title.size - 20.0).abs() < 1e-6);
    let two = texts
        .iter()
        .find(|t| t.content == "Two lines\ncentred")
        .expect("two lines");
    assert_eq!(two.align, TextAlign::Centre);
    assert!((two.line_height - 18.0 / 14.0).abs() < 1e-4);
    assert!((two.letter_spacing - 1.0).abs() < 1e-6);
    let right = texts.iter().find(|t| t.content == "Right").expect("right");
    assert_eq!(right.align, TextAlign::Right);
    assert_eq!(right.style.fill, Paint::None);
    assert_eq!(imported.report.outlined, Vec::<String>::new());
}

#[test]
fn text_that_cannot_be_text_comes_in_as_outlines_and_says_why() {
    let fonts = fonts();
    let (imported, doc) = import(&read("text_outlines.svg"), &fonts);
    assert!(!kinds(&doc).contains(&"text"));
    let mut why = imported.report.outlined.clone();
    why.sort();
    assert_eq!(
        why,
        ["font “Nope Sans” is not installed", "it mixes styles"]
    );
    let summary = imported.report.summary();
    assert!(summary.contains("2 texts as outlines"), "{summary}");
    let missing = doc
        .nodes
        .values()
        .find(|n| n.name.as_deref() == Some("missing"))
        .expect("the outlines keep the text's id");
    assert!(matches!(
        missing.kind,
        NodeKind::Group(_) | NodeKind::Shape(_)
    ));
}

#[test]
fn unsupported_features_are_dropped_and_listed() {
    let fonts = fonts();
    let (imported, doc) = import(&read("unsupported.svg"), &fonts);
    let r = &imported.report;
    assert_eq!(r.dropped.get(&Dropped::Filter), Some(&1));
    assert_eq!(r.dropped.get(&Dropped::Mask), Some(&1));
    assert_eq!(r.dropped.get(&Dropped::BlendMode), Some(&1));
    assert_eq!(r.dropped.get(&Dropped::Image), Some(&2));
    assert_eq!(r.dropped.get(&Dropped::PaintOrder), Some(&1));
    assert_eq!(
        r.summary(),
        "Imported 4 objects. Left out: 1 filter, 1 mask, 1 blend mode, 2 images, \
         1 stroke-first paint order."
    );
    // The content of filtered and masked objects is kept.
    assert_eq!(kinds(&doc).iter().filter(|k| **k == "shape").count(), 4);
}

#[test]
fn several_objects_are_grouped_for_placing_one_stays_alone() {
    let fonts = fonts();
    let imported = import_svg(&read("shapes.svg"), &fonts).expect("imports");
    let g = grouped(imported, Some("shapes"));
    let roots: Vec<_> = g
        .payload
        .nodes
        .iter()
        .filter(|n| {
            !g.payload
                .nodes
                .iter()
                .filter_map(|p| p.container())
                .any(|c| c.children.contains(&n.id))
        })
        .collect();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].name.as_deref(), Some("shapes"));
    let one = import_image(&read("pixel_2x2.png")).expect("png");
    assert_eq!(grouped(one.clone(), Some("x")), one);
}

#[test]
fn a_png_file_becomes_one_image_at_its_pixel_size() {
    let imported = import_image(&read("pixel_2x2.png")).expect("png");
    assert_eq!(imported.size, (2.0, 2.0));
    assert_eq!(imported.payload.nodes.len(), 1);
    let NodeKind::Image(i) = &imported.payload.nodes[0].kind else {
        panic!("an image")
    };
    assert_eq!((i.rect.width, i.rect.height), (2.0, 2.0));
}

#[test]
fn our_own_sample_documents_survive_export_and_import() {
    let fonts = fonts();
    let doc = vextor_render::fixtures::basic_shapes();
    let options = SvgExportOptions::default();
    let original = export_svg(&doc, &fonts, &options);
    let (imported, back) = import(original.as_bytes(), &fonts);
    assert!(imported.report.is_clean(), "{:?}", imported.report);
    let first = export_svg(&back, &fonts, &options);
    let (_, again) = import(first.as_bytes(), &fonts);
    assert_eq!(first, export_svg(&again, &fonts, &options));
}

#[test]
fn current_color_stays_current_color_and_never_leaks_its_stand_in() {
    let fonts = fonts();
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 30 10">
        <rect width="10" height="10" fill="currentColor"/>
        <rect x="10" width="10" height="10" fill="currentColor" fill-opacity=".5"/>
        <linearGradient id="g"><stop stop-color="currentColor"/><stop offset="1" stop-color="#fff"/></linearGradient>
        <rect x="20" width="10" height="10" fill="url(#g)"/></svg>"##;
    let (imported, doc) = import(svg, &fonts);
    let fills: Vec<Paint> = doc
        .nodes
        .values()
        .filter_map(|n| match &n.kind {
            NodeKind::Shape(s) => Some(s.style.fill),
            _ => None,
        })
        .collect();
    assert_eq!(fills[..2], [Paint::CurrentColor, Paint::CurrentColor]);
    assert_eq!(
        imported.report.approximated, 1,
        "the lost opacity is counted"
    );
    let code = export_svg(&doc, &fonts, &SvgExportOptions::default());
    assert!(!code.contains("#010203"), "{code}");
    assert!(code.contains("fill=\"currentColor\""), "{code}");
}
