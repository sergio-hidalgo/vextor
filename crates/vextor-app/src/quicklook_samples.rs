//! Sample documents for checking the Quick Look extensions (spec 14d): ones
//! Finder should show the artwork of (`good-*`) and damaged or hostile ones
//! it should show the plain document icon for (`bad-*`). Test code only:
//! `scripts/check-quicklook.sh` runs
//!
//! ```sh
//! VEXTOR_QL_SAMPLES_DIR=target/quicklook-check cargo test -p vextor-app -- --ignored write_quicklook_samples
//! ```

use std::fs;
use std::io::Write as _;
use std::path::Path;

use flate2::Compression;
use flate2::write::GzEncoder;
use vextor_core::{Document, to_bytes, to_writer_with_preview};
use vextor_geom::Fonts;
use vextor_render::fixtures;

use crate::files;
use crate::thumbnails::preview_png;

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut out = GzEncoder::new(Vec::new(), Compression::default());
    out.write_all(bytes).expect("in memory");
    out.finish().expect("in memory")
}

/// `doc` as `.vxtr` JSON with `preview` as its picture, uncompressed.
fn json_with_preview(doc: &Document, preview: &[u8]) -> Vec<u8> {
    let mut json = Vec::new();
    to_writer_with_preview(&mut json, doc, Some(preview)).expect("encodes");
    json
}

/// `doc` with `field` (raw JSON) put in as its `preview_png`, gzipped.
fn with_raw_preview(doc: &Document, field: &str) -> Vec<u8> {
    let json = String::from_utf8(to_bytes(doc).expect("encodes")).expect("utf-8");
    let json = json.replacen(
        "\"next_id\"",
        &format!("\"preview_png\":{field},\"next_id\""),
        1,
    );
    gzip(json.as_bytes())
}

/// The PNG header of a 100 000 × 100 000 px picture, with no pixels.
fn huge_png_header() -> Vec<u8> {
    let chunk = |kind: &[u8], data: &[u8]| {
        let mut c = u32::try_from(data.len())
            .expect("small")
            .to_be_bytes()
            .to_vec();
        c.extend_from_slice(kind);
        c.extend_from_slice(data);
        let crc = crc32(&c[4..]);
        c.extend_from_slice(&crc.to_be_bytes());
        c
    };
    let mut ihdr = 100_000_u32.to_be_bytes().repeat(2);
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(chunk(b"IHDR", &ihdr));
    png.extend(chunk(b"IEND", &[]));
    png
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

fn write(dir: &Path, name: &str, bytes: &[u8]) {
    fs::write(dir.join(name), bytes).expect("writes");
}

/// The samples, in `$VEXTOR_QL_SAMPLES_DIR`.
#[test]
#[ignore = "writes files, run by scripts/check-quicklook.sh"]
fn write_quicklook_samples() {
    let dir = std::env::var_os("VEXTOR_QL_SAMPLES_DIR").expect("VEXTOR_QL_SAMPLES_DIR is set");
    let dir = Path::new(&dir);
    fs::create_dir_all(dir).expect("creates");
    let fonts = Fonts::default();
    let shapes = fixtures::basic_shapes();
    let png = preview_png(&shapes, &fonts).expect("drawn");
    let saved = |name: &str, doc: &Document| {
        let png = preview_png(doc, &fonts).expect("drawn");
        files::write_document_with_preview(&dir.join(name), doc, Some(&png)).expect("writes");
    };

    saved("good-shapes.vxtr", &shapes);
    saved("good-paints.vxtr", &fixtures::paints());
    write(
        dir,
        "good-plain-json.vxtr",
        &json_with_preview(&shapes, &png),
    );

    files::write_document(&dir.join("bad-no-preview.vxtr"), &shapes).expect("writes");
    write(
        dir,
        "bad-format-4.vxtr",
        include_bytes!("../../vextor-core/tests/fixtures/label_v4.vxtr"),
    );
    let good = fs::read(dir.join("good-shapes.vxtr")).expect("reads");
    write(dir, "bad-cut-off.vxtr", &good[..good.len() / 3]);
    write(dir, "bad-empty.vxtr", b"");
    let garbage: Vec<u8> = (0..5000_u32).map(|i| (i * 7919 % 251) as u8).collect();
    write(dir, "bad-garbage.vxtr", &garbage);
    write(
        dir,
        "bad-not-png.vxtr",
        &gzip(&json_with_preview(&shapes, b"GIF89a")),
    );
    write(
        dir,
        "bad-huge-png.vxtr",
        &gzip(&json_with_preview(&shapes, &huge_png_header())),
    );
    write(
        dir,
        "bad-base64.vxtr",
        &with_raw_preview(&shapes, "\"!!not base64!!\""),
    );
    write(dir, "bad-number.vxtr", &with_raw_preview(&shapes, "42"));
    let deep = format!("{}{}", "[".repeat(1_000_000), "]".repeat(1_000_000));
    write(dir, "bad-deep.vxtr", &with_raw_preview(&shapes, &deep));
    // 200 MB of JSON once unpacked, four times the limit.
    let mut bomb = GzEncoder::new(Vec::new(), Compression::default());
    bomb.write_all(b"{\"format_version\":5,\"x\":\"")
        .expect("in memory");
    let zeros = vec![b'0'; 1 << 20];
    for _ in 0..200 {
        bomb.write_all(&zeros).expect("in memory");
    }
    bomb.write_all(b"\"}").expect("in memory");
    write(dir, "bad-bomb.vxtr", &bomb.finish().expect("in memory"));
}
