//! Saving off the UI thread (spec 14).
//!
//! Save and autosave hand a snapshot of the document to one worker thread
//! and return at once; a frame is never blocked by encoding, compressing,
//! writing or checking a big file. Jobs run in the order given, so an
//! autosave queued before a save is on disk before the save clears it.
//! The app collects the results each frame ([`Saver::poll`]), and waits
//! for them ([`Saver::wait`]) only where it must: before closing or
//! quitting.

use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use vextor_core::Document;

use crate::backups::Backups;
use crate::documents::DocId;
use crate::files;
use crate::recovery::AutosaveJob;

/// Work for the saving thread.
pub enum Job {
    /// Save `document` (at `revision`) to `path`, backing up the file
    /// there first.
    Save {
        doc: DocId,
        path: PathBuf,
        document: Box<Document>,
        revision: u64,
    },
    Autosave {
        doc: DocId,
        job: Box<AutosaveJob>,
    },
}

/// What a job came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Done {
    Saved {
        doc: DocId,
        path: PathBuf,
        revision: u64,
    },
    SaveFailed {
        doc: DocId,
        error: String,
    },
    Autosaved {
        doc: DocId,
        revision: u64,
    },
    AutosaveFailed {
        doc: DocId,
        error: String,
    },
}

impl Done {
    fn doc(&self) -> DocId {
        match self {
            Self::Saved { doc, .. }
            | Self::SaveFailed { doc, .. }
            | Self::Autosaved { doc, .. }
            | Self::AutosaveFailed { doc, .. } => *doc,
        }
    }

    fn is_save(&self) -> bool {
        matches!(self, Self::Saved { .. } | Self::SaveFailed { .. })
    }
}

/// The saving thread and the jobs it has not finished.
pub struct Saver {
    jobs: Option<Sender<Job>>,
    done: Receiver<Done>,
    worker: Option<JoinHandle<()>>,
    /// Jobs in flight per document: (saves, autosaves).
    in_flight: HashMap<DocId, (usize, usize)>,
    /// Results of jobs run on the UI thread when no thread could start.
    inline: VecDeque<Done>,
    backups: Option<Arc<Mutex<Backups>>>,
}

impl Saver {
    /// Starts the saving thread; `wake` is called after each job so the UI
    /// collects the result.
    pub fn new(backups: Option<Arc<Mutex<Backups>>>, wake: impl Fn() + Send + 'static) -> Self {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let (out, done) = mpsc::channel();
        let shared = backups.clone();
        let worker = thread::Builder::new()
            .name("vextor-save".to_owned())
            .spawn(move || {
                for job in inbox {
                    let _ = out.send(run(job, shared.as_deref()));
                    wake();
                }
            })
            .ok();
        Self {
            jobs: worker.as_ref().map(|_| jobs),
            done,
            worker,
            in_flight: HashMap::new(),
            inline: VecDeque::new(),
            backups,
        }
    }

    /// Queues `job`.
    pub fn send(&mut self, job: Job) {
        let entry = self.in_flight.entry(job_doc(&job)).or_default();
        match job {
            Job::Save { .. } => entry.0 += 1,
            Job::Autosave { .. } => entry.1 += 1,
        }
        let job = match &self.jobs {
            Some(jobs) => match jobs.send(job) {
                Ok(()) => return,
                Err(mpsc::SendError(job)) => job,
            },
            None => job,
        };
        let done = run(job, self.backups.as_deref());
        self.inline.push_back(done);
    }

    /// Whether a save of `doc` is queued or running.
    #[must_use]
    pub fn is_saving(&self, doc: DocId) -> bool {
        self.in_flight.get(&doc).is_some_and(|c| c.0 > 0)
    }

    /// Whether any job of `doc` is queued or running.
    #[must_use]
    pub fn is_busy(&self, doc: DocId) -> bool {
        self.in_flight.get(&doc).is_some_and(|c| c.0 + c.1 > 0)
    }

    /// The results ready now, in the order the jobs ran.
    pub fn poll(&mut self) -> Vec<Done> {
        let mut out: Vec<Done> = self.inline.drain(..).collect();
        out.extend(self.done.try_iter());
        for d in &out {
            self.finished(d);
        }
        out
    }

    /// Every result still to come; blocks until the queue is empty.
    pub fn wait(&mut self) -> Vec<Done> {
        let mut out = self.poll();
        while self.in_flight.values().any(|c| c.0 + c.1 > 0) {
            match self.done.recv() {
                Ok(d) => {
                    self.finished(&d);
                    out.push(d);
                }
                Err(_) => break,
            }
        }
        out
    }

    fn finished(&mut self, d: &Done) {
        if let Some(c) = self.in_flight.get_mut(&d.doc()) {
            if d.is_save() {
                c.0 = c.0.saturating_sub(1);
            } else {
                c.1 = c.1.saturating_sub(1);
            }
            if *c == (0, 0) {
                self.in_flight.remove(&d.doc());
            }
        }
    }
}

impl Drop for Saver {
    /// Lets the queued jobs finish: nothing being saved is cut off.
    fn drop(&mut self) {
        self.jobs = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn job_doc(job: &Job) -> DocId {
    match job {
        Job::Save { doc, .. } | Job::Autosave { doc, .. } => *doc,
    }
}

/// Runs one job; a panic inside it becomes a failure, not a dead thread.
fn run(job: Job, backups: Option<&Mutex<Backups>>) -> Done {
    let doc = job_doc(&job);
    let is_save = matches!(job, Job::Save { .. });
    catch_unwind(AssertUnwindSafe(|| run_job(job, backups))).unwrap_or_else(|_| {
        let error = "an internal error stopped it".to_owned();
        if is_save {
            Done::SaveFailed { doc, error }
        } else {
            Done::AutosaveFailed { doc, error }
        }
    })
}

fn run_job(job: Job, backups: Option<&Mutex<Backups>>) -> Done {
    match job {
        Job::Save {
            doc,
            path,
            document,
            revision,
        } => {
            let name = files::display_name(&path);
            if let Some(b) = backups {
                let mut b = b.lock().unwrap_or_else(PoisonError::into_inner);
                if let Err(e) = b.before_overwrite(&path) {
                    return Done::SaveFailed {
                        doc,
                        error: format!(
                            "Couldn't back up “{name}” first, so it was not overwritten: {e}"
                        ),
                    };
                }
            }
            match files::write_document(&path, &document) {
                Ok(()) => {
                    if let Some(b) = backups {
                        b.lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .written(&path);
                    }
                    Done::Saved {
                        doc,
                        path,
                        revision,
                    }
                }
                Err(e) => Done::SaveFailed {
                    doc,
                    error: format!("Couldn't save “{name}”: {e}"),
                },
            }
        }
        Job::Autosave { doc, job } => match job.run() {
            Ok(()) => Done::Autosaved {
                doc,
                revision: job.revision,
            },
            Err(e) => Done::AutosaveFailed {
                doc,
                error: format!("Autosave failed: {e}"),
            },
        },
    }
}

#[cfg(test)]
mod tests;
