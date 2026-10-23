//! Icon buttons: the standard one, and the variants the inspector's
//! field rows use (quiet, choice, and the swatches' icon-sized `+`). All
//! draw an [`Icon`] with [`paint`], coloured by state: grey at rest
//! (lighter or darker per kind), white on hover, yellow while selected.

use eframe::egui::{self, Color32, Rect, Response, Sense, Vec2};

use super::{Icon, LABEL_SIZE, SIZE, paint};
use crate::theme;

/// Size of an icon button, in points.
const BUTTON: Vec2 = Vec2::new(28.0, 24.0);

/// A button showing `icon`, highlighted when `selected`, explained by
/// `tooltip` on hover, or by `why` on hover while disabled.
pub fn button(
    ui: &mut egui::Ui,
    icon: Icon,
    selected: bool,
    enabled: bool,
    tooltip: &str,
    why: &str,
) -> Response {
    let look = Look {
        size: BUTTON,
        icon: SIZE,
        rest: theme::GREY_100,
    };
    draw_button(ui, icon, look, selected, enabled, tooltip, why)
}

/// How far in from its square an icon's ink starts on the 16 grid (the
/// plus's arm at 2.5), less where a label's first letter starts (about
/// 0.5): [`tight_button`] draws the icon this much to the left, so its ink
/// lines up with the labels above.
const INK_INSET: f32 = 2.0;

/// A [`button`] exactly its icon's size, its icon's ink starting at the
/// button's left edge (lined up with labels above it) and as tall as a
/// swatch tile beside it (the swatches' `+`). Drawn by hand: an egui
/// button is never shorter than a line of text.
pub fn tight_button(
    ui: &mut egui::Ui,
    icon: Icon,
    enabled: bool,
    tooltip: &str,
    why: &str,
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, r) = ui.allocate_exact_size(Vec2::splat(SIZE), sense);
    let r = r.on_hover_text(if enabled { tooltip } else { why });
    let color = if !enabled {
        theme::GREY_500
    } else if r.hovered() {
        ui.painter()
            .rect_filled(rect, 3.0, ui.visuals().widgets.hovered.weak_bg_fill);
        theme::WHITE
    } else {
        theme::GREY_100
    };
    let ink = rect.translate(Vec2::new(-INK_INSET * rect.width() / 16.0, 0.0));
    paint(ui.painter(), ink, icon, color);
    r
}

/// A button for a field row (Transform's flips): like [`button`], but
/// `size` (to share a field's width and height) and the icon drawn at
/// label size in the label grey, so it sits quietly among the field
/// labels around it; white on hover, yellow while `selected` (an armed
/// pipette).
pub fn quiet_button(
    ui: &mut egui::Ui,
    icon: Icon,
    size: Vec2,
    selected: bool,
    enabled: bool,
    tooltip: &str,
    why: &str,
) -> Response {
    let look = Look {
        size,
        icon: LABEL_SIZE,
        rest: theme::GREY_400,
    };
    draw_button(ui, icon, look, selected, enabled, tooltip, why)
}

/// One of a row of choices (Stroke ▸ Cap, Join): `size`, drawn like the
/// Align and Path buttons; yellow while `selected`.
pub fn choice_button(
    ui: &mut egui::Ui,
    icon: Icon,
    size: Vec2,
    selected: bool,
    tooltip: &str,
) -> Response {
    let look = Look {
        size,
        icon: SIZE,
        rest: theme::GREY_100,
    };
    draw_button(ui, icon, look, selected, true, tooltip, "")
}

/// How an icon button is drawn: its size, its icon's side, and the icon
/// colour at rest.
#[derive(Clone, Copy)]
struct Look {
    size: Vec2,
    icon: f32,
    rest: Color32,
}

fn draw_button(
    ui: &mut egui::Ui,
    icon: Icon,
    look: Look,
    selected: bool,
    enabled: bool,
    tooltip: &str,
    why: &str,
) -> Response {
    let r = ui
        .add_enabled(
            enabled,
            egui::Button::selectable(selected, "").min_size(look.size),
        )
        .on_hover_text(tooltip)
        .on_disabled_hover_text(why);
    let color = if !enabled {
        theme::GREY_500
    } else if selected {
        theme::YELLOW
    } else if r.hovered() {
        theme::WHITE
    } else {
        look.rest
    };
    let square = Rect::from_center_size(r.rect.center(), Vec2::splat(look.icon));
    paint(ui.painter(), square, icon, color);
    r
}
