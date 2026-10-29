//! The save pipeline (spec 14): no broken or lost files.
//!
//! 1. The document is validated; an invalid one is never written.
//! 2. It is encoded straight into a buffered temporary file in the target
//!    folder, hashed on the way (no full-size copy in memory).
//! 3. The temporary file is flushed to disk (`fsync`).
//! 4. It is read back: same length and hash as what was written, and a
//!    `.vxtr` must load to a document equal to the one saved.
//! 5. It is renamed over the target, and the folder is synced.
//! 6. On any failure the target is untouched and the temporary file is
//!    removed; the error says which step failed.
//!
//! Exports go through steps 2–6 with their bytes. Every file operation
//! goes through [`Disk`], so tests can make any step fail.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;
use vextor_core::{Document, Fnv1a, FormatError, MAX_FILE_BYTES, to_writer_with_preview};

use crate::files::{FileError, decode};

/// A temporary file being written.
pub trait TempFile: Write {
    /// Flushes the file's data to the disk.
    ///
    /// # Errors
    /// The OS refused or failed the sync.
    fn sync(&mut self) -> io::Result<()>;
}

/// The file operations a save needs.
pub trait Disk {
    /// Creates (or truncates) `path` for writing.
    ///
    /// # Errors
    /// I/O errors.
    fn create(&self, path: &Path) -> io::Result<Box<dyn TempFile>>;

    /// Reads at most `limit + 1` bytes of `path`.
    ///
    /// # Errors
    /// I/O errors.
    fn read(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>>;

    /// Renames `from` over `to`.
    ///
    /// # Errors
    /// I/O errors; `to` is then unchanged.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Removes `path`, ignoring a file that is already gone.
    fn remove(&self, path: &Path);

    /// Makes a rename in `dir` durable where the OS allows syncing a
    /// folder; a failure here does not undo the save.
    fn sync_dir(&self, dir: &Path);
}

/// The real file system.
pub struct RealDisk;

struct RealFile(BufWriter<File>);

impl Write for RealFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl TempFile for RealFile {
    fn sync(&mut self) -> io::Result<()> {
        self.0.flush()?;
        self.0.get_ref().sync_all()
    }
}

impl Disk for RealDisk {
    fn create(&self, path: &Path) -> io::Result<Box<dyn TempFile>> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        Ok(Box::new(RealFile(BufWriter::with_capacity(1 << 16, file))))
    }

    fn read(&self, path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        File::open(path)?
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove(&self, path: &Path) {
        let _ = fs::remove_file(path);
    }

    fn sync_dir(&self, dir: &Path) {
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
    }
}

/// Counts and hashes what passes through to `inner`.
struct Hashing<W> {
    inner: W,
    hash: Fnv1a,
    len: u64,
}

impl<W: Write> Write for Hashing<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hash.update(&buf[..n]);
        self.len += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Saves `doc` at `path` as a compressed `.vxtr` through every step of the
/// pipeline.
///
/// # Errors
/// [`FileError::Invalid`] for a document that breaks an invariant,
/// [`FileError::Verify`] when the file read back differs, I/O and
/// encoding errors otherwise; `path` is then unchanged.
pub fn save_document(disk: &dyn Disk, path: &Path, doc: &Document) -> Result<(), FileError> {
    save_document_with_preview(disk, path, doc, None)
}

/// [`save_document`] with `preview`, a PNG of the document, stored in the
/// file for Finder (spec 14d).
///
/// # Errors
/// As [`save_document`].
pub fn save_document_with_preview(
    disk: &dyn Disk,
    path: &Path,
    doc: &Document,
    preview: Option<&[u8]>,
) -> Result<(), FileError> {
    if let Err(violations) = doc.validate() {
        let first = violations
            .first()
            .map_or_else(String::new, ToString::to_string);
        return Err(FileError::Invalid(first));
    }
    write_verified(
        disk,
        path,
        |w| {
            // The JSON writer makes many tiny writes; gzip wants big ones.
            let mut json =
                BufWriter::with_capacity(1 << 16, GzEncoder::new(w, Compression::default()));
            to_writer_with_preview(&mut json, doc, preview)?;
            json.into_inner()
                .map_err(|e| FormatError::Encode(e.error().to_string()))?
                .finish()
                .map_err(|e| FormatError::Encode(e.to_string()))?;
            Ok(())
        },
        |bytes| match decode(bytes) {
            Ok(back) if back == *doc => Ok(()),
            Ok(_) => Err(FileError::Verify(
                "it reads back as a different document".to_owned(),
            )),
            Err(e) => Err(FileError::Verify(format!("it does not read back: {e}"))),
        },
    )
}

/// Writes `bytes` at `path` through steps 2–6 (exports, settings).
///
/// # Errors
/// I/O errors, and [`FileError::Verify`] when the file read back differs;
/// `path` is then unchanged.
pub fn save_bytes(disk: &dyn Disk, path: &Path, bytes: &[u8]) -> Result<(), FileError> {
    write_verified(
        disk,
        path,
        |w| {
            w.write_all(bytes)?;
            Ok(())
        },
        |_| Ok(()),
    )
}

/// The temporary file a save of `path` writes first: hidden, next to it,
/// named after this process.
pub fn temp_path(path: &Path) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let mut tmp = std::ffi::OsString::from(".");
    tmp.push(name);
    tmp.push(format!(".{}.tmp", std::process::id()));
    Ok(folder(path).join(tmp))
}

fn folder(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn write_verified(
    disk: &dyn Disk,
    path: &Path,
    body: impl FnOnce(&mut dyn Write) -> Result<(), FileError>,
    check: impl FnOnce(&[u8]) -> Result<(), FileError>,
) -> Result<(), FileError> {
    let tmp = temp_path(path)?;
    let result = (|| {
        let mut out = Hashing {
            inner: disk.create(&tmp)?,
            hash: Fnv1a::default(),
            len: 0,
        };
        body(&mut out)?;
        out.flush()?;
        out.inner.sync()?;
        let (hash, len) = (out.hash.finish(), out.len);
        drop(out);

        let back = disk.read(&tmp, len.max(MAX_FILE_BYTES))?;
        if back.len() as u64 != len || vextor_core::fnv1a(&back) != hash {
            return Err(FileError::Verify(format!(
                "{} bytes were written but {} read back",
                len,
                back.len()
            )));
        }
        check(&back)?;
        disk.rename(&tmp, path)?;
        disk.sync_dir(folder(path));
        Ok(())
    })();
    if result.is_err() {
        disk.remove(&tmp);
    }
    result
}

#[cfg(test)]
mod tests;
