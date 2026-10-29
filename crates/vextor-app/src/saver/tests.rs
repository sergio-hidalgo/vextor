use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use vextor_render::fixtures;

use super::{Done, Job, PreviewFonts, Saver};
use crate::backups::Backups;
use crate::documents::{DocId, Documents};
use crate::files::read_document;
use crate::files::test_dir::TestDir;
use crate::recovery::Recovery;
use crate::session::Session;

fn doc_id() -> DocId {
    Documents::default().add(Session::new(fixtures::basic_shapes()), None)
}

fn save(doc: DocId, path: &std::path::Path, document: vextor_core::Document, revision: u64) -> Job {
    Job::Save {
        doc,
        path: path.to_path_buf(),
        document: Box::new(document),
        revision,
        preview: PreviewFonts {
            fonts: Arc::default(),
            loaded: true,
        },
    }
}

#[test]
fn a_save_runs_in_the_background_and_reports_its_revision() {
    let dir = TestDir::new();
    let path = dir.path().join("a.vxtr");
    let mut saver = Saver::new(None, || {});
    let id = doc_id();
    saver.send(save(id, &path, fixtures::many_nodes(100), 7));
    assert!(saver.is_saving(id));
    let done = saver.wait();
    assert_eq!(
        done,
        [Done::Saved {
            doc: id,
            path: path.clone(),
            revision: 7
        }]
    );
    assert!(!saver.is_busy(id));
    assert_eq!(
        read_document(&path).expect("loads"),
        fixtures::many_nodes(100)
    );
}

#[test]
fn jobs_run_in_the_order_given() {
    let dir = TestDir::new();
    let path = dir.path().join("a.vxtr");
    let mut saver = Saver::new(None, || {});
    let id = doc_id();
    for n in [1_u64, 2, 3] {
        let count = usize::try_from(n).expect("small");
        saver.send(save(id, &path, fixtures::many_nodes(count), n));
    }
    let revisions: Vec<u64> = saver
        .wait()
        .into_iter()
        .map(|d| match d {
            Done::Saved { revision, .. } => revision,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(revisions, [1, 2, 3]);
    assert_eq!(
        read_document(&path).expect("loads"),
        fixtures::many_nodes(3)
    );
}

#[test]
fn a_failed_save_says_why() {
    let dir = TestDir::new();
    let path = dir.path().join("missing-folder").join("a.vxtr");
    let mut saver = Saver::new(None, || {});
    let id = doc_id();
    saver.send(save(id, &path, fixtures::basic_shapes(), 1));
    match saver.wait().as_slice() {
        [Done::SaveFailed { error, .. }] => {
            assert!(error.starts_with("Couldn't save “a.vxtr”"), "{error}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn overwriting_a_file_backs_it_up_first() {
    let app = TestDir::new();
    let docs = TestDir::new();
    let path = docs.path().join("a.vxtr");
    crate::files::write_document(&path, &fixtures::basic_shapes()).expect("saves");
    let backups = Arc::new(Mutex::new(Backups::open(app.path())));
    let mut saver = Saver::new(Some(Arc::clone(&backups)), || {});
    let id = doc_id();
    saver.send(save(id, &path, fixtures::many_nodes(10), 1));
    saver.send(save(id, &path, fixtures::many_nodes(20), 2));
    assert_eq!(saver.wait().len(), 2);
    let list = backups.lock().expect("lock").of(&path);
    assert_eq!(list.len(), 1, "once per session");
    assert_eq!(
        read_document(&list[0].path).expect("backup loads"),
        fixtures::basic_shapes()
    );
    assert_eq!(
        read_document(&path).expect("loads"),
        fixtures::many_nodes(20)
    );
}

#[test]
fn a_backup_that_fails_refuses_the_save() {
    let app = TestDir::new();
    let docs = TestDir::new();
    let path = docs.path().join("a.vxtr");
    crate::files::write_document(&path, &fixtures::basic_shapes()).expect("saves");
    fs::write(app.path().join("backups"), b"in the way").expect("writes");
    let backups = Arc::new(Mutex::new(Backups::open(app.path())));
    let mut saver = Saver::new(Some(backups), || {});
    let id = doc_id();
    saver.send(save(id, &path, fixtures::many_nodes(10), 1));
    match saver.wait().as_slice() {
        [Done::SaveFailed { error, .. }] => {
            assert!(
                error.starts_with("Couldn't back up “a.vxtr” first"),
                "{error}"
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        read_document(&path).expect("loads"),
        fixtures::basic_shapes()
    );
}

#[test]
fn an_autosave_runs_in_the_background() {
    let app = TestDir::new();
    let mut slot = Recovery::claim(app.path(), Instant::now()).expect("claims");
    let mut saver = Saver::new(None, || {});
    let id = doc_id();
    let job = slot.job(&fixtures::basic_shapes(), None, 4, Instant::now());
    saver.send(Job::Autosave {
        doc: id,
        job: Box::new(job),
    });
    assert!(saver.is_busy(id) && !saver.is_saving(id));
    assert_eq!(
        saver.wait(),
        [Done::Autosaved {
            doc: id,
            revision: 4
        }]
    );
}

/// The `preview_png` stored in the `.vxtr` at `path`, decoded.
fn stored_preview(path: &std::path::Path) -> Option<vextor_render::Pixmap> {
    use base64::Engine as _;
    use std::io::Read as _;
    let mut json = Vec::new();
    flate2::read::GzDecoder::new(fs::File::open(path).expect("opens"))
        .read_to_end(&mut json)
        .expect("gzip");
    let value: serde_json::Value = serde_json::from_slice(&json).expect("json");
    let png = base64::engine::general_purpose::STANDARD
        .decode(value.get("preview_png")?.as_str().expect("text"))
        .expect("base64");
    Some(vextor_render::Pixmap::decode_png(&png).expect("a PNG"))
}

fn save_with_fonts(path: &std::path::Path, document: vextor_core::Document, loaded: bool) -> Done {
    let mut saver = Saver::new(None, || {});
    let id = doc_id();
    let mut job = save(id, path, document, 1);
    if let Job::Save { preview, .. } = &mut job {
        preview.loaded = loaded;
    }
    saver.send(job);
    saver.wait().pop().expect("one result")
}

#[test]
fn a_save_writes_a_preview_512_pixels_on_its_longest_side() {
    let dir = TestDir::new();
    let path = dir.path().join("a.vxtr");
    let doc = fixtures::basic_shapes();
    assert!(matches!(
        save_with_fonts(&path, doc.clone(), true),
        Done::Saved { .. }
    ));
    let png = stored_preview(&path).expect("a preview");
    let (w, h) = (png.width(), png.height());
    assert_eq!(w.max(h), crate::thumbnails::PREVIEW_SIDE, "{w}×{h}");
    let (aw, ah) = vextor_render::fixtures::BASIC_SHAPES_SIZE;
    let ratio = f64::from(w) / f64::from(h);
    assert!((ratio - aw / ah).abs() < 0.01, "{w}×{h} for {aw}×{ah}");
    let centre = png.pixel(w / 2, h / 2).expect("inside");
    assert_eq!(centre.alpha(), 255, "the artwork, not an empty picture");
    assert_eq!(read_document(&path).expect("loads"), doc);
}

#[test]
fn a_save_whose_preview_cannot_be_drawn_still_writes_the_document() {
    let dir = TestDir::new();
    // No artboard: nothing to draw.
    let path = dir.path().join("a.vxtr");
    let doc = vextor_core::Document::new("no artboard");
    assert!(matches!(
        save_with_fonts(&path, doc.clone(), true),
        Done::Saved { .. }
    ));
    assert!(stored_preview(&path).is_none());
    assert_eq!(read_document(&path).expect("loads"), doc);
    // Text while the system fonts are still being read.
    let path = dir.path().join("b.vxtr");
    let doc = fixtures::big::texts(3);
    assert!(matches!(
        save_with_fonts(&path, doc.clone(), false),
        Done::Saved { .. }
    ));
    assert!(stored_preview(&path).is_none());
    assert_eq!(read_document(&path).expect("loads"), doc);
}

#[test]
fn shapes_get_a_preview_while_the_fonts_load() {
    let dir = TestDir::new();
    let path = dir.path().join("a.vxtr");
    assert!(matches!(
        save_with_fonts(&path, fixtures::basic_shapes(), false),
        Done::Saved { .. }
    ));
    assert!(stored_preview(&path).is_some());
}
