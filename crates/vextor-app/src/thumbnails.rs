//! Snapshots of the recent files for the start screen (spec 06 review).
//!
//! Each time the picker opens, the files it shows are read and rendered on
//! a background thread — with every check File ▸ Open makes — so the
//! picker never waits and never shows a stale picture. Text needs the
//! system fonts: when they finish loading after the first pictures, the
//! pictures are rendered again with them.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use vextor_core::{Document, Rgba, Vec2, View};
use vextor_geom::{FontDb, Fonts};
use vextor_render::{Viewport, render};

use crate::files;

/// Size of one snapshot on screen, in points.
pub const SIZE: egui::Vec2 = egui::vec2(200.0, 112.0);
/// The background a transparent artboard gets in a snapshot.
const TRANSPARENT_BACKGROUND: Rgba = Rgba::rgb(0xFF, 0xFF, 0xFF);

/// One file's snapshot.
pub enum Snapshot {
    /// Still being read or rendered.
    Loading,
    /// Rendered.
    Ready(TextureHandle),
    /// The file could not be read or drawn (moved, deleted, damaged).
    Failed,
}

/// Raw pixels from the worker: width, height, premultiplied RGBA.
type Pixels = (usize, usize, Vec<u8>);

/// The snapshots of some files, rendered off the UI thread.
pub struct Thumbnails {
    paths: Vec<PathBuf>,
    shots: Vec<Snapshot>,
    rx: Option<Receiver<(usize, Option<Pixels>)>>,
    /// The fonts the pictures were (or are being) rendered with.
    db: Option<Arc<FontDb>>,
}

impl Thumbnails {
    /// Starts rendering `paths` with the fonts in `db` at `pixels_per_point`.
    #[must_use]
    pub fn start(
        ctx: &egui::Context,
        paths: Vec<PathBuf>,
        db: Arc<FontDb>,
        pixels_per_point: f32,
    ) -> Self {
        let shots = paths.iter().map(|_| Snapshot::Loading).collect();
        let (tx, rx) = mpsc::channel();
        let jobs = paths.clone();
        let fonts_db = Arc::clone(&db);
        let ctx2 = ctx.clone();
        let spawned = thread::Builder::new()
            .name("vextor-thumbnails".to_owned())
            .spawn(move || {
                // Its own layout cache: node ids of other files must not
                // mix with the open documents'.
                let fonts = Fonts::new(fonts_db);
                for (i, path) in jobs.iter().enumerate() {
                    let pixels = render_file(path, &fonts, pixels_per_point);
                    if tx.send((i, pixels)).is_err() {
                        return; // the picker closed
                    }
                    ctx2.request_repaint();
                }
            });
        let mut t = Self {
            paths,
            shots,
            rx: spawned.ok().map(|_| rx),
            db: Some(db),
        };
        if t.rx.is_none() {
            t.shots.iter_mut().for_each(|s| *s = Snapshot::Failed);
        }
        t
    }

    /// True when these pictures show `paths` drawn with `db`.
    #[must_use]
    pub fn shows(&self, paths: &[PathBuf], db: &Arc<FontDb>) -> bool {
        self.paths == paths && self.db.as_ref().is_some_and(|d| Arc::ptr_eq(d, db))
    }

    /// Takes the pictures finished since the last frame. Returns the
    /// snapshot of each file, in order.
    pub fn poll(&mut self, ctx: &egui::Context) -> &[Snapshot] {
        if let Some(rx) = &self.rx {
            loop {
                let (i, pixels) = match rx.try_recv() {
                    Ok(msg) => msg,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        // Done, or the worker died: nothing more will come.
                        for shot in &mut self.shots {
                            if matches!(shot, Snapshot::Loading) {
                                *shot = Snapshot::Failed;
                            }
                        }
                        self.rx = None;
                        break;
                    }
                };
                let shot = match pixels {
                    Some((w, h, data)) => {
                        let image = ColorImage::from_rgba_premultiplied([w, h], &data);
                        Snapshot::Ready(ctx.load_texture(
                            format!("recent-{i}"),
                            image,
                            TextureOptions::LINEAR,
                        ))
                    }
                    None => Snapshot::Failed,
                };
                if let Some(slot) = self.shots.get_mut(i) {
                    *slot = shot;
                }
            }
        }
        &self.shots
    }
}

/// Space between two snapshots, in points.
const GAP: f32 = 16.0;

impl Thumbnails {
    /// The recent files as a centred row of snapshots, each with its file
    /// name under it (full path on hover). Returns the one clicked, and
    /// the area the snapshots cover.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> (Option<PathBuf>, Option<egui::Rect>) {
        let paths = self.paths.clone();
        if paths.is_empty() {
            return (None, None);
        }
        let ctx = ui.ctx().clone();
        let shots = self.poll(&ctx);
        let per_row = per_row(paths.len(), ui.available_width());
        let mut clicked = None;
        let mut area: Option<egui::Rect> = None;
        let cards: Vec<_> = paths.iter().zip(shots).collect();
        for row in cards.chunks(per_row) {
            #[allow(clippy::cast_precision_loss)] // at most three
            let n = row.len() as f32;
            let width = n * SIZE.x + (n - 1.0) * GAP;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                // `add_space` adds no item spacing of its own.
                ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                for (path, shot) in row {
                    let r = card(ui, path, shot);
                    area = Some(area.map_or(r.rect, |a| a.union(r.rect)));
                    if r.clicked() {
                        clicked = Some((*path).clone());
                    }
                }
            });
            ui.add_space(8.0);
        }
        (clicked, area)
    }
}

/// How many of `count` snapshots fit side by side in `available` points
/// (at least one).
fn per_row(count: usize, available: f32) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small, ≥ 0
    let fit = ((available + GAP) / (SIZE.x + GAP)).floor().max(1.0) as usize;
    fit.min(count).max(1)
}

/// One snapshot and its file name, clickable as a whole. Hovering only
/// changes the pointer to a hand.
fn card(ui: &mut egui::Ui, path: &std::path::Path, shot: &Snapshot) -> egui::Response {
    let name = files::display_name(path);
    let inner = ui.vertical(|ui| {
        ui.set_width(SIZE.x);
        let (rect, response) = ui.allocate_exact_size(SIZE, egui::Sense::click());
        let painter = ui.painter();
        painter.rect_filled(rect, 2.0, ui.visuals().faint_bg_color);
        match shot {
            Snapshot::Ready(texture) => {
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                painter.image(texture.id(), rect, uv, egui::Color32::WHITE);
            }
            Snapshot::Loading => {}
            Snapshot::Failed => {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "No preview",
                    egui::FontId::proportional(13.0),
                    ui.visuals().weak_text_color(),
                );
            }
        }
        ui.add_space(4.0);
        let color = ui.visuals().hyperlink_color;
        let label = ui
            .vertical_centered(|ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(name).color(color))
                        .sense(egui::Sense::click())
                        .selectable(false),
                )
            })
            .inner;
        response | label
    });
    inner
        .inner
        .on_hover_text(path.display().to_string())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The snapshot of the document at `path`, read with every check a
/// normal open makes; `None` when it can't be read or drawn.
#[must_use]
pub fn render_file(path: &std::path::Path, fonts: &Fonts, pixels_per_point: f32) -> Option<Pixels> {
    let doc = files::read_document(path).ok()?;
    snapshot(&doc, fonts, pixels_per_point)
}

/// `doc`'s artboards fitted (whole, centred) into [`SIZE`] at
/// `pixels_per_point`; transparent artboards show white. `None` for a
/// document without artboards or a failed render.
#[must_use]
pub fn snapshot(doc: &Document, fonts: &Fonts, pixels_per_point: f32) -> Option<Pixels> {
    let mut doc = doc.clone();
    let bounds = doc
        .artboards
        .iter()
        .map(|a| {
            kurbo::Rect::new(
                a.rect.x,
                a.rect.y,
                a.rect.x + a.rect.width,
                a.rect.y + a.rect.height,
            )
        })
        .reduce(|a, b| a.union(b))?;
    for a in &mut doc.artboards {
        a.background.get_or_insert(TRANSPARENT_BACKGROUND);
    }
    let (w, h) = (f64::from(SIZE.x), f64::from(SIZE.y));
    if bounds.width() <= vextor_core::EPS || bounds.height() <= vextor_core::EPS {
        return None;
    }
    let zoom = (w / bounds.width()).min(h / bounds.height());
    let pan = Vec2::new(
        (w - bounds.width() * zoom) / 2.0 - bounds.x0 * zoom,
        (h - bounds.height() * zoom) / 2.0 - bounds.y0 * zoom,
    );
    let ppp = f64::from(pixels_per_point.max(1.0));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small, > 0
    let viewport = Viewport {
        view: View { zoom, pan },
        width: (w * ppp).round() as u32,
        height: (h * ppp).round() as u32,
        pixels_per_point: ppp,
    };
    let pixmap = render(&doc, fonts, &viewport).ok()?;
    Some((
        pixmap.width() as usize,
        pixmap.height() as usize,
        pixmap.data().to_vec(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_doc::new_document;

    /// RGBA of pixel `(x, y)`.
    fn pixel((w, _, data): &Pixels, x: usize, y: usize) -> [u8; 4] {
        let i = (y * w + x) * 4;
        [data[i], data[i + 1], data[i + 2], data[i + 3]]
    }

    #[test]
    fn a_wide_artboard_fills_the_width_and_is_centred() {
        // 400 × 100 into 200 × 112: zoom ½, 50 points tall, 31 above.
        let doc = new_document("wide", 400.0, 100.0);
        let shot = snapshot(&doc, &Fonts::default(), 1.0).expect("renders");
        assert_eq!((shot.0, shot.1), (200, 112));
        assert_eq!(
            pixel(&shot, 100, 56),
            [255, 255, 255, 255],
            "white artboard"
        );
        assert_eq!(pixel(&shot, 100, 10)[3], 0, "transparent above it");
        assert_eq!(pixel(&shot, 1, 56)[3], 255, "touches the left edge");
    }

    #[test]
    fn the_row_of_snapshots_is_centred() {
        let ctx = egui::Context::default();
        let paths: Vec<PathBuf> = (0..3)
            .map(|i| PathBuf::from(format!("/x/{i}.vxtr")))
            .collect();
        let mut t = Thumbnails {
            shots: paths.iter().map(|_| Snapshot::Failed).collect(),
            paths,
            rx: None,
            db: None,
        };
        let (mut area, mut panel) = (None, egui::Rect::NOTHING);
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 600.0),
                )),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::no_frame().show(ui, |ui| {
                    panel = ui.max_rect();
                    ui.vertical_centered(|ui| area = t.ui(ui).1);
                });
            })
            .textures_delta
            .clear();
        }
        let area = area.expect("drawn");
        assert!(
            (area.width() - (3.0 * SIZE.x + 2.0 * GAP)).abs() < 0.5,
            "{area:?}"
        );
        assert!(
            (area.center().x - panel.center().x).abs() < 0.5,
            "{area:?} in {panel:?}"
        );
    }

    #[test]
    fn snapshots_wrap_on_a_narrow_window() {
        assert_eq!(per_row(3, 1000.0), 3);
        assert_eq!(per_row(3, 2.0 * SIZE.x + GAP), 2);
        assert_eq!(per_row(3, 50.0), 1, "never zero");
        assert_eq!(per_row(2, 1000.0), 2);
    }

    #[test]
    fn retina_doubles_the_pixels() {
        let doc = new_document("icon", 24.0, 24.0);
        let shot = snapshot(&doc, &Fonts::default(), 2.0).expect("renders");
        assert_eq!((shot.0, shot.1), (400, 224));
    }

    #[test]
    fn a_document_without_artboards_has_no_snapshot() {
        assert!(snapshot(&Document::new("x"), &Fonts::default(), 1.0).is_none());
    }

    #[test]
    fn files_render_and_missing_or_damaged_ones_do_not() {
        let dir = std::env::temp_dir().join(format!("vextor-thumbs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let good = dir.join("good.vxtr");
        files::write_document(&good, &new_document("a", 32.0, 32.0)).expect("writes");
        let damaged = dir.join("damaged.vxtr");
        std::fs::write(&damaged, b"{ not a document").expect("writes");
        let fonts = Fonts::default();
        let good_shot = render_file(&good, &fonts, 1.0);
        let damaged_shot = render_file(&damaged, &fonts, 1.0);
        let missing_shot = render_file(&dir.join("missing.vxtr"), &fonts, 1.0);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(good_shot.is_some());
        assert!(damaged_shot.is_none());
        assert!(missing_shot.is_none());
    }
}
