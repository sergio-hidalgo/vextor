//! Files Vextor is asked to open from outside (spec 14b): named on the
//! command line at launch (how Windows and Linux file associations start
//! apps), or sent by macOS (double-click in Finder, the Dock icon, "Open
//! With") at any time.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use crate::import::FileKind;

/// Most files opened from one launch.
pub const MAX_LAUNCH_FILES: usize = 20;

/// The arguments (program name already skipped) that are files Vextor
/// opens: `.vxtr`, SVG, PNG, JPEG or WebP, in any letter case, in order and
/// at most [`MAX_LAUNCH_FILES`]. Flags and anything else (old macOS adds
/// `-psn_…`) are skipped.
#[must_use]
pub fn launch_paths(args: impl IntoIterator<Item = OsString>) -> Vec<PathBuf> {
    args.into_iter()
        .filter(|a| !a.to_string_lossy().starts_with('-'))
        .map(PathBuf::from)
        .filter(|p| FileKind::of(p) != FileKind::Other)
        // Relative to the folder Vextor was started from, so an open tab and
        // Open Recent see the same path File ▸ Open gives.
        .map(|p| std::path::absolute(&p).unwrap_or(p))
        .take(MAX_LAUNCH_FILES)
        .collect()
}

/// Files waiting to be opened, in the order they were asked for.
#[derive(Default)]
pub struct OpenQueue {
    waiting: VecDeque<PathBuf>,
    /// Paths sent from outside the frame loop (the macOS hook).
    incoming: Option<Receiver<PathBuf>>,
}

impl OpenQueue {
    /// A queue starting with `paths`, then whatever `incoming` brings.
    #[must_use]
    pub fn new(paths: Vec<PathBuf>, incoming: Option<Receiver<PathBuf>>) -> Self {
        Self {
            waiting: paths.into(),
            incoming,
        }
    }

    /// The files to open now, oldest first; none while a dialog is open
    /// (`blocked`), so they wait until it closes.
    pub fn take(&mut self, blocked: bool) -> Vec<PathBuf> {
        if let Some(rx) = &self.incoming {
            loop {
                match rx.try_recv() {
                    Ok(path) => self.waiting.push_back(path),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.incoming = None;
                        break;
                    }
                }
            }
        }
        if blocked {
            return Vec::new();
        }
        self.waiting.drain(..).collect()
    }
}

/// Least time the launch card stays up (spec 14e).
pub const CARD_MIN: Duration = Duration::from_secs(2);

/// What the window shows while Vextor starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The launch card alone; files from the launch wait.
    Card,
    /// The editor.
    Editor,
}

/// The card stays until [`CARD_MIN`] has passed since launch (`elapsed`)
/// and the editor is `ready`; then the editor shows.
#[must_use]
pub fn phase(elapsed: Duration, ready: bool) -> Phase {
    if ready && elapsed >= CARD_MIN {
        Phase::Editor
    } else {
        Phase::Card
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    fn names(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|p| p.file_name().expect("named").to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_card_shows_before_two_seconds_even_when_ready() {
        assert_eq!(phase(Duration::ZERO, true), Phase::Card);
        assert_eq!(phase(Duration::from_millis(1999), true), Phase::Card);
    }

    #[test]
    fn the_editor_shows_from_two_seconds_only_once_ready() {
        assert_eq!(phase(CARD_MIN, false), Phase::Card);
        assert_eq!(phase(Duration::from_secs(5), false), Phase::Card);
        assert_eq!(phase(CARD_MIN, true), Phase::Editor);
        assert_eq!(phase(Duration::from_secs(5), true), Phase::Editor);
    }

    #[test]
    fn queued_files_wait_while_the_card_shows() {
        let mut q = OpenQueue::new(vec!["a.vxtr".into()], None);
        let blocked = phase(Duration::from_millis(1500), true) == Phase::Card;
        assert_eq!(names(&q.take(blocked)), [] as [&str; 0]);
        let blocked = phase(CARD_MIN, true) == Phase::Card;
        assert_eq!(names(&q.take(blocked)), ["a.vxtr"]);
    }

    #[test]
    fn documents_svgs_and_images_are_kept_in_order() {
        let got = launch_paths(args(&[
            "b.vxtr", "a.svg", "c.png", "d.jpg", "e.jpeg", "f.webp",
        ]));
        assert_eq!(
            names(&got),
            ["b.vxtr", "a.svg", "c.png", "d.jpg", "e.jpeg", "f.webp"]
        );
    }

    #[test]
    fn other_extensions_flags_and_process_serial_numbers_are_skipped() {
        let got = launch_paths(args(&[
            "-psn_0_12345",
            "--verbose",
            "notes.txt",
            "a.vxtr",
            "Makefile",
            "-x.vxtr",
        ]));
        assert_eq!(names(&got), ["a.vxtr"]);
    }

    #[test]
    fn letter_case_is_ignored() {
        let got = launch_paths(args(&["A.VXTR", "B.Svg", "C.PNG"]));
        assert_eq!(names(&got), ["A.VXTR", "B.Svg", "C.PNG"]);
    }

    #[test]
    fn at_most_twenty_files_are_taken() {
        let many: Vec<String> = (0..30).map(|i| format!("{i}.vxtr")).collect();
        let got = launch_paths(many.iter().map(OsString::from));
        assert_eq!(got.len(), MAX_LAUNCH_FILES);
        assert_eq!(names(&got)[0], "0.vxtr");
        assert_eq!(names(&got)[19], "19.vxtr");
    }

    #[test]
    fn relative_paths_become_absolute() {
        let got = launch_paths(args(&["some.vxtr"]));
        assert!(got[0].is_absolute(), "{:?}", got[0]);
    }

    #[test]
    fn paths_queued_before_the_first_frame_open_on_it() {
        let (tx, rx) = mpsc::channel();
        let mut q = OpenQueue::new(vec!["a.vxtr".into()], Some(rx));
        // A macOS cold start: the event arrives before the window is up.
        tx.send("b.vxtr".into()).expect("sends");
        assert_eq!(names(&q.take(false)), ["a.vxtr", "b.vxtr"]);
        assert_eq!(q.take(false), Vec::<PathBuf>::new());
    }

    #[test]
    fn paths_wait_while_a_dialog_is_open() {
        let (tx, rx) = mpsc::channel();
        let mut q = OpenQueue::new(Vec::new(), Some(rx));
        tx.send("a.vxtr".into()).expect("sends");
        assert_eq!(q.take(true), Vec::<PathBuf>::new());
        tx.send("b.vxtr".into()).expect("sends");
        assert_eq!(q.take(true), Vec::<PathBuf>::new());
        assert_eq!(names(&q.take(false)), ["a.vxtr", "b.vxtr"]);
    }

    #[test]
    fn a_closed_channel_keeps_the_waiting_paths() {
        let (tx, rx) = mpsc::channel();
        let mut q = OpenQueue::new(Vec::new(), Some(rx));
        tx.send("a.vxtr".into()).expect("sends");
        drop(tx);
        assert_eq!(names(&q.take(false)), ["a.vxtr"]);
        assert_eq!(q.take(false), Vec::<PathBuf>::new());
    }
}
