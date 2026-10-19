//! The new-document flow: pick an artboard preset or a custom size.

use std::cell::OnceCell;

use eframe::egui;
use vextor_core::{Artboard, Command, Document, Rect};

use crate::{theme, wordmark};

/// Largest custom artboard side, in document px.
pub const MAX_SIDE: f64 = 10_000.0;

/// Size of a preset button, in points. All the same, so rows can be centred.
const PRESET_BUTTON: egui::Vec2 = egui::vec2(96.0, 48.0);
/// Least space kept left and right of the preset rows, in points.
const SIDE_MARGIN: f32 = 32.0;
/// Space between the wordmark and the "New document" title, in points.
const WORDMARK_GAP: f32 = 12.0;
/// Least space kept above the picker, in points.
const TOP_MARGIN: f32 = 16.0;

/// The artboard sizes offered when creating a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Icon16,
    Icon24,
    Icon32,
    Icon48,
    Button,
    Card,
}

impl Preset {
    /// Every preset, in the order shown.
    pub const ALL: [Self; 6] = [
        Self::Icon16,
        Self::Icon24,
        Self::Icon32,
        Self::Icon48,
        Self::Button,
        Self::Card,
    ];

    /// Width and height in document px.
    #[must_use]
    pub fn size(self) -> (f64, f64) {
        match self {
            Self::Icon16 => (16.0, 16.0),
            Self::Icon24 => (24.0, 24.0),
            Self::Icon32 => (32.0, 32.0),
            Self::Icon48 => (48.0, 48.0),
            Self::Button => (160.0, 48.0),
            Self::Card => (320.0, 200.0),
        }
    }

    /// Text on the picker button.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Icon16 => "Icon 16",
            Self::Icon24 => "Icon 24",
            Self::Icon32 => "Icon 32",
            Self::Icon48 => "Icon 48",
            Self::Button => "Button",
            Self::Card => "Card",
        }
    }

    /// Name given to the artboard (becomes the exported file's name).
    #[must_use]
    pub fn artboard_name(self) -> &'static str {
        match self {
            Self::Icon16 => "icon-16",
            Self::Icon24 => "icon-24",
            Self::Icon32 => "icon-32",
            Self::Icon48 => "icon-48",
            Self::Button => "button",
            Self::Card => "card",
        }
    }
}

/// Parses the custom width and height fields.
///
/// # Errors
/// A short message for the user when a value is not a number, not > 0, or
/// larger than [`MAX_SIDE`].
pub fn parse_custom(width: &str, height: &str) -> Result<(f64, f64), String> {
    let side = |text: &str, what: &str| -> Result<f64, String> {
        let v: f64 = text
            .trim()
            .parse()
            .map_err(|_| format!("{what} must be a number"))?;
        if !v.is_finite() || v <= 0.0 {
            return Err(format!("{what} must be greater than 0"));
        }
        if v > MAX_SIDE {
            return Err(format!("{what} can be at most {MAX_SIDE} px"));
        }
        Ok(v)
    };
    Ok((side(width, "Width")?, side(height, "Height")?))
}

/// A new untitled document with one transparent artboard at the origin.
/// The artboard is part of the starting state, not an undoable step.
#[must_use]
pub fn new_document(artboard_name: &str, width: f64, height: f64) -> Document {
    let mut doc = Document::new("Untitled");
    let artboard = Artboard {
        id: doc.alloc_artboard_id(),
        name: artboard_name.to_owned(),
        rect: Rect::new(0.0, 0.0, width, height),
        background: None,
    };
    if let Err(e) = doc.apply(Command::SetArtboards(vec![artboard])) {
        // Sizes come from presets or `parse_custom`, so this cannot happen;
        // the document is still usable without its artboard.
        debug_assert!(false, "new artboard rejected: {e}");
    }
    doc
}

/// What the user did in the picker this frame.
pub enum Outcome {
    /// Nothing yet.
    Open,
    /// Closed without creating (only offered when a document is open).
    Cancel,
    /// A new document to switch to.
    Create(Box<Document>),
}

/// State of the picker (the custom size fields).
pub struct Picker {
    width: String,
    height: String,
    error: Option<String>,
    /// Width of the custom-size row on the last frame, to centre it.
    custom_row_width: f32,
    /// Height of the whole picker on the last frame, to centre it.
    content_height: f32,
    /// Loaded on the first frame; `None` after that if it failed to decode.
    wordmark: OnceCell<Option<egui::TextureHandle>>,
}

impl Default for Picker {
    fn default() -> Self {
        Self {
            width: "64".to_owned(),
            height: "64".to_owned(),
            error: None,
            custom_row_width: 0.0,
            content_height: 0.0,
            wordmark: OnceCell::new(),
        }
    }
}

impl Picker {
    /// Draws the picker centred in `ui`. `can_cancel` shows a Cancel
    /// button and lets `Esc` close it.
    pub fn ui(&mut self, ui: &mut egui::Ui, can_cancel: bool) -> Outcome {
        let mut outcome = Outcome::Open;
        if can_cancel && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            return Outcome::Cancel;
        }
        let wordmark = self
            .wordmark
            .get_or_init(|| wordmark::texture(ui.ctx()))
            .clone();
        ui.vertical_centered(|ui| {
            // Its height depends on the window width (rows of presets), so
            // it is measured each frame and the padding uses the last one.
            let top = ((ui.available_height() - self.content_height) / 2.0).max(TOP_MARGIN);
            ui.add_space(top);
            let start = ui.cursor().min.y;
            if let Some(texture) = &wordmark {
                ui.add(egui::Image::new((texture.id(), wordmark::size(texture))));
                ui.add_space(WORDMARK_GAP);
            }
            ui.label(
                egui::RichText::new("New document")
                    .heading()
                    .color(theme::YELLOW_SOFT),
            );
            ui.label("Pick the size of what you are drawing.");
            ui.add_space(12.0);
            // `vertical_centered` does not centre rows, so each row is
            // padded on the left by half the room it leaves.
            let gap = ui.spacing().item_spacing.x;
            let room = ui.available_width() - 2.0 * SIDE_MARGIN;
            let per_row = per_row(Preset::ALL.len(), room, PRESET_BUTTON.x, gap);
            for row in Preset::ALL.chunks(per_row) {
                ui.horizontal(|ui| {
                    ui.add_space(left_pad(
                        ui.available_width(),
                        row_width(row.len(), PRESET_BUTTON.x, gap),
                    ));
                    for &preset in row {
                        let (w, h) = preset.size();
                        let text = format!("{}\n{w} × {h}", preset.label());
                        if ui
                            .add_sized(PRESET_BUTTON, egui::Button::new(text))
                            .clicked()
                        {
                            outcome = Outcome::Create(Box::new(new_document(
                                preset.artboard_name(),
                                w,
                                h,
                            )));
                        }
                    }
                });
            }
            // Wide: Create sits at the end of the Custom row. Narrow (presets
            // on several rows): Create gets its own row, above Cancel.
            let wide = per_row == Preset::ALL.len();
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                // Its width depends on the font, so it is measured each frame
                // and the padding uses the last measurement.
                ui.add_space(left_pad(ui.available_width(), self.custom_row_width));
                let start = ui.cursor().min.x;
                ui.label("Custom");
                ui.add(egui::TextEdit::singleline(&mut self.width).desired_width(56.0));
                ui.label("×");
                ui.add(egui::TextEdit::singleline(&mut self.height).desired_width(56.0));
                ui.label("px");
                if wide && ui.button("Create").clicked() {
                    outcome = self.create_custom();
                }
                let width = ui.min_rect().max.x - start;
                if (width - self.custom_row_width).abs() > 0.5 {
                    self.custom_row_width = width;
                    ui.ctx().request_repaint();
                }
            });
            if let Some(e) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.add_space(12.0);
            if !wide && ui.add(action_button("Create")).clicked() {
                outcome = self.create_custom();
            }
            if can_cancel && ui.add(action_button("Cancel")).clicked() {
                outcome = Outcome::Cancel;
            }
            let height = ui.min_rect().max.y - start;
            if (height - self.content_height).abs() > 0.5 {
                self.content_height = height;
                ui.ctx().request_repaint();
            }
        });
        if matches!(outcome, Outcome::Create(_)) {
            self.error = None;
        }
        outcome
    }

    /// The document for the custom size fields, or the reason it can't be made.
    fn create_custom(&mut self) -> Outcome {
        match parse_custom(&self.width, &self.height) {
            Ok((w, h)) => Outcome::Create(Box::new(new_document("artboard", w, h))),
            Err(e) => {
                self.error = Some(e);
                Outcome::Open
            }
        }
    }
}

/// Create / Cancel under the picker: one width so stacked buttons line up.
fn action_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(text).min_size(egui::vec2(PRESET_BUTTON.x, 0.0))
}

/// How many of `count` items of width `item` go on each row of `available`
/// points. Rows are kept even (6 → 6, 3 + 3, 2 + 2 + 2), never 5 + 1.
fn per_row(count: usize, available: f32, item: f32, gap: f32) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small, ≥ 0
    let fit = ((available + gap) / (item + gap)).floor().max(1.0) as usize;
    let fit = fit.min(count).max(1);
    let rows = count.div_ceil(fit);
    count.div_ceil(rows).max(1)
}

/// Width of `n` items of width `item` separated by `gap`.
#[allow(clippy::cast_precision_loss)] // small counts
fn row_width(n: usize, item: f32, gap: f32) -> f32 {
    n as f32 * item + n.saturating_sub(1) as f32 * gap
}

/// Space before a row of `width` so it sits in the middle of `available`.
fn left_pad(available: f32, width: f32) -> f32 {
    ((available - width) / 2.0).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_share_one_row_when_there_is_room() {
        assert_eq!(per_row(6, 1000.0, 96.0, 8.0), 6);
    }

    #[test]
    fn presets_wrap_into_rows_on_a_narrow_canvas() {
        // (400 + 8) / (96 + 8) = 3.9 → 3 per row.
        assert_eq!(per_row(6, 400.0, 96.0, 8.0), 3);
        assert_eq!(per_row(6, 10.0, 96.0, 8.0), 1, "never zero");
    }

    #[test]
    fn preset_rows_stay_even() {
        // Room for 5 or 4 → 3 + 3, not 5 + 1 or 4 + 2.
        assert_eq!(per_row(6, 5.0 * 104.0, 96.0, 8.0), 3);
        assert_eq!(per_row(6, 4.0 * 104.0, 96.0, 8.0), 3);
        // Room for 2 → 2 + 2 + 2.
        assert_eq!(per_row(6, 2.0 * 104.0, 96.0, 8.0), 2);
    }

    #[test]
    fn row_is_centred() {
        let width = row_width(6, 96.0, 8.0);
        assert!((width - 616.0).abs() < f32::EPSILON);
        assert!((left_pad(1000.0, width) - 192.0).abs() < f32::EPSILON);
        assert!(
            left_pad(300.0, width).abs() < f32::EPSILON,
            "no negative pad"
        );
    }

    #[test]
    fn presets_have_the_spec_sizes() {
        let sizes: Vec<_> = Preset::ALL.iter().map(|p| p.size()).collect();
        assert_eq!(
            sizes,
            [
                (16.0, 16.0),
                (24.0, 24.0),
                (32.0, 32.0),
                (48.0, 48.0),
                (160.0, 48.0),
                (320.0, 200.0)
            ]
        );
    }

    #[test]
    fn preset_document_has_one_transparent_artboard_at_the_origin() {
        let (w, h) = Preset::Icon24.size();
        let doc = new_document(Preset::Icon24.artboard_name(), w, h);
        assert!(doc.validate().is_ok());
        assert_eq!(doc.artboards.len(), 1);
        let a = &doc.artboards[0];
        assert_eq!(a.name, "icon-24");
        assert_eq!(a.rect, Rect::new(0.0, 0.0, 24.0, 24.0));
        assert_eq!(a.background, None);
    }

    #[test]
    fn custom_size_accepts_positive_numbers() {
        assert_eq!(parse_custom(" 120 ", "40.5"), Ok((120.0, 40.5)));
    }

    #[test]
    fn custom_size_rejects_bad_values() {
        for (w, h) in [
            ("abc", "10"),
            ("10", ""),
            ("0", "10"),
            ("-5", "10"),
            ("NaN", "10"),
            ("inf", "10"),
            ("10", "10001"),
        ] {
            assert!(parse_custom(w, h).is_err(), "{w} × {h} should be rejected");
        }
    }

    #[test]
    fn custom_size_error_names_the_field() {
        assert_eq!(
            parse_custom("10", "0"),
            Err("Height must be greater than 0".to_owned())
        );
    }
}
