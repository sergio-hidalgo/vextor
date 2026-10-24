//! Reading and writing `.vxtr` files, the app-data folder and the
//! recent-files list. The bytes themselves are defined by
//! [`vextor_core::to_bytes`] / [`vextor_core::from_bytes`].

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use vextor_core::{Document, FormatError, MAX_FILE_BYTES, from_bytes, to_bytes};

/// File extension of documents, without the dot.
pub const EXTENSION: &str = "vxtr";

/// True when `path` ends in the document extension (any letter case).
pub fn has_document_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
}

/// Most entries in File ▸ Open Recent.
pub const MAX_RECENT: usize = 10;

/// Why a document could not be opened or saved.
#[derive(Debug, Error)]
pub enum FileError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Format(#[from] FormatError),
}

/// The OS folder for Vextor's own files (recent list, autosaves), e.g.
/// `~/Library/Application Support/Vextor` on macOS. `None` when the OS has
/// no home folder; those features are then off.
pub fn app_data_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "Vextor").map(|d| d.data_dir().to_path_buf())
}

/// Reads and parses the document at `path`. The size is checked before
/// anything is read, and reading stops at the limit even if the file grows.
///
/// # Errors
/// I/O errors, and [`FormatError`] for refused content.
pub fn read_document(path: &Path) -> Result<Document, FileError> {
    Ok(from_bytes(&read_limited(path)?)?)
}

/// Reads at most [`MAX_FILE_BYTES`] bytes; a bigger file is
/// [`FormatError::TooLarge`] without reading it.
pub(crate) fn read_limited(path: &Path) -> Result<Vec<u8>, FileError> {
    let file = File::open(path)?;
    let len = file.metadata()?.len();
    if len > MAX_FILE_BYTES {
        return Err(FormatError::TooLarge(len).into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    let read = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if read > MAX_FILE_BYTES {
        return Err(FormatError::TooLarge(read).into());
    }
    Ok(bytes)
}

/// Saves `doc` at `path` atomically ([`write_atomic`]).
///
/// # Errors
/// I/O errors; the file at `path` is then unchanged.
pub fn write_document(path: &Path, doc: &Document) -> Result<(), FileError> {
    write_atomic(path, &to_bytes(doc)?)?;
    Ok(())
}

/// Writes `bytes` to a temporary file in `path`'s folder, flushes it to
/// disk, then renames it over `path`. A crash at any point leaves either
/// the old file or the new one, never a half-written file.
///
/// # Errors
/// I/O errors; the temporary file is removed and `path` is unchanged.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".{}.tmp", std::process::id()));
    let tmp = dir.join(tmp_name);

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)?;
        // Make the rename itself durable where the OS allows syncing a
        // folder; failure here does not undo the save.
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// `path` with the `.vxtr` extension added when it has none or another.
pub fn with_extension(path: PathBuf) -> PathBuf {
    if has_document_extension(&path) {
        path
    } else {
        let mut s = path.into_os_string();
        s.push(".");
        s.push(EXTENSION);
        PathBuf::from(s)
    }
}

/// The name a file is shown with: its file name, or the whole path when it
/// has none.
pub fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// Longest document name, in characters.
pub const MAX_NAME_CHARS: usize = 64;

/// `input` (typed on a tab) as a document name, which is also a file name
/// without its extension: control characters removed, trimmed, a typed
/// `.vxtr` dropped, cut to [`MAX_NAME_CHARS`].
///
/// # Errors
/// Why the name can't be used: empty, hidden (leading dot), or holding a
/// character that is a folder separator somewhere (`/`, `\`, `:`).
pub fn clean_name(input: &str) -> Result<String, &'static str> {
    let cleaned: String = input.chars().filter(|c| !c.is_control()).collect();
    let mut name = cleaned.trim();
    let suffix = format!(".{EXTENSION}");
    if name.len() > suffix.len() && name.to_lowercase().ends_with(&suffix) {
        name = name[..name.len() - suffix.len()].trim_end();
    }
    if name.is_empty() {
        return Err("A name can't be empty.");
    }
    if name.starts_with('.') {
        return Err("A name can't start with a dot.");
    }
    if name.contains(['/', '\\', ':']) {
        return Err("A name can't contain /, \\ or :.");
    }
    let cut: String = name.chars().take(MAX_NAME_CHARS).collect();
    Ok(cut.trim_end().to_owned())
}

/// Renames the document file `from` to `<name>.vxtr` in the same folder
/// and returns the new path. Never replaces another file: a name already
/// taken is refused. Changing only the letter case is allowed.
///
/// # Errors
/// The name is taken, or the OS refused the rename (the file is unchanged).
pub fn rename_document(from: &Path, name: &str) -> io::Result<PathBuf> {
    let to = from.with_file_name(format!("{name}.{EXTENSION}"));
    if to == from {
        return Ok(to);
    }
    let taken = || {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("a file named “{}” is already there", display_name(&to)),
        )
    };
    // The same file under another letter case (case-insensitive disks).
    let same_file = matches!(
        (fs::canonicalize(from), fs::canonicalize(&to)),
        (Ok(a), Ok(b)) if a == b
    );
    if same_file {
        fs::rename(from, &to)?;
        return Ok(to);
    }
    // A hard link fails when the name is taken, with no gap between the
    // check and the rename; disks without hard links fall back below.
    match fs::hard_link(from, &to) {
        Ok(()) => {
            if let Err(e) = fs::remove_file(from) {
                let _ = fs::remove_file(&to);
                return Err(e);
            }
            Ok(to)
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(taken()),
        Err(_) if to.exists() => Err(taken()),
        Err(_) => {
            fs::rename(from, &to)?;
            Ok(to)
        }
    }
}

/// File ▸ Open Recent: most recent first, no duplicates, at most
/// [`MAX_RECENT`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentFiles {
    paths: Vec<PathBuf>,
}

impl RecentFiles {
    /// File name of the list inside the app-data folder.
    const FILE: &str = "recent.json";

    /// The list saved in `dir`, or an empty one when it is missing or
    /// unreadable (it is a convenience, never an error).
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(Self::FILE);
        let Ok(file) = File::open(&path) else {
            return Self::default();
        };
        let mut bytes = Vec::new();
        // The list is tiny; anything over 1 MB is not ours.
        if file.take(1024 * 1024).read_to_end(&mut bytes).is_err() {
            return Self::default();
        }
        let mut list: Self = serde_json::from_slice(&bytes).unwrap_or_default();
        list.paths.truncate(MAX_RECENT);
        list
    }

    /// Writes the list into `dir`.
    ///
    /// # Errors
    /// I/O errors.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        write_atomic(&dir.join(Self::FILE), &bytes)
    }

    /// The paths, most recent first.
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Moves `path` to the front.
    pub fn push(&mut self, path: PathBuf) {
        self.paths.retain(|p| p != &path);
        self.paths.insert(0, path);
        self.paths.truncate(MAX_RECENT);
    }

    /// Removes `path` (e.g. it no longer opens).
    pub fn remove(&mut self, path: &Path) {
        self.paths.retain(|p| p != path);
    }

    /// Empties the list.
    pub fn clear(&mut self) {
        self.paths.clear();
    }
}

#[cfg(test)]
pub mod test_dir {
    //! Throwaway folders for file tests, removed on drop.

    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A fresh empty folder under the OS temp dir.
    pub struct TestDir(PathBuf);

    impl TestDir {
        pub fn new() -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("vextor-test-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("temp dir is writable");
            Self(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use vextor_render::fixtures;

    use super::test_dir::TestDir;
    use super::*;

    #[test]
    fn names_are_cleaned_like_file_names() {
        assert_eq!(
            clean_name("  pricing\t card \n").as_deref(),
            Ok("pricing card")
        );
        assert_eq!(clean_name("card.vxtr").as_deref(), Ok("card"));
        assert_eq!(clean_name("card.VXTR ").as_deref(), Ok("card"));
        assert_eq!(
            clean_name(&"x".repeat(80)).map(|n| n.len()),
            Ok(MAX_NAME_CHARS)
        );
        assert!(clean_name("   ").is_err());
        assert!(
            clean_name(".vxtr").is_err(),
            "only the extension is left as a dot name"
        );
        assert!(clean_name(".hidden").is_err());
        for bad in ["a/b", "a\\b", "a:b", "../up"] {
            assert!(clean_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn renaming_moves_the_file_and_keeps_its_content() {
        let dir = TestDir::new();
        let from = dir.path().join("card.vxtr");
        fs::write(&from, b"content").expect("writes");
        let to = rename_document(&from, "pricing").expect("renames");
        assert_eq!(to, dir.path().join("pricing.vxtr"));
        assert_eq!(files_in(dir.path()), ["pricing.vxtr"]);
        assert_eq!(fs::read(&to).expect("reads"), b"content");
        assert_eq!(rename_document(&to, "pricing").expect("same name"), to);
    }

    #[test]
    fn only_vxtr_counts_as_a_document_and_a_typed_one_is_dropped() {
        assert!(has_document_extension(Path::new("/a/b.vxtr")));
        assert!(has_document_extension(Path::new("/a/b.VXTR")));
        assert!(!has_document_extension(Path::new("/a/b.vextor")));
        assert!(!has_document_extension(Path::new("/a/b.svg")));
        assert_eq!(clean_name("card.vxtr").as_deref(), Ok("card"));
        assert_eq!(clean_name("card.vextor").as_deref(), Ok("card.vextor"));
    }

    #[test]
    fn renaming_never_replaces_another_file() {
        let dir = TestDir::new();
        let from = dir.path().join("card.vxtr");
        let other = dir.path().join("pricing.vxtr");
        fs::write(&from, b"card").expect("writes");
        fs::write(&other, b"pricing").expect("writes");
        let e = rename_document(&from, "pricing").expect_err("refused");
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&other).expect("reads"), b"pricing", "untouched");
        assert_eq!(fs::read(&from).expect("reads"), b"card", "untouched");
    }

    #[test]
    fn renaming_can_change_only_the_letter_case() {
        let dir = TestDir::new();
        let from = dir.path().join("card.vxtr");
        fs::write(&from, b"card").expect("writes");
        let to = rename_document(&from, "Card").expect("renames");
        assert_eq!(files_in(dir.path()), ["Card.vxtr"]);
        assert_eq!(fs::read(&to).expect("reads"), b"card");
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("readable")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn every_sample_document_round_trips_through_disk() {
        let dir = TestDir::new();
        let path = dir.path().join("doc.vxtr");
        for doc in [
            Document::new("empty"),
            fixtures::basic_shapes(),
            fixtures::many_nodes(500),
        ] {
            write_document(&path, &doc).expect("saves");
            let back = read_document(&path).expect("loads");
            assert_eq!(back, doc);
            assert_eq!(back.next_id(), doc.next_id());
        }
    }

    #[test]
    fn atomic_write_replaces_the_file_and_leaves_no_temp_file() {
        let dir = TestDir::new();
        let path = dir.path().join("a.vxtr");
        write_atomic(&path, b"old").expect("writes");
        write_atomic(&path, b"new").expect("writes");
        assert_eq!(fs::read(&path).expect("reads"), b"new");
        assert_eq!(files_in(dir.path()), ["a.vxtr"]);
    }

    #[test]
    fn failed_atomic_write_keeps_the_old_file() {
        let dir = TestDir::new();
        let path = dir.path().join("a.vxtr");
        write_atomic(&path, b"old").expect("writes");
        // A folder where the temp file should go makes the write fail.
        let blocker = dir
            .path()
            .join(format!(".a.vxtr.{}.tmp", std::process::id()));
        fs::create_dir(&blocker).expect("creates");
        assert!(write_atomic(&path, b"new").is_err());
        assert_eq!(fs::read(&path).expect("reads"), b"old");
    }

    #[test]
    fn file_over_50_mb_is_refused_before_reading() {
        let dir = TestDir::new();
        let path = dir.path().join("big.vxtr");
        // Sparse: 60 MB long, nothing written, so the test stays fast.
        File::create(&path)
            .and_then(|f| f.set_len(60 * 1024 * 1024))
            .expect("creates");
        let err = read_document(&path).expect_err("refused");
        assert!(
            matches!(err, FileError::Format(FormatError::TooLarge(_))),
            "{err}"
        );
        assert_eq!(
            err.to_string(),
            "the file is 60 MB, more than the 50 MB limit"
        );
    }

    #[test]
    fn truncated_and_unknown_version_files_fail_with_a_message() {
        let dir = TestDir::new();
        let path = dir.path().join("doc.vxtr");
        write_document(&path, &fixtures::basic_shapes()).expect("saves");
        let bytes = fs::read(&path).expect("reads");

        fs::write(&path, &bytes[..bytes.len() / 2]).expect("writes");
        let err = read_document(&path).expect_err("refused");
        assert_eq!(err.to_string(), "the file is incomplete (it was cut off)");

        let newer = String::from_utf8(bytes).expect("utf-8").replacen(
            "\"format_version\": 3",
            "\"format_version\": 4",
            1,
        );
        fs::write(&path, newer).expect("writes");
        let err = read_document(&path).expect_err("refused");
        assert_eq!(
            err.to_string(),
            "the file uses format version 4; this Vextor reads versions 1 to 3"
        );
    }

    #[test]
    fn missing_file_is_an_io_error() {
        let dir = TestDir::new();
        let err = read_document(&dir.path().join("nope.vxtr")).expect_err("missing");
        assert!(matches!(err, FileError::Io(_)));
    }

    #[test]
    fn extension_is_added_when_missing() {
        assert_eq!(
            with_extension(PathBuf::from("/x/icon")),
            PathBuf::from("/x/icon.vxtr")
        );
        assert_eq!(
            with_extension(PathBuf::from("/x/icon.vxtr")),
            PathBuf::from("/x/icon.vxtr")
        );
        assert_eq!(
            with_extension(PathBuf::from("/x/icon.vextor")),
            PathBuf::from("/x/icon.vextor.vxtr")
        );
        assert_eq!(
            with_extension(PathBuf::from("/x/icon.svg")),
            PathBuf::from("/x/icon.svg.vxtr")
        );
    }

    #[test]
    fn recent_files_keep_order_without_duplicates_and_cap_at_ten() {
        let mut r = RecentFiles::default();
        for i in 0..12 {
            r.push(PathBuf::from(format!("/f{i}.vxtr")));
        }
        r.push(PathBuf::from("/f5.vxtr"));
        assert_eq!(r.paths().len(), MAX_RECENT);
        assert_eq!(r.paths()[0], PathBuf::from("/f5.vxtr"));
        assert_eq!(r.paths()[1], PathBuf::from("/f11.vxtr"));
        assert_eq!(
            r.paths().iter().filter(|p| p.ends_with("f5.vxtr")).count(),
            1
        );
        r.remove(Path::new("/f5.vxtr"));
        assert_eq!(r.paths()[0], PathBuf::from("/f11.vxtr"));
    }

    #[test]
    fn recent_files_survive_a_restart_and_ignore_junk() {
        let dir = TestDir::new();
        let mut r = RecentFiles::default();
        r.push(PathBuf::from("/a.vxtr"));
        r.save(dir.path()).expect("saves");
        assert_eq!(RecentFiles::load(dir.path()), r);

        fs::write(dir.path().join("recent.json"), b"{broken").expect("writes");
        assert_eq!(RecentFiles::load(dir.path()), RecentFiles::default());
        assert_eq!(
            RecentFiles::load(&dir.path().join("missing")),
            RecentFiles::default()
        );
    }
}
