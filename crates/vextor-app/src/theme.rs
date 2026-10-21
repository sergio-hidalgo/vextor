//! Vextor's colours. Surfaces stay egui's black and dark greys; text,
//! icons and accents use the palette below, so the app reads as one with
//! its icon and wordmark.

use eframe::egui::{self, Color32, Stroke};

// Yellows, strongest to palest.

/// Accent: focus, text cursor, warnings.
pub const YELLOW: Color32 = Color32::from_rgb(0xF3, 0xCD, 0x34);
/// Muted yellow for titles.
pub const YELLOW_SOFT: Color32 = Color32::from_rgb(0xF8, 0xE0, 0x7F);
/// Links.
pub const YELLOW_LIGHT: Color32 = Color32::from_rgb(0xFA, 0xE7, 0x9C);
/// Faint yellow tint.
#[allow(dead_code)] // part of the palette; no UI uses it yet
pub const YELLOW_PALE: Color32 = Color32::from_rgb(0xFD, 0xF5, 0xD9);

// The app icon's yellows (`assets/icon.svg`).

/// The logo's yellow; also the snap mark (spec 08).
pub const LOGO_YELLOW: Color32 = Color32::from_rgb(0xFF, 0xD5, 0x00);
/// The icon's amber tile; also the text-selection background.
pub const LOGO_AMBER: Color32 = Color32::from_rgb(0xDC, 0xA5, 0x00);

// Neutrals for text and icons, lightest to darkest.

/// Text and icons on hovered or pressed widgets.
pub const WHITE: Color32 = Color32::WHITE;
/// Text and icons on widgets at rest.
pub const GREY_100: Color32 = Color32::from_gray(0xE7);
/// Secondary text.
#[allow(dead_code)] // part of the palette; no UI uses it yet
pub const GREY_200: Color32 = Color32::from_gray(0xD9);
/// Labels (non-interactive text).
pub const GREY_300: Color32 = Color32::from_gray(0xC2);
/// Muted text.
#[allow(dead_code)] // part of the palette; no UI uses it yet
pub const GREY_400: Color32 = Color32::from_gray(0xA4);
/// Faint text and lines; also the canvas grid (spec 08).
pub const GREY_500: Color32 = Color32::from_gray(0x82);

/// Uses the dark theme with the palette's foreground colours.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, visuals());
}

/// egui's dark visuals with the palette's foreground colours.
fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.widgets.noninteractive.fg_stroke.color = GREY_300;
    v.widgets.inactive.fg_stroke.color = GREY_100;
    v.widgets.hovered.fg_stroke.color = WHITE;
    v.widgets.active.fg_stroke.color = WHITE;
    v.widgets.open.fg_stroke.color = WHITE;
    // Selected text: yellow on a dimmed amber; the same stroke outlines
    // the focused text field.
    v.selection.bg_fill = LOGO_AMBER.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, YELLOW);
    v.text_cursor.stroke.color = YELLOW;
    v.hyperlink_color = YELLOW_LIGHT;
    v.warn_fg_color = YELLOW;
    v
}
