//! Backups before overwriting an existing file (spec 14).
//!
//! The first time in a session that a save or an export is about to
//! replace a file that already exists, the file is first copied byte for
//! byte to `<app data>/backups/<hash of its path>/`, and the copy is
//! checked (size, hash, then every byte) before the write goes ahead. If
//! that fails, the write is refused. Opening a file makes no backup.
//!
//! Each folder holds an `index.json` naming the original file. Backups are
//! pruned oldest first: at most [`Settings::per_file`] per file and
//! [`Settings::total_mb`] in all, after each backup and at startup.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use vextor_core::{Fnv1a, fnv1a};

use crate::files::write_atomic;

/// Name of the backups folder inside the app-data folder.
const FOLDER: &str = "backups";
/// The settings file, inside the backups folder.
const SETTINGS: &str = "settings.json";
/// Names the original file inside each backup folder.
const INDEX: &str = "index.json";
/// Largest settings or index file read.
const MAX_SMALL_FILE: u64 = 64 * 1024;

/// Backups made by this process so far; part of the name so two backups
/// within one millisecond get their own (I-001).
static MADE: AtomicU64 = AtomicU64::new(0);

/// What the user chose (File ▸ Restore from Backup…).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Backups are made at all.
    pub enabled: bool,
    /// Most backups kept per file.
    pub per_file: usize,
    /// Most space all backups take, in MB.
    pub total_mb: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            per_file: 5,
            total_mb: 100,
        }
    }
}

impl Settings {
    /// The smallest and largest values the settings accept.
    pub const PER_FILE: std::ops::RangeInclusive<usize> = 1..=50;
    pub const TOTAL_MB: std::ops::RangeInclusive<u64> = 10..=2_000;

    fn clamped(self) -> Self {
        Self {
            enabled: self.enabled,
            per_file: self
                .per_file
                .clamp(*Self::PER_FILE.start(), *Self::PER_FILE.end()),
            total_mb: self
                .total_mb
                .clamp(*Self::TOTAL_MB.start(), *Self::TOTAL_MB.end()),
        }
    }
}

/// One backup copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backup {
    pub path: PathBuf,
    /// When it was made.
    pub made: SystemTime,
    /// Size in bytes.
    pub bytes: u64,
    /// Made-at milliseconds and sequence number, for ordering backups
    /// made within the same millisecond.
    order: (u64, u64),
}

/// The backups folder and which files were backed up this session.
pub struct Backups {
    dir: PathBuf,
    settings: Settings,
    /// Canonical paths backed up (or found missing) this session.
    done: BTreeSet<PathBuf>,
}

/// The `index.json` of one backup folder.
#[derive(Serialize, Deserialize)]
struct Index {
    original: PathBuf,
}

impl Backups {
    /// The backups under `<app_data>/backups`, with the saved settings
    /// (defaults when unreadable), pruned to them.
    pub fn open(app_data: &Path) -> Self {
        let dir = app_data.join(FOLDER);
        let settings = read_small(&dir.join(SETTINGS))
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .unwrap_or_default()
            .clamped();
        let backups = Self {
            dir,
            settings,
            done: BTreeSet::new(),
        };
        backups.prune(None);
        backups
    }

    #[must_use]
    pub fn settings(&self) -> Settings {
        self.settings
    }

    /// Changes and saves the settings, then prunes to them.
    ///
    /// # Errors
    /// The settings could not be written (they still apply this session).
    pub fn set_settings(&mut self, settings: Settings) -> io::Result<()> {
        self.settings = settings.clamped();
        self.prune(None);
        fs::create_dir_all(&self.dir)?;
        let bytes = serde_json::to_vec_pretty(&self.settings).map_err(io::Error::other)?;
        write_atomic(&self.dir.join(SETTINGS), &bytes)
    }

    /// Before `path` is overwritten: backs it up if it exists, backups are
    /// on and it was not backed up yet this session.
    ///
    /// # Errors
    /// The copy could not be made or did not check out; the caller must
    /// then not write to `path`.
    pub fn before_overwrite(&mut self, path: &Path) -> io::Result<()> {
        if !self.settings.enabled {
            return Ok(());
        }
        let Ok(canonical) = fs::canonicalize(path) else {
            return Ok(()); // a new file: nothing to lose
        };
        if self.done.contains(&canonical) {
            return Ok(());
        }
        let made = self.copy(&canonical)?;
        self.done.insert(canonical);
        self.prune(Some(&made));
        Ok(())
    }

    /// A save made `path` (perhaps under a new name): the file now holds
    /// this session's work, so later saves need no backup of it.
    pub fn written(&mut self, path: &Path) {
        if let Ok(canonical) = fs::canonicalize(path) {
            self.done.insert(canonical);
        }
    }

    /// The backups of `path`, newest first.
    #[must_use]
    pub fn of(&self, path: &Path) -> Vec<Backup> {
        let Ok(canonical) = fs::canonicalize(path) else {
            return Vec::new();
        };
        let mut list = entries(&self.folder_of(&canonical));
        list.sort_by_key(|b| std::cmp::Reverse(b.order));
        list
    }

    /// Space all backups take, in bytes.
    #[must_use]
    pub fn usage(&self) -> u64 {
        self.folders()
            .iter()
            .flat_map(|f| entries(f))
            .map(|b| b.bytes)
            .sum()
    }

    fn folder_of(&self, canonical: &Path) -> PathBuf {
        let key = fnv1a(canonical.as_os_str().as_encoded_bytes());
        self.dir.join(format!("{key:016x}"))
    }

    fn folders(&self) -> Vec<PathBuf> {
        let Ok(read) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        read.filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect()
    }

    /// Copies `canonical` into its folder and checks the copy.
    fn copy(&self, canonical: &Path) -> io::Result<PathBuf> {
        let folder = self.folder_of(canonical);
        fs::create_dir_all(&folder)?;
        let index = serde_json::to_vec(&Index {
            original: canonical.to_path_buf(),
        })
        .map_err(io::Error::other)?;
        write_atomic(&folder.join(INDEX), &index)?;

        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        let n = MADE.fetch_add(1, Ordering::Relaxed);
        let name = canonical
            .file_name()
            .map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
        let target = folder.join(format!("{millis}-{n}-{name}"));
        let partial = folder.join(format!(".{millis}-{n}.partial"));
        let checked = (|| {
            let copied = fs::copy(canonical, &partial)?;
            File::open(&partial)?.sync_all()?;
            if copied != fs::metadata(canonical)?.len() || !same_bytes(canonical, &partial)? {
                return Err(io::Error::other("the backup copy does not match the file"));
            }
            fs::rename(&partial, &target)
        })();
        if checked.is_err() {
            let _ = fs::remove_file(&partial);
        }
        checked.map(|()| target)
    }

    /// Deletes the oldest backups past the settings, never `keep`.
    fn prune(&self, keep: Option<&Path>) {
        let mut all = Vec::new();
        for folder in self.folders() {
            let mut list = entries(&folder);
            list.sort_by_key(|b| std::cmp::Reverse(b.order));
            for old in list.split_off(list.len().min(self.settings.per_file)) {
                if Some(old.path.as_path()) != keep {
                    let _ = fs::remove_file(&old.path);
                }
            }
            if list.is_empty() {
                let _ = fs::remove_dir_all(&folder);
            }
            all.extend(list);
        }
        all.sort_by_key(|b| b.order);
        let limit = self.settings.total_mb * 1024 * 1024;
        let mut total: u64 = all.iter().map(|b| b.bytes).sum();
        for old in &all {
            if total <= limit {
                break;
            }
            if Some(old.path.as_path()) == keep {
                continue;
            }
            if fs::remove_file(&old.path).is_ok() {
                total -= old.bytes;
            }
        }
    }
}

/// The backups in one folder, unordered.
fn entries(folder: &Path) -> Vec<Backup> {
    let Ok(read) = fs::read_dir(folder) else {
        return Vec::new();
    };
    read.filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let mut parts = name.splitn(3, '-');
            let millis: u64 = parts.next()?.parse().ok()?;
            let n: u64 = parts.next()?.parse().ok()?;
            parts.next()?;
            let meta = e.metadata().ok().filter(fs::Metadata::is_file)?;
            Some(Backup {
                path: e.path(),
                made: UNIX_EPOCH + std::time::Duration::from_millis(millis),
                bytes: meta.len(),
                order: (millis, n),
            })
        })
        .collect()
}

/// Whether two files hold the same bytes: hash first, then each byte.
fn same_bytes(a: &Path, b: &Path) -> io::Result<bool> {
    let (mut fa, mut fb) = (File::open(a)?, File::open(b)?);
    let (mut ha, mut hb) = (Fnv1a::default(), Fnv1a::default());
    let (mut ba, mut bb) = (vec![0; 1 << 16], vec![0; 1 << 16]);
    loop {
        let na = read_full(&mut fa, &mut ba)?;
        let nb = read_full(&mut fb, &mut bb)?;
        if na != nb || ba[..na] != bb[..nb] {
            return Ok(false);
        }
        ha.update(&ba[..na]);
        hb.update(&bb[..nb]);
        if na == 0 {
            return Ok(ha.finish() == hb.finish());
        }
    }
}

/// Fills `buf` as far as the file allows; the count read.
fn read_full(f: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

fn read_small(path: &Path) -> Option<Vec<u8>> {
    let file = File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_SMALL_FILE {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_SMALL_FILE).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// How long ago `made` was, for the restore list ("5 min ago").
#[must_use]
pub fn age(made: SystemTime, now: SystemTime) -> String {
    let secs = now.duration_since(made).map_or(0, |d| d.as_secs());
    match secs {
        0..60 => "just now".to_owned(),
        60..3_600 => format!("{} min ago", secs / 60),
        3_600..86_400 => format!("{} h ago", secs / 3_600),
        _ => match secs / 86_400 {
            1 => "1 day ago".to_owned(),
            d => format!("{d} days ago"),
        },
    }
}

/// `bytes` for people: "512 KB", "3.4 MB".
#[must_use]
pub fn size(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)] // shown with one decimal
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1.0 {
        format!("{mb:.1} MB")
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

#[cfg(test)]
mod tests;
