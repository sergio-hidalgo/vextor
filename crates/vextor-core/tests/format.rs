//! `.vextor` bytes: round trips and refused files (`security-check` §2).

mod common;

use common::{fixture, rect_geom, shape};
use vextor_core::{
    Document, FORMAT_VERSION, FormatError, Geometry, MAX_FILE_BYTES, MAX_NODES, NodeKind, Paint,
    Point, Rgba, from_bytes, to_bytes,
};

const MINIMAL_V1: &str = include_str!("fixtures/minimal_v1.vextor");

fn round_trip(doc: &Document) {
    let bytes = to_bytes(doc).expect("encodes");
    let back = from_bytes(&bytes).expect("decodes");
    assert_eq!(&back, doc);
    assert_eq!(back.next_id(), doc.next_id());
}

#[test]
fn every_fixture_round_trips() {
    round_trip(&Document::new("empty"));
    round_trip(&fixture().doc);
    round_trip(&from_bytes(MINIMAL_V1.as_bytes()).expect("fixture loads"));
}

#[test]
fn hand_written_v1_file_keeps_loading() {
    let doc = from_bytes(MINIMAL_V1.as_bytes()).expect("loads");
    assert_eq!(doc.format_version, FORMAT_VERSION);
    assert_eq!(doc.name, "icon");
    assert_eq!(doc.artboards[0].name, "icon-24");
    let dot = doc
        .nodes
        .values()
        .find(|n| n.name.as_deref() == Some("dot"))
        .expect("dot exists");
    let NodeKind::Shape(s) = &dot.kind else {
        panic!("dot is a shape");
    };
    assert_eq!(
        s.geometry,
        Geometry::Ellipse {
            center: Point { x: 12.0, y: 12.0 },
            rx: 8.0,
            ry: 8.0
        }
    );
    assert_eq!(
        s.style.fill,
        Paint::Solid(Rgba {
            r: 30,
            g: 144,
            b: 255,
            a: 255
        })
    );
}

#[test]
fn unknown_format_version_is_refused() {
    let newer = MINIMAL_V1.replace("\"format_version\": 1", "\"format_version\": 7");
    assert_eq!(
        from_bytes(newer.as_bytes()),
        Err(FormatError::UnknownVersion(7))
    );
    let zero = MINIMAL_V1.replace("\"format_version\": 1", "\"format_version\": 0");
    assert_eq!(
        from_bytes(zero.as_bytes()),
        Err(FormatError::UnknownVersion(0))
    );
}

#[test]
fn missing_format_version_is_refused() {
    let none = MINIMAL_V1.replace("\"format_version\": 1,", "");
    assert!(matches!(
        from_bytes(none.as_bytes()),
        Err(FormatError::NotVextor(_))
    ));
}

#[test]
fn truncated_file_is_refused() {
    let bytes = to_bytes(&fixture().doc).expect("encodes");
    for cut in [1, bytes.len() / 3, bytes.len() / 2, bytes.len() - 2] {
        assert_eq!(
            from_bytes(&bytes[..cut]),
            Err(FormatError::Truncated),
            "cut at {cut}"
        );
    }
}

#[test]
fn empty_and_garbage_files_are_refused() {
    assert_eq!(from_bytes(b""), Err(FormatError::Truncated));
    for junk in [
        &b"not json"[..],
        b"[1,2,3]",
        b"\xff\xfe\x00",
        b"{\"format_version\": \"1\"}",
    ] {
        assert!(
            matches!(from_bytes(junk), Err(FormatError::NotVextor(_))),
            "{junk:?}"
        );
    }
}

#[test]
fn deeply_nested_json_is_refused_without_overflow() {
    let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    assert!(from_bytes(deep.as_bytes()).is_err());
}

#[test]
fn file_over_50_mb_is_refused_before_parsing() {
    let size = usize::try_from(MAX_FILE_BYTES).expect("fits") + 1;
    let big = vec![b' '; size];
    assert_eq!(
        from_bytes(&big),
        Err(FormatError::TooLarge(MAX_FILE_BYTES + 1))
    );
}

#[test]
fn too_many_nodes_are_refused() {
    let mut doc = Document::new("big");
    let layer = doc.layers[0];
    let ids: Vec<_> = (0..MAX_NODES).map(|_| doc.alloc_node_id()).collect();
    for &id in &ids {
        doc.nodes
            .insert(id, shape(id, rect_geom(0.0, 0.0, 1.0, 1.0)));
    }
    doc.nodes
        .get_mut(&layer)
        .and_then(|n| n.container_mut())
        .expect("layer")
        .children = ids;
    doc.validate().expect("valid, just big");
    let bytes = to_bytes(&doc).expect("encodes");
    assert_eq!(
        from_bytes(&bytes),
        Err(FormatError::TooManyNodes(MAX_NODES + 1))
    );
}

#[test]
fn a_file_breaking_an_invariant_is_refused() {
    let dangling = MINIMAL_V1.replace("\"children\": [3]", "\"children\": [3, 99]");
    assert!(matches!(
        from_bytes(dangling.as_bytes()),
        Err(FormatError::Invalid(_))
    ));
}

#[test]
fn error_messages_read_as_sentences() {
    assert_eq!(
        FormatError::TooLarge(60 * 1024 * 1024).to_string(),
        "the file is 60 MB, more than the 50 MB limit"
    );
    assert_eq!(
        FormatError::UnknownVersion(7).to_string(),
        "the file uses format version 7; this Vextor reads version 1"
    );
}
