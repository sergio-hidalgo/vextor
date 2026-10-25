//! Emergency recovery (spec 14): if the app panics, the latest unsaved
//! work of every open document is written to its autosave slot before the
//! process ends, and the next launch offers it (spec 04's recovery flow).
//!
//! While a document has unsaved changes, the app keeps a snapshot of it
//! here, at most every [`EVERY`]. The panic hook writes them; it takes
//! the lock with `try_lock`, so a panic while the lock is held can never
//! deadlock. It acts only on the main thread: a panic on a worker thread
//! (import, save) is caught there and only fails that job.

use std::collections::BTreeMap;
use std::panic;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use crate::recovery::AutosaveJob;

/// Most time between snapshots of a document being edited.
pub const EVERY: Duration = Duration::from_secs(2);

/// The latest snapshot of each document with unsaved changes, by slot.
static LATEST: Mutex<BTreeMap<String, AutosaveJob>> = Mutex::new(BTreeMap::new());

/// Keeps `job` as slot `slot`'s latest unsaved work.
pub fn keep(slot: &str, job: AutosaveJob) {
    if let Ok(mut latest) = LATEST.lock() {
        latest.insert(slot.to_owned(), job);
    }
}

/// Slot `slot` has nothing unsaved any more.
pub fn forget(slot: &str) {
    if let Ok(mut latest) = LATEST.lock() {
        latest.remove(slot);
    }
}

/// Writes every kept snapshot to its slot; how many were written.
pub fn write_all() -> usize {
    let Ok(latest) = LATEST.try_lock() else {
        return 0;
    };
    latest.values().filter(|job| job.run().is_ok()).count()
}

/// Installs the panic hook (after the default one, which prints the
/// panic).
pub fn install() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        previous(info);
        if thread::current().name() != Some("main") {
            return;
        }
        let kept = write_all();
        let text = if kept > 0 {
            "Vextor ran into a problem and has to close. Your unsaved work was \
             kept and will be offered when you open Vextor again."
        } else {
            "Vextor ran into a problem and has to close."
        };
        eprintln!("{text}");
        let _ = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Vextor has to close")
            .set_description(text)
            .show();
    }));
}
