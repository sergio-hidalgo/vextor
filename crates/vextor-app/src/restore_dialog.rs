//! File ▸ Restore from Backup… (spec 14): the backups of the current file,
//! newest first, and the backup settings. Opening a backup makes a new
//! unsaved document; nothing is ever overwritten from here.

use std::path::PathBuf;
use std::time::SystemTime;

use eframe::egui::{self, Rect};

use crate::backups::{self, Backup, Settings};
use crate::prompts::{buttons_enabled, centered, dialog_of_width};
use crate::theme;

/// Width of the dialog's content.
const WIDTH: f32 = 340.0;

/// What the user did in the dialog this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Open this backup as a new document.
    Open(PathBuf),
    /// The settings changed.
    Settings(Settings),
    Close,
}

/// The open dialog.
pub struct RestoreDialog {
    /// The current file's name, `None` when the document was never saved.
    file: Option<String>,
    list: Vec<Backup>,
    settings: Settings,
    /// Space all backups take, in bytes.
    usage: u64,
    picked: Option<usize>,
}

#[derive(Clone, Copy)]
enum Button {
    Open,
    Close,
}

impl RestoreDialog {
    #[must_use]
    pub fn new(file: Option<String>, list: Vec<Backup>, settings: Settings, usage: u64) -> Self {
        let picked = (!list.is_empty()).then_some(0);
        Self {
            file,
            list,
            settings,
            usage,
            picked,
        }
    }

    /// The settings saved and the space used now (after pruning).
    pub fn refresh(&mut self, settings: Settings, usage: u64) {
        self.settings = settings;
        self.usage = usage;
    }

    /// Shows the dialog centred on `canvas`.
    pub fn show(&mut self, ctx: &egui::Context, canvas: Rect) -> Option<Outcome> {
        let now = SystemTime::now();
        let modal = dialog_of_width(ctx, "restore_backup", canvas, WIDTH, |ui| {
            ui.add_space(4.0);
            ui.heading("Restore from Backup");
            ui.add_space(4.0);
            let mut outcome = None;
            match &self.file {
                None => {
                    ui.label("This document has no file yet, so it has no backups.");
                }
                Some(_) if self.list.is_empty() => {
                    ui.label(
                        "No backups of this file yet. One is made the first time \
                         Vextor overwrites it in a session.",
                    );
                }
                Some(name) => {
                    ui.label(format!("Backups of “{name}”, newest first:"));
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            for (i, b) in self.list.iter().enumerate() {
                                let text = format!(
                                    "{}   {}",
                                    backups::age(b.made, now),
                                    backups::size(b.bytes)
                                );
                                let row = ui.selectable_label(self.picked == Some(i), text);
                                if row.clicked() {
                                    self.picked = Some(i);
                                }
                                if row.double_clicked() {
                                    outcome = Some(Outcome::Open(b.path.clone()));
                                }
                            }
                        });
                    ui.add_space(2.0);
                    ui.colored_label(theme::GREY_400, "Opens as a new, unsaved document.");
                }
            }
            ui.add_space(10.0);
            ui.separator();
            if let Some(s) = self.settings_ui(ui) {
                outcome = Some(Outcome::Settings(s));
            }
            ui.add_space(10.0);
            let can_open = self.picked.is_some();
            match buttons_enabled(
                ui,
                &[
                    ("Open", Button::Open, can_open),
                    ("Close", Button::Close, true),
                ],
            ) {
                Some(Button::Open) => {
                    if let Some(b) = self.picked.and_then(|i| self.list.get(i)) {
                        outcome = Some(Outcome::Open(b.path.clone()));
                    }
                }
                Some(Button::Close) => outcome = Some(Outcome::Close),
                None => {}
            }
            outcome
        });
        if modal.inner.is_none() && modal.should_close() {
            return Some(Outcome::Close);
        }
        modal.inner
    }

    /// Backups on/off, how many per file, how much space in all. Returns
    /// the new settings when one changed.
    fn settings_ui(&self, ui: &mut egui::Ui) -> Option<Settings> {
        let mut s = self.settings;
        ui.add_space(4.0);
        ui.checkbox(&mut s.enabled, "Back up files before overwriting them");
        // Centred like the rest of the dialog: a row's width is only known
        // once drawn, so `centered` keeps last frame's.
        centered(ui, "backup_limits", |ui| {
            ui.add_enabled_ui(s.enabled, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Keep");
                    ui.add(egui::DragValue::new(&mut s.per_file).range(Settings::PER_FILE));
                    ui.label("per file, at most");
                    ui.add(
                        egui::DragValue::new(&mut s.total_mb)
                            .range(Settings::TOTAL_MB)
                            .suffix(" MB"),
                    );
                });
            });
        });
        ui.colored_label(
            theme::GREY_400,
            format!("Backups use {} now.", backups::size(self.usage)),
        );
        (s != self.settings).then_some(s)
    }
}
