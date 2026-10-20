//! Autosave and crash recovery.
//!
//! Each open document has its own slot (spec 05f). While the document has
//! unsaved changes, a copy is written every [`INTERVAL`] to
//! `<app data>/recovery/<id>.vxtr` (plus `<id>.path` with the file it
//! came from, if any). A clean save, closing the document or a clean quit
//! deletes it, so a copy found at launch means Vextor did not exit cleanly.
//!
//! A slot holds an OS lock on `<id>.lock` for as long as its document is
//! open. The OS drops the lock when the process dies, however it dies, so a
//! copy whose lock can be taken is an orphan to offer; a copy whose lock is
//! held belongs to a document open in a running window and is left alone.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use vextor_core::Document;

use crate::files::{self, FileError, read_document, write_document};

/// Time between autosaves of a document with unsaved changes.
pub const INTERVAL: Duration = Duration::from_secs(60);

/// Name of the autosave folder inside the app-data folder.
const FOLDER: &str = "recovery";

/// Slots claimed by this process so far; part of the slot id so two starts
/// within one millisecond get their own slots (I-001).
static STARTS: AtomicU64 = AtomicU64::new(0);

/// One open document's autosave slot.
pub struct Recovery {
    dir: PathBuf,
    id: String,
    /// Held for the life of the process; the OS releases it on any exit.
    lock: Option<File>,
    last_write: Instant,
    /// Revision of the document last written; `None` when nothing is on
    /// disk for this run.
    written: Option<u64>,
}

/// An autosave left behind by a run that did not exit cleanly.
#[derive(Clone, Debug)]
pub struct Orphan {
    dir: PathBuf,
    id: String,
    /// The copy's extension: `vxtr`, or `vextor` from an older version.
    ext: String,
    /// The file the document was opened from or saved to, if any.
    pub original: Option<PathBuf>,
}

impl Recovery {
    /// The autosaves under `<app_data>/recovery` whose owner is gone,
    /// newest first. Called once at launch, before any slot is claimed.
    ///
    /// # Errors
    /// I/O errors creating the folder.
    pub fn orphans(app_data: &Path) -> io::Result<Vec<Orphan>> {
        let dir = app_data.join(FOLDER);
        fs::create_dir_all(&dir)?;
        Ok(find_orphans(&dir))
    }

    /// [`Recovery::orphans`], then [`Recovery::claim`].
    #[cfg(test)]
    fn start(app_data: &Path, now: Instant) -> io::Result<(Self, Vec<Orphan>)> {
        let orphans = Self::orphans(app_data)?;
        Ok((Self::claim(app_data, now)?, orphans))
    }

    /// Claims a fresh slot under `<app_data>/recovery` for one document.
    ///
    /// # Errors
    /// I/O errors creating the folder or the lock.
    pub fn claim(app_data: &Path, now: Instant) -> io::Result<Self> {
        let dir = app_data.join(FOLDER);
        fs::create_dir_all(&dir)?;

        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let n = STARTS.fetch_add(1, Ordering::Relaxed);
        let id = format!("{}-{millis}-{n}", std::process::id());
        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join(format!("{id}.lock")))?;
        lock.try_lock().map_err(|e| match e {
            TryLockError::Error(e) => e,
            TryLockError::WouldBlock => io::Error::other("autosave slot already in use"),
        })?;
        Ok(Self {
            dir,
            id,
            lock: Some(lock),
            last_write: now,
            written: None,
        })
    }

    /// True when an autosave should be written now: there are unsaved
    /// changes not yet autosaved and [`INTERVAL`] has passed since the
    /// last write (or since launch).
    pub fn due(&self, now: Instant, revision: u64, dirty: bool) -> bool {
        self.pending(revision, dirty) && self.time_left(now).is_zero()
    }

    /// True when there are unsaved changes not yet autosaved.
    pub fn pending(&self, revision: u64, dirty: bool) -> bool {
        dirty && self.written != Some(revision)
    }

    /// Time until the interval since the last write has passed.
    pub fn time_left(&self, now: Instant) -> Duration {
        INTERVAL.saturating_sub(now.saturating_duration_since(self.last_write))
    }

    /// Writes `doc` (at `revision`, opened from `original`) to this, replacing the previous copy atomically.
    ///
    /// # Errors
    /// I/O or encoding errors; the previous copy is kept.
    pub fn write(
        &mut self,
        doc: &Document,
        original: Option<&Path>,
        revision: u64,
        now: Instant,
    ) -> Result<(), FileError> {
        self.last_write = now;
        write_document(&self.doc_path(), doc)?;
        match original.and_then(Path::to_str) {
            Some(p) => files::write_atomic(&self.path_path(), p.as_bytes())?,
            None => remove_if_present(&self.path_path())?,
        }
        self.written = Some(revision);
        Ok(())
    }

    /// Deletes this slot's copy (after a clean save). The next autosave
    /// waits a full interval.
    pub fn clear(&mut self, now: Instant) {
        let _ = remove_if_present(&self.doc_path());
        let _ = remove_if_present(&self.path_path());
        self.written = None;
        self.last_write = now;
    }

    /// The document closed, or the app quit cleanly: deletes the copy and
    /// releases and removes the lock.
    pub fn shutdown(&mut self) {
        self.clear(Instant::now());
        if self.lock.take().is_some() {
            let _ = remove_if_present(&self.dir.join(format!("{}.lock", self.id)));
        }
    }

    fn doc_path(&self) -> PathBuf {
        self.dir.join(format!("{}.{}", self.id, files::EXTENSION))
    }

    fn path_path(&self) -> PathBuf {
        self.dir.join(format!("{}.path", self.id))
    }
}

impl Orphan {
    /// Loads the recovered document, with every check a normal open does.
    ///
    /// # Errors
    /// I/O errors and refused content.
    pub fn load(&self) -> Result<Document, FileError> {
        read_document(&self.dir.join(format!("{}.{}", self.id, self.ext)))
    }

    /// What the recovery prompt calls this document.
    pub fn name(&self) -> String {
        self.original
            .as_deref()
            .map_or_else(|| "an unsaved document".to_owned(), files::display_name)
    }

    /// Deletes the orphan's files.
    pub fn discard(&self) {
        for ext in [self.ext.as_str(), "path", "lock"] {
            let _ = remove_if_present(&self.dir.join(format!("{}.{ext}", self.id)));
        }
    }
}

/// Autosaves in `dir` whose owner is gone, newest first. Locks left by a
/// run that died before its first autosave are cleaned up.
fn find_orphans(dir: &Path) -> Vec<Orphan> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(SystemTime, Orphan)> = Vec::new();
    let mut stale_locks = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
            continue;
        };
        let ext = path.extension().and_then(|e| e.to_str());
        if ext == Some("lock") {
            let has_copy = files::OPEN_EXTENSIONS
                .iter()
                .any(|e| dir.join(format!("{id}.{e}")).exists());
            if !has_copy && owner_gone(&path) {
                stale_locks.push(path);
            }
            continue;
        }
        let Some(ext) = ext.filter(|e| files::OPEN_EXTENSIONS.contains(e)) else {
            continue;
        };
        if !owner_gone(&dir.join(format!("{id}.lock"))) {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(UNIX_EPOCH);
        let original = fs::read_to_string(dir.join(format!("{id}.path")))
            .ok()
            .map(PathBuf::from);
        found.push((
            modified,
            Orphan {
                dir: dir.to_path_buf(),
                id,
                ext: ext.to_owned(),
                original,
            },
        ));
    }
    for lock in stale_locks {
        let _ = fs::remove_file(lock);
    }
    found.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    found.into_iter().map(|(_, o)| o).collect()
}

/// True when no running process holds the lock at `lock_path` (or there is
/// no lock file at all).
fn owner_gone(lock_path: &Path) -> bool {
    let Ok(file) = OpenOptions::new().write(true).open(lock_path) else {
        return true;
    };
    match file.try_lock() {
        Ok(()) => true, // released when `file` drops
        // Held by a running window; or, on an error, cannot tell, and
        // offering a live window's copy is worse than not offering a dead
        // one's.
        Err(TryLockError::WouldBlock | TryLockError::Error(_)) => false,
    }
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use vextor_render::fixtures;

    use super::*;
    use crate::files::test_dir::TestDir;

    /// What a crash leaves: the lock is released by the OS, files stay.
    fn crash(mut r: Recovery) {
        r.lock = None;
    }

    #[test]
    fn autosave_is_due_only_with_new_unsaved_changes_after_the_interval() {
        let dir = TestDir::new();
        let t0 = Instant::now();
        let (mut r, _) = Recovery::start(dir.path(), t0).expect("starts");
        let later = t0 + INTERVAL;
        assert!(!r.due(t0, 1, true), "interval not passed");
        assert!(!r.due(later, 1, false), "nothing unsaved");
        assert!(r.due(later, 1, true));

        r.write(&Document::new("d"), None, 1, later)
            .expect("writes");
        assert!(!r.due(later + INTERVAL, 1, true), "already autosaved");
        assert!(!r.due(later + INTERVAL / 2, 2, true), "too soon");
        assert!(r.due(later + INTERVAL, 2, true));
        assert_eq!(r.time_left(later + INTERVAL / 4), INTERVAL * 3 / 4);
    }

    #[test]
    fn killing_the_app_mid_edit_offers_recovery_on_relaunch() {
        let dir = TestDir::new();
        let doc = fixtures::basic_shapes();
        let original = dir.path().join("icon.vextor");
        let (mut first, orphans) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        assert!(orphans.is_empty());
        first
            .write(&doc, Some(&original), 3, Instant::now())
            .expect("writes");
        crash(first);

        let (_second, orphans) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].original.as_deref(), Some(original.as_path()));
        assert_eq!(orphans[0].name(), "icon.vextor");
        assert_eq!(orphans[0].load().expect("loads"), doc);
    }

    #[test]
    fn autosaves_are_vxtr_and_an_older_vextor_copy_is_still_offered() {
        let dir = TestDir::new();
        let doc = fixtures::basic_shapes();
        let (mut first, _) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        first.write(&doc, None, 1, Instant::now()).expect("writes");
        let copy = first.doc_path();
        assert_eq!(copy.extension().and_then(|e| e.to_str()), Some("vxtr"));
        crash(first);
        // As an older version left it.
        fs::rename(&copy, copy.with_extension("vextor")).expect("renames");

        let (_second, orphans) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].load().expect("loads"), doc);
        orphans[0].discard();
        assert!(!copy.with_extension("vextor").exists());
    }

    #[test]
    fn a_clean_save_or_quit_leaves_nothing_to_recover() {
        let dir = TestDir::new();
        let (mut r, _) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        r.write(&Document::new("d"), None, 1, Instant::now())
            .expect("writes");
        r.clear(Instant::now());
        crash(r);
        let (mut r, orphans) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        assert!(orphans.is_empty(), "cleared after save");

        r.write(&Document::new("d"), None, 1, Instant::now())
            .expect("writes");
        r.shutdown();
        let (_r, orphans) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        assert!(orphans.is_empty(), "removed on quit");
    }

    #[test]
    fn another_running_window_is_not_offered_as_a_crash() {
        let dir = TestDir::new();
        let (mut live, _) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        live.write(&Document::new("d"), None, 1, Instant::now())
            .expect("writes");
        let (_other, orphans) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        assert!(orphans.is_empty());
    }

    #[test]
    fn two_starts_in_the_same_millisecond_get_their_own_slots() {
        let dir = TestDir::new();
        // Back-to-back starts land in the same millisecond almost always.
        let slots: Vec<Recovery> = (0..20)
            .map(|_| {
                Recovery::start(dir.path(), Instant::now())
                    .expect("starts")
                    .0
            })
            .collect();
        let mut ids: Vec<&str> = slots.iter().map(|r| r.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), slots.len(), "every start has its own id");
    }

    #[test]
    fn each_document_has_its_own_slot_and_closing_one_keeps_the_other() {
        let dir = TestDir::new();
        let now = Instant::now();
        assert!(Recovery::orphans(dir.path()).expect("scans").is_empty());
        let mut a = Recovery::claim(dir.path(), now).expect("claims");
        let mut b = Recovery::claim(dir.path(), now).expect("claims");
        a.write(&Document::new("a"), None, 1, now).expect("writes");
        b.write(&Document::new("b"), None, 1, now).expect("writes");
        assert!(a.doc_path().exists() && b.doc_path().exists());

        a.shutdown();
        assert!(!a.doc_path().exists(), "closed document's copy is gone");
        assert!(b.doc_path().exists(), "the other document's copy stays");
        assert!(
            Recovery::orphans(dir.path()).expect("scans").is_empty(),
            "an open document's copy is not an orphan"
        );
    }

    #[test]
    fn a_crash_with_two_unsaved_documents_offers_both() {
        let dir = TestDir::new();
        let now = Instant::now();
        let mut a = Recovery::claim(dir.path(), now).expect("claims");
        let mut b = Recovery::claim(dir.path(), now).expect("claims");
        a.write(&Document::new("a"), None, 1, now).expect("writes");
        b.write(&Document::new("b"), None, 1, now).expect("writes");
        crash(a);
        crash(b);
        let orphans = Recovery::orphans(dir.path()).expect("scans");
        let mut names: Vec<String> = orphans
            .iter()
            .map(|o| o.load().expect("loads").name)
            .collect();
        names.sort();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn discarding_an_orphan_deletes_it() {
        let dir = TestDir::new();
        let (mut first, _) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        first
            .write(&Document::new("d"), None, 1, Instant::now())
            .expect("writes");
        crash(first);
        let (_r, orphans) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        assert_eq!(orphans[0].name(), "an unsaved document");
        orphans[0].discard();
        let (_r, orphans) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        assert!(orphans.is_empty());
    }

    #[test]
    fn a_crash_before_the_first_autosave_leaves_no_prompt_and_no_lock() {
        let dir = TestDir::new();
        let (first, _) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        let lock = dir.path().join(FOLDER).join(format!("{}.lock", first.id));
        crash(first);
        let (_r, orphans) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        assert!(orphans.is_empty());
        assert!(!lock.exists(), "stale lock removed");
    }

    #[test]
    fn a_damaged_autosave_fails_to_load_with_a_message() {
        let dir = TestDir::new();
        let (mut first, _) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        first
            .write(&Document::new("d"), None, 1, Instant::now())
            .expect("writes");
        fs::write(first.doc_path(), b"{\"format_version\": 1, \"na").expect("writes");
        crash(first);
        let (_r, orphans) = Recovery::start(dir.path(), Instant::now()).expect("starts");
        let err = orphans[0].load().expect_err("refused");
        assert_eq!(err.to_string(), "the file is incomplete (it was cut off)");
    }
}
