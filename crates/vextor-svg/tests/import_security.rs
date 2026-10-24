//! `security-check` §2 for import: hostile files are refused or bounded,
//! never a panic, a hang or a read of anything outside the file.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::fonts;
use vextor_core::{NodeKind, SvgExportOptions};
use vextor_svg::{
    Dropped, Format, ImportError, Scope, export, import_image, import_svg, into_document,
};

/// Generous for a debug build; a hang or an expansion blows far past it.
const QUICK: Duration = Duration::from_secs(5);

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/import")
        .join(name)
}

fn read(name: &str) -> Vec<u8> {
    std::fs::read(fixture(name)).expect("fixture exists")
}

fn has_images(nodes: &[vextor_core::Node]) -> bool {
    nodes.iter().any(|n| matches!(n.kind, NodeKind::Image(_)))
}

#[test]
fn external_references_are_never_followed_and_nothing_executable_survives() {
    let fonts = fonts();
    let imported = import_svg(&read("hostile_external_href.svg"), &fonts).expect("imports");
    assert!(!has_images(&imported.payload.nodes));
    assert_eq!(imported.report.dropped.get(&Dropped::Image), Some(&3));
    let doc = into_document(&imported, "hostile").expect("places");
    let options = SvgExportOptions::default();
    for format in Format::ALL {
        let code = export(
            &doc,
            &fonts,
            Scope::Artboard(doc.artboards[0].id),
            format,
            &options,
        )
        .expect("exports")
        .code
        .to_lowercase();
        for bad in [
            "script",
            "foreignobject",
            "onclick",
            "javascript",
            "http:",
            "https:",
            "file:",
            "passwd",
            "hosts",
            "other.svg",
            "@import",
        ] {
            // The SVG namespace is the one URL a file carries.
            let code = code.replace("http://www.w3.org/2000/svg", "");
            assert!(
                !code.contains(bad),
                "{format:?} output contains {bad}: {code}"
            );
        }
    }
}

#[test]
fn an_image_on_disk_is_not_read_even_by_absolute_path() {
    let png = fixture("pixel_2x2.png");
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4">
             <image width="4" height="4" href="{}"/><rect width="1" height="1"/></svg>"#,
        png.display()
    );
    let imported = import_svg(svg.as_bytes(), &fonts()).expect("imports");
    assert!(!has_images(&imported.payload.nodes));
    assert_eq!(imported.report.dropped.get(&Dropped::Image), Some(&1));
}

#[test]
fn entity_expansion_is_refused_and_external_entities_are_never_read() {
    let fonts = fonts();
    let start = Instant::now();
    let laughs = import_svg(&read("hostile_xxe.svg"), &fonts);
    assert!(matches!(laughs, Err(ImportError::Invalid(_))), "{laughs:?}");
    assert!(start.elapsed() < QUICK);
    // An external entity alone: refused, or dropped without reading it.
    match import_svg(&read("hostile_xxe_external_only.svg"), &fonts) {
        Err(ImportError::Invalid(_)) => {}
        Ok(imported) => {
            let text = format!("{:?}", imported.payload);
            assert!(!text.contains("root:"), "read /etc/passwd");
        }
        Err(e) => panic!("unexpected {e:?}"),
    }
}

#[test]
fn use_and_pattern_loops_are_cut_without_hanging() {
    let start = Instant::now();
    let result = import_svg(&read("hostile_use_loop.svg"), &fonts());
    assert!(start.elapsed() < QUICK, "{:?}", start.elapsed());
    // usvg refuses the loop at its own bound, or imports what is left.
    match result {
        Ok(imported) => assert!(imported.report.objects > 0),
        Err(e) => assert_eq!(e, ImportError::TooManyObjects),
    }
    // A `<use>` of itself, alone, is cut and the rest imports.
    let only_loop = br##"<svg xmlns="http://www.w3.org/2000/svg">
        <use id="self" href="#self"/><rect width="1" height="1"/></svg>"##;
    assert!(import_svg(only_loop, &fonts()).is_ok());
}

#[test]
fn a_hundred_thousand_objects_are_refused_quickly() {
    let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">"#);
    for _ in 0..100_000 {
        svg.push_str(r#"<rect width="1" height="1"/>"#);
    }
    svg.push_str("</svg>");
    let start = Instant::now();
    assert_eq!(
        import_svg(svg.as_bytes(), &fonts()),
        Err(ImportError::TooManyObjects)
    );
    assert!(start.elapsed() < QUICK, "{:?}", start.elapsed());
}

#[test]
fn many_objects_through_use_are_refused_quickly() {
    // 30 000 rects drawn by reusing one group: few elements, many objects.
    let mut svg = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><defs><g id="t">"#,
    );
    for _ in 0..100 {
        svg.push_str(r#"<rect width="1" height="1"/>"#);
    }
    svg.push_str("</g></defs>");
    for _ in 0..300 {
        svg.push_str(r##"<use href="#t"/>"##);
    }
    svg.push_str("</svg>");
    let start = Instant::now();
    assert_eq!(
        import_svg(svg.as_bytes(), &fonts()),
        Err(ImportError::TooManyObjects)
    );
    assert!(start.elapsed() < QUICK, "{:?}", start.elapsed());
}

#[test]
fn a_huge_image_is_left_out_of_an_svg_and_refused_alone() {
    let imported = import_svg(&read("hostile_huge_image.svg"), &fonts()).expect("imports");
    assert!(!has_images(&imported.payload.nodes));
    assert_eq!(imported.report.dropped.get(&Dropped::Image), Some(&1));
    assert_eq!(
        import_image(&read("hostile_huge_image.png")),
        Err(ImportError::ImageTooLarge)
    );
}

#[test]
fn malformed_input_is_an_error_not_a_panic() {
    let fonts = fonts();
    for bad in [
        &b""[..],
        b"not xml at all",
        b"<svg",
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"",
        b"\xff\xfe\x00<\x00s\x00v\x00g",
        b"<html><body/></html>",
        b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"-1\" height=\"nan\"/>",
        b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1e39 1e39\"><rect width=\"1e39\" height=\"1\"/></svg>",
    ] {
        assert!(import_svg(bad, &fonts).is_err(), "{:?}", String::from_utf8_lossy(bad));
    }
    for bad in [
        &b""[..],
        b"\x89PNG\r\n\x1a\n",
        b"<svg/>",
        b"RIFF\0\0\0\0WEBPVP8 ",
    ] {
        assert!(import_image(bad).is_err());
    }
}

#[test]
fn an_svg_that_draws_nothing_says_so() {
    let empty = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><g/></svg>"#;
    assert_eq!(import_svg(empty, &fonts()), Err(ImportError::Empty));
}

/// Found by the `svg_import` fuzzer: an arc radius of 10⁴⁸ made usvg ask
/// for gigabytes splitting it into curves.
#[test]
fn a_huge_arc_radius_is_refused_before_parsing() {
    let start = Instant::now();
    assert_eq!(
        import_svg(&read("hostile_huge_arc.svg"), &fonts()),
        Err(ImportError::TooFar)
    );
    assert!(start.elapsed() < QUICK);
}
