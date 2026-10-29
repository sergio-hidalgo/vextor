//! The system fonts, loaded once on a background thread so the first
//! frame never waits for them (spec 05c).

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use eframe::egui;
use vextor_geom::{FontDb, Fonts};

/// The fonts text is laid out with: empty until the OS fonts are read.
pub struct SystemFonts {
    current: Arc<Fonts>,
    pending: Option<Receiver<FontDb>>,
}

impl SystemFonts {
    /// Starts reading the OS font folders; the window repaints when done.
    pub fn load(ctx: &egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let spawned = thread::Builder::new()
            .name("vextor-fonts".to_owned())
            .spawn(move || {
                // The receiver may be gone if the app closed first.
                let _ = tx.send(FontDb::system());
                ctx.request_repaint();
            });
        Self {
            current: Arc::default(),
            // Without a thread there are no system fonts: text is still
            // placed with estimated metrics.
            pending: spawned.ok().map(|_| rx),
        }
    }

    /// The fonts to use now.
    pub fn current(&self) -> Arc<Fonts> {
        Arc::clone(&self.current)
    }

    /// Whether the system fonts are still being read.
    pub fn is_loading(&self) -> bool {
        self.pending.is_some()
    }

    /// The system fonts, the first time they are ready.
    pub fn poll(&mut self) -> Option<Arc<Fonts>> {
        let rx = self.pending.as_ref()?;
        match rx.try_recv() {
            Ok(db) => {
                self.pending = None;
                self.current = Arc::new(Fonts::new(Arc::new(db)));
                Some(self.current())
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                None
            }
        }
    }
}
