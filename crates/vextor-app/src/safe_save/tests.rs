//! Fault injection (spec 14): whatever step of a save fails, the file that
//! was there is unchanged and still loads.

use std::cell::Cell;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use vextor_core::{Document, NodeId};
use vextor_render::fixtures;

use super::{
    Disk, RealDisk, TempFile, save_bytes, save_document, save_document_with_preview, temp_path,
};
use crate::files::test_dir::TestDir;
use crate::files::{FileError, read_document};

/// The step a [`Faulty`] disk breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fault {
    /// The temporary file can't be created.
    PermissionDenied,
    /// The disk fills up after this many bytes.
    DiskFull(usize),
    /// The disk claims to write everything but keeps only this many bytes.
    ShortWrite(usize),
    /// Flushing to disk fails.
    SyncFails,
    /// The read-back returns other bytes.
    CorruptRead,
    /// The final rename fails.
    RenameFails,
}

/// The real disk with one step broken.
struct Faulty {
    fault: Fault,
    renamed: Cell<bool>,
}

impl Faulty {
    fn new(fault: Fault) -> Self {
        Self {
            fault,
            renamed: Cell::new(false),
        }
    }
}

struct FaultyFile {
    inner: Box<dyn TempFile>,
    fault: Fault,
    written: usize,
}

impl Write for FaultyFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.fault {
            Fault::DiskFull(limit) if self.written + buf.len() > limit => Err(io::Error::new(
                io::ErrorKind::StorageFull,
                "no space left on device",
            )),
            Fault::ShortWrite(keep) => {
                let room = keep.saturating_sub(self.written).min(buf.len());
                self.inner.write_all(&buf[..room])?;
                self.written += buf.len();
                Ok(buf.len())
            }
            _ => {
                let n = self.inner.write(buf)?;
                self.written += n;
                Ok(n)
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl TempFile for FaultyFile {
    fn sync(&mut self) -> io::Result<()> {
        if self.fault == Fault::SyncFails {
            return Err(io::Error::other("input/output error"));
        }
        self.inner.sync()
    }
}

impl Disk for Faulty {
    fn create(&self, path: &Path) -> io::Result<Box<dyn TempFile>> {
        if self.fault == Fault::PermissionDenied {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "permission denied",
            ));
        }
        Ok(Box::new(FaultyFile {
            inner: RealDisk.create(path)?,
            fault: self.fault,
            written: 0,
        }))
    }

    fn read(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        let mut bytes = RealDisk.read(path, limit)?;
        if self.fault == Fault::CorruptRead
            && let Some(b) = bytes.get_mut(20)
        {
            *b ^= 0xff;
        }
        Ok(bytes)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.fault == Fault::RenameFails {
            return Err(io::Error::other("rename refused"));
        }
        self.renamed.set(true);
        RealDisk.rename(from, to)
    }

    fn remove(&self, path: &Path) {
        RealDisk.remove(path);
    }

    fn sync_dir(&self, dir: &Path) {
        RealDisk.sync_dir(dir);
    }
}

const FAULTS: [Fault; 7] = [
    Fault::PermissionDenied,
    Fault::DiskFull(0),
    Fault::DiskFull(100),
    Fault::ShortWrite(100),
    Fault::SyncFails,
    Fault::CorruptRead,
    Fault::RenameFails,
];

/// Nothing but the saved file is left in `dir`.
fn only_file(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .expect("lists")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn every_injected_failure_leaves_the_original_file_loading_as_it_was() {
    let dir = TestDir::new();
    let path = dir.path().join("icon.vxtr");
    let original = fixtures::basic_shapes();
    save_document(&RealDisk, &path, &original).expect("first save");
    let bytes_before = fs::read(&path).expect("reads");

    let changed = fixtures::many_nodes(50);
    for fault in FAULTS {
        let disk = Faulty::new(fault);
        let err = save_document(&disk, &path, &changed).expect_err("the save fails");
        assert!(!disk.renamed.get(), "{fault:?}: nothing renamed");
        assert_eq!(fs::read(&path).expect("reads"), bytes_before, "{fault:?}");
        assert_eq!(read_document(&path).expect("loads"), original, "{fault:?}");
        assert_eq!(only_file(dir.path()), ["icon.vxtr"], "{fault:?}: {err}");
    }
}

#[test]
fn a_failed_first_save_leaves_no_file_behind() {
    let dir = TestDir::new();
    let path = dir.path().join("new.vxtr");
    for fault in FAULTS {
        let result = save_document(&Faulty::new(fault), &path, &fixtures::basic_shapes());
        assert!(result.is_err(), "{fault:?}");
        assert_eq!(only_file(dir.path()), Vec::<String>::new(), "{fault:?}");
    }
}

#[test]
fn a_short_write_is_caught_by_the_read_back() {
    let dir = TestDir::new();
    let path = dir.path().join("a.vxtr");
    let err = save_document(
        &Faulty::new(Fault::ShortWrite(100)),
        &path,
        &fixtures::basic_shapes(),
    )
    .expect_err("caught");
    assert!(matches!(err, FileError::Verify(_)), "{err}");
    assert!(err.to_string().contains("the old file is unchanged"));
}

#[test]
fn an_export_failure_leaves_the_old_export() {
    let dir = TestDir::new();
    let path = dir.path().join("icon.svg");
    fs::write(&path, b"<svg>old</svg>").expect("writes");
    // Longer than every fault's byte count.
    let new = format!("<svg>{}</svg>", "<path d=\"M0 0h1\"/>".repeat(40));
    for fault in FAULTS {
        let result = save_bytes(&Faulty::new(fault), &path, new.as_bytes());
        assert!(result.is_err(), "{fault:?}");
        assert_eq!(
            fs::read(&path).expect("reads"),
            b"<svg>old</svg>",
            "{fault:?}"
        );
    }
    save_bytes(&RealDisk, &path, new.as_bytes()).expect("saves");
    assert_eq!(fs::read(&path).expect("reads"), new.as_bytes());
}

#[test]
fn a_process_killed_mid_save_leaves_the_original_loading() {
    // Killed after part of the temporary file was written: the truncated
    // temp file stays behind, the original is untouched, and the next
    // save replaces the leftover.
    let dir = TestDir::new();
    let path = dir.path().join("icon.vxtr");
    let original = fixtures::basic_shapes();
    save_document(&RealDisk, &path, &original).expect("saves");
    let tmp = temp_path(&path).expect("temp path");
    fs::write(&tmp, &fs::read(&path).expect("reads")[..10]).expect("writes");

    assert_eq!(read_document(&path).expect("loads"), original);
    let next = fixtures::many_nodes(5);
    save_document(&RealDisk, &path, &next).expect("saves");
    assert_eq!(read_document(&path).expect("loads"), next);
    assert!(!tmp.exists(), "leftover replaced and renamed");
}

#[test]
fn an_invalid_document_is_never_written() {
    let dir = TestDir::new();
    let path = dir.path().join("bad.vxtr");
    let mut doc = fixtures::basic_shapes();
    // A layer listing a child that does not exist breaks an invariant.
    let layer = doc.layers[0];
    if let Some(vextor_core::NodeKind::Layer(c)) = doc.nodes.get_mut(&layer).map(|n| &mut n.kind) {
        c.children.push(NodeId(9_999));
    }
    let err = save_document(&RealDisk, &path, &doc).expect_err("refused");
    assert!(matches!(err, FileError::Invalid(_)), "{err}");
    assert_eq!(only_file(dir.path()), Vec::<String>::new());
}

#[test]
fn saved_files_are_compressed_and_load_back_equal() {
    let dir = TestDir::new();
    let path = dir.path().join("big.vxtr");
    let doc = fixtures::many_nodes(2_000);
    save_document(&RealDisk, &path, &doc).expect("saves");
    let bytes = fs::read(&path).expect("reads");
    assert_eq!(bytes[..2], [0x1f, 0x8b], "gzip");
    let plain = vextor_core::to_bytes(&doc).expect("encodes");
    assert!(
        bytes.len() * 2 < plain.len(),
        "{} vs {}",
        bytes.len(),
        plain.len()
    );
    assert_eq!(read_document(&path).expect("loads"), doc);
}

#[test]
fn an_uncompressed_file_still_opens() {
    let dir = TestDir::new();
    let path = dir.path().join("plain.vxtr");
    let doc = fixtures::basic_shapes();
    fs::write(&path, vextor_core::to_bytes_pretty(&doc).expect("encodes")).expect("writes");
    assert_eq!(read_document(&path).expect("loads"), doc);
}

#[test]
fn a_compressed_file_unpacking_past_the_limit_is_refused() {
    // 1 MB of gzip that would unpack to 60 MB of spaces.
    use flate2::Compression;
    use flate2::write::GzEncoder;
    let mut gz = GzEncoder::new(Vec::new(), Compression::best());
    let chunk = vec![b' '; 1 << 20];
    for _ in 0..60 {
        gz.write_all(&chunk).expect("compresses");
    }
    let bomb = gz.finish().expect("finishes");
    assert!(bomb.len() < 1 << 20);
    let err = crate::files::decode(&bomb).expect_err("refused");
    assert!(
        err.to_string().contains("more than the 50 MB limit"),
        "{err}"
    );
}

#[test]
fn a_truncated_compressed_file_is_refused_with_a_message() {
    let dir = TestDir::new();
    let path = dir.path().join("cut.vxtr");
    save_document(&RealDisk, &path, &fixtures::many_nodes(200)).expect("saves");
    let bytes = fs::read(&path).expect("reads");
    fs::write(&path, &bytes[..bytes.len() / 2]).expect("writes");
    let err = read_document(&path).expect_err("refused");
    assert_eq!(err.to_string(), "the file is incomplete (it was cut off)");
}

#[test]
fn the_empty_document_round_trips() {
    let dir = TestDir::new();
    let path = dir.path().join("empty.vxtr");
    let doc = Document::new("empty");
    save_document(&RealDisk, &path, &doc).expect("saves");
    assert_eq!(read_document(&path).expect("loads"), doc);
}

/// Spec 14 budgets on the 20 000-node fixture: the UI thread only clones
/// (≤ 16 ms); the background save, with backup-free write, read back and
/// check, ≤ 300 ms; opening ≤ 300 ms. Spec 14d: the save with its
/// preview drawn stays within +50 % of the save without.
/// `cargo test --release -p vextor-app time_save_20k -- --ignored --nocapture`
#[test]
#[ignore = "timing, release only"]
fn time_save_20k() {
    use std::time::Instant;
    let dir = TestDir::new();
    let path = dir.path().join("big.vxtr");
    let doc = fixtures::big::nodes(20_000);
    let best = |f: &mut dyn FnMut()| {
        (0..5)
            .map(|_| {
                let t = Instant::now();
                f();
                t.elapsed().as_secs_f64() * 1000.0
            })
            .fold(f64::INFINITY, f64::min)
    };
    let snapshot = best(&mut || drop(std::hint::black_box(doc.clone())));
    let save = best(&mut || save_document(&RealDisk, &path, &doc).expect("saves"));
    let plain = fs::metadata(&path).expect("exists").len();
    let fonts = vextor_geom::Fonts::default();
    let mut png = Vec::new();
    let with_preview = best(&mut || {
        png = crate::thumbnails::preview_png(&doc, &fonts).expect("drawn");
        save_document_with_preview(&RealDisk, &path, &doc, Some(&png)).expect("saves");
    });
    let open = best(&mut || drop(read_document(&path).expect("loads")));
    let size = fs::metadata(&path).expect("exists").len();
    println!(
        "20k nodes: UI snapshot {snapshot:.1} ms, background save {save:.1} ms, \
         with preview {with_preview:.1} ms, open {open:.1} ms, \
         file {} KB → {} KB (PNG {} KB)",
        plain / 1024,
        size / 1024,
        png.len() / 1024
    );
}
