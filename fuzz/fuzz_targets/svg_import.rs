//! SVG import (spec 13): any bytes give a document or an error, never a
//! panic, a hang or unbounded memory; what imports also exports.

#![no_main]

use std::sync::{Arc, LazyLock};

use libfuzzer_sys::fuzz_target;
use vextor_core::SvgExportOptions;
use vextor_geom::{FontDb, Fonts};
use vextor_svg::{Format, Scope, export, import_svg, into_document};

static FONTS: LazyLock<Fonts> = LazyLock::new(|| {
    let regular = include_bytes!("../../crates/vextor-geom/tests/fixtures/fonts/NotoSans-Regular.ttf");
    Fonts::new(Arc::new(FontDb::from_data([regular.to_vec()])))
});

fuzz_target!(|data: &[u8]| {
    let Ok(imported) = import_svg(data, &FONTS) else {
        return;
    };
    let doc = into_document(&imported, "fuzz").expect("an import always places");
    assert!(doc.validate().is_ok(), "an import is a valid document");
    let artboard = doc.artboards[0].id;
    let _ = export(
        &doc,
        &FONTS,
        Scope::Artboard(artboard),
        Format::Svg,
        &SvgExportOptions::default(),
    );
});
