//! The swatch library (spec 09): the colours kept for every document, in a
//! fill list and a stroke list. It belongs to the app, not to a document:
//! a swatch is there in every open and later document, whether or not any
//! document is saved. It lives in `swatches.json` in the app-data folder,
//! saved as soon as it changes; editing it is not an undo step (like the
//! recent-files list).

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use vextor_core::{MAX_SWATCH_NAME_CHARS, MAX_SWATCHES, Rgba, Swatch, SwatchKind};

use crate::files::write_atomic;

/// File name of the library inside the app-data folder.
const FILE: &str = "swatches.json";

/// Largest library file read; anything bigger is not ours.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// The saved form.
#[derive(Default, Serialize, Deserialize)]
struct Saved {
    swatches: Vec<Swatch>,
}

#[derive(Default)]
struct Library {
    swatches: Vec<Swatch>,
    /// Changed since last saved.
    changed: bool,
}

/// The app's swatch library, shared by every open document (each session
/// holds a handle; the app saves it).
#[derive(Clone, Default)]
pub struct Swatches(Arc<Mutex<Library>>);

impl Swatches {
    fn lock(&self) -> MutexGuard<'_, Library> {
        // A panic while holding the lock leaves plain data behind; keep it.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The library saved in `dir`, or an empty one when it is missing,
    /// too big or unreadable (it is a convenience, never an error). More
    /// than [`MAX_SWATCHES`] are dropped and long names cut short.
    #[must_use]
    pub fn load(dir: &Path) -> Self {
        let Ok(file) = File::open(dir.join(FILE)) else {
            return Self::default();
        };
        let mut bytes = Vec::new();
        if file
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() as u64 > MAX_FILE_BYTES
        {
            return Self::default();
        }
        let mut swatches = serde_json::from_slice::<Saved>(&bytes)
            .unwrap_or_default()
            .swatches;
        swatches.truncate(MAX_SWATCHES);
        for s in &mut swatches {
            s.name = s
                .name
                .take()
                .map(|n| clean_name(&n))
                .filter(|n| !n.is_empty());
        }
        Self(Arc::new(Mutex::new(Library {
            swatches,
            changed: false,
        })))
    }

    /// Writes the library into `dir` (a temporary file renamed into place).
    ///
    /// # Errors
    /// I/O errors.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        let saved = Saved {
            swatches: self.lock().swatches.clone(),
        };
        fs::create_dir_all(dir)?;
        let bytes = serde_json::to_vec_pretty(&saved).map_err(io::Error::other)?;
        write_atomic(&dir.join(FILE), &bytes)
    }

    /// True once after any change, so the app saves it.
    pub fn take_changed(&self) -> bool {
        std::mem::take(&mut self.lock().changed)
    }

    /// Every swatch with its index (which [`Swatches::rename`] and
    /// [`Swatches::delete`] take), in the order added.
    #[must_use]
    pub fn list(&self) -> Vec<(usize, Swatch)> {
        self.lock().swatches.iter().cloned().enumerate().collect()
    }

    /// True when `color` can be added to the `kind` list: it is not in it
    /// yet and the library holds fewer than [`MAX_SWATCHES`].
    #[must_use]
    pub fn can_add(&self, kind: SwatchKind, color: Rgba) -> bool {
        let lib = self.lock();
        lib.swatches.len() < MAX_SWATCHES
            && !lib
                .swatches
                .iter()
                .any(|s| s.kind == kind && s.color == color)
    }

    /// Adds `color` to the `kind` list; false when [`Swatches::can_add`]
    /// says no.
    pub fn add(&self, kind: SwatchKind, color: Rgba) -> bool {
        if !self.can_add(kind, color) {
            return false;
        }
        let mut lib = self.lock();
        lib.swatches.push(Swatch {
            name: None,
            color,
            kind,
        });
        lib.changed = true;
        true
    }

    /// Names swatch `index` (trimmed, at most [`MAX_SWATCH_NAME_CHARS`];
    /// blank removes the name). False when nothing changed.
    pub fn rename(&self, index: usize, name: &str) -> bool {
        let name = clean_name(name);
        let name = (!name.is_empty()).then_some(name);
        let mut lib = self.lock();
        let Some(swatch) = lib.swatches.get_mut(index) else {
            return false;
        };
        if swatch.name == name {
            return false;
        }
        swatch.name = name;
        lib.changed = true;
        true
    }

    /// Removes swatch `index`. False when there is none.
    pub fn delete(&self, index: usize) -> bool {
        let mut lib = self.lock();
        if index >= lib.swatches.len() {
            return false;
        }
        lib.swatches.remove(index);
        lib.changed = true;
        true
    }
}

/// A name as kept: trimmed, control characters dropped, at most
/// [`MAX_SWATCH_NAME_CHARS`] characters.
fn clean_name(name: &str) -> String {
    name.trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_SWATCH_NAME_CHARS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::test_dir::TestDir;

    const RED: Rgba = Rgba::rgb(255, 0, 0);

    #[test]
    fn swatches_are_added_once_per_list_renamed_and_deleted() {
        let lib = Swatches::default();
        assert!(lib.add(SwatchKind::Fill, RED));
        assert!(!lib.add(SwatchKind::Fill, RED), "already in the fill list");
        assert!(
            lib.add(SwatchKind::Stroke, RED),
            "the stroke list is its own"
        );
        assert!(lib.rename(0, "  brand  "));
        assert_eq!(lib.list()[0].1.name.as_deref(), Some("brand"));
        assert!(lib.rename(0, " "));
        assert_eq!(lib.list()[0].1.name, None, "blank clears the name");
        assert!(lib.delete(0));
        assert_eq!(lib.list().len(), 1);
        assert!(!lib.delete(5));
        assert!(lib.take_changed());
        assert!(!lib.take_changed(), "once per change");
    }

    #[test]
    fn every_session_sees_the_same_library() {
        let lib = Swatches::default();
        let other = lib.clone();
        lib.add(SwatchKind::Fill, RED);
        assert_eq!(other.list().len(), 1);
    }

    #[test]
    fn the_library_is_saved_and_loaded_with_its_lists() {
        let dir = TestDir::new();
        assert_eq!(Swatches::load(dir.path()).list(), vec![], "no file yet");
        let lib = Swatches::default();
        lib.add(SwatchKind::Fill, RED);
        lib.add(SwatchKind::Stroke, Rgba::BLACK);
        lib.rename(1, "ink");
        lib.save(dir.path()).expect("saves");
        let loaded = Swatches::load(dir.path());
        assert_eq!(
            loaded
                .list()
                .into_iter()
                .map(|(_, s)| s)
                .collect::<Vec<_>>(),
            lib.list().into_iter().map(|(_, s)| s).collect::<Vec<_>>()
        );
        assert!(!loaded.take_changed(), "loading is not a change");
    }

    #[test]
    fn a_broken_huge_or_overfull_file_loads_safely() {
        let dir = TestDir::new();
        fs::write(dir.path().join(FILE), b"{broken").expect("writes");
        assert_eq!(Swatches::load(dir.path()).list(), vec![]);

        let big = vec![b' '; usize::try_from(MAX_FILE_BYTES).expect("fits") + 1];
        fs::write(dir.path().join(FILE), big).expect("writes");
        assert_eq!(Swatches::load(dir.path()).list(), vec![]);

        let long = "n\u{7}".repeat(100);
        let many: Vec<Swatch> = (0..300)
            .map(|_| Swatch {
                name: Some(long.clone()),
                color: RED,
                kind: SwatchKind::Fill,
            })
            .collect();
        let json = serde_json::to_vec(&Saved { swatches: many }).expect("json");
        fs::write(dir.path().join(FILE), json).expect("writes");
        let list = Swatches::load(dir.path()).list();
        assert_eq!(list.len(), MAX_SWATCHES);
        let name = list[0].1.name.clone().expect("named");
        assert_eq!(name.chars().count(), MAX_SWATCH_NAME_CHARS);
        assert!(!name.chars().any(char::is_control));
    }
}
