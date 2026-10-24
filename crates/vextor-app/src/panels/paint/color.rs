//! The colour editor (spec 09): a Colour row (the pad's tile under the
//! left-hand fields, the hex value and the eyedropper in the right half;
//! an inline error when the hex does not parse), R G B A rows (a slider,
//! its number in the right-hand field column), and the swatches. RGB
//! only. Laid out on the Transform grid (`layout`).

use eframe::egui;
use vextor_core::{Rgba, SwatchKind};

use super::layout::{Row, WIDE_START};
use crate::icons::{self, Icon};
use crate::session::Session;
use crate::swatch_library::Swatches;
use crate::theme;

/// Side of a swatch tile, in points.
const SWATCH: f32 = 16.0;
/// What the hex field accepts, shown when it does not parse.
const HEX_HELP: &str = "Use #RGB, #RRGGBB or #RRGGBBAA";

/// The editor for `color`, keyed by `key` (stable per paint), with the
/// `kind` swatch list (fill or stroke). Returns the colour chosen this
/// frame.
pub(super) fn editor(
    ui: &mut egui::Ui,
    s: &mut Session,
    key: egui::Id,
    color: Rgba,
    kind: SwatchKind,
) -> Option<Rgba> {
    let mut picked = take_pick(s, key);
    let row = Row::new(ui, "Colour");
    picked = row
        .put(ui, row.field1(), |ui| pad(ui, s, key, color))
        .or(picked);
    let (hex_cell, pipette) = row.right_half_and_button();
    picked = row.put(ui, hex_cell, |ui| hex(ui, key, color)).or(picked);
    row.put(ui, pipette, |ui| eyedropper(ui, s, key, pipette.size()));
    hex_error(ui, key);
    picked = sliders(ui, s, color).or(picked);
    swatches(ui, s, key, color, kind).or(picked)
}

/// The preview tile; a click opens the colour pad (egui's picker: a
/// saturation/value square, hue and alpha strips, R G B A; normal blending
/// only, as SVG has no additive colour). The colour stays RGB. Changes
/// made while the pointer is held in the pad are one undo step.
fn pad(ui: &mut egui::Ui, s: &mut Session, key: egui::Id, color: Rgba) -> Option<Rgba> {
    let mut shown = egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a);
    let changed = ui
        .scope(|ui| {
            ui.spacing_mut().interact_size = ui.available_size();
            egui::color_picker::color_edit_button_srgba(
                ui,
                &mut shown,
                egui::color_picker::Alpha::OnlyBlend,
            )
            .on_hover_text("Open the colour pad")
            .changed()
        })
        .inner;
    let gesture = key.with("pad-gesture");
    let in_gesture = ui.data(|d| d.get_temp::<bool>(gesture)).is_some();
    let down = ui.input(|i| i.pointer.any_down());
    if changed && down && !in_gesture {
        s.begin_gesture();
        ui.data_mut(|d| d.insert_temp(gesture, true));
    }
    if in_gesture && !down {
        s.end_gesture();
        ui.data_mut(|d| d.remove::<bool>(gesture));
    }
    let [red, green, blue, alpha] = shown.to_srgba_unmultiplied();
    let picked = Rgba {
        r: red,
        g: green,
        b: blue,
        a: alpha,
    };
    (changed && picked != color).then_some(picked)
}

/// The colour the canvas eyedropper picked for `key`, once.
fn take_pick(s: &mut Session, key: egui::Id) -> Option<Rgba> {
    match s.picked {
        Some((k, c)) if k == key.value() => {
            s.picked = None;
            Some(c)
        }
        _ => None,
    }
}

/// A swatch tile with its own `id` (so its right-click menu stays tied
/// to it).
fn swatch_tile(ui: &mut egui::Ui, id: egui::Id, color: Rgba) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(SWATCH, SWATCH), egui::Sense::hover());
    let r = ui.interact(rect, id, egui::Sense::click());
    paint_tile(ui, rect, color);
    r
}

/// `color` in `rect` over a checkerboard (so alpha shows), outlined.
fn paint_tile(ui: &egui::Ui, rect: egui::Rect, color: Rgba) {
    let size = rect.size();
    let painter = ui.painter();
    let half = egui::vec2(size.x / 2.0, size.y / 2.0);
    painter.rect_filled(rect, 2.0, theme::GREY_100);
    for corner in [rect.min, rect.min + half] {
        painter.rect_filled(
            egui::Rect::from_min_size(corner, half),
            0.0,
            theme::GREY_400,
        );
    }
    let c = egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a);
    painter.rect_filled(rect, 2.0, c);
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, theme::GREY_500),
        egui::StrokeKind::Inside,
    );
}

/// The hex value, committed when the field loses focus. Text that does
/// not parse stays in the field (with [`hex_error`] under it) until it is
/// fixed or `Esc` puts the colour back.
fn hex(ui: &mut egui::Ui, key: egui::Id, color: Rgba) -> Option<Rgba> {
    let draft = key.with("hex-draft");
    let mut text = ui
        .data(|d| d.get_temp::<String>(draft))
        .unwrap_or_else(|| color.to_hex());
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
    if r.changed() {
        ui.data_mut(|d| d.insert_temp(draft, text.clone()));
    }
    if !r.lost_focus() {
        if !r.has_focus()
            && ui
                .data(|d| d.get_temp::<bool>(key.with("hex-bad")))
                .is_none()
        {
            ui.data_mut(|d| d.remove::<String>(draft));
        }
        return None;
    }
    let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
    let parsed = vextor_core::Rgba::from_hex(&text).ok();
    if escaped || parsed.is_some() {
        forget_hex(ui.ctx(), key);
    } else {
        ui.data_mut(|d| {
            d.insert_temp(key.with("hex-bad"), true);
            d.insert_temp(pending_hex(), key);
        });
    }
    parsed.filter(|_| !escaped)
}

/// Where the key of the colour editor whose hex value does not parse is
/// kept (one at a time), so `Esc` reaches it after the field lost focus.
fn pending_hex() -> egui::Id {
    egui::Id::unique("paint-hex-pending")
}

/// Drops `key`'s hex draft and error: the field shows the colour again.
fn forget_hex(ctx: &egui::Context, key: egui::Id) {
    ctx.data_mut(|d| {
        d.remove::<String>(key.with("hex-draft"));
        d.remove::<bool>(key.with("hex-bad"));
        if d.get_temp::<egui::Id>(pending_hex()) == Some(key) {
            d.remove::<egui::Id>(pending_hex());
        }
    });
}

/// `Esc` while a hex value that does not parse is showing (its field no
/// longer focused): puts that editor's colour back and takes the key, so
/// it does not also deselect. Called before the app's shortcuts. Returns
/// true when it took the `Esc`.
pub fn escape_bad_hex(ctx: &egui::Context) -> bool {
    let Some(key) = ctx.data(|d| d.get_temp::<egui::Id>(pending_hex())) else {
        return false;
    };
    if !ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        return false;
    }
    forget_hex(ctx, key);
    true
}

/// The inline error under a hex value that does not parse.
fn hex_error(ui: &mut egui::Ui, key: egui::Id) {
    if ui
        .data(|d| d.get_temp::<bool>(key.with("hex-bad")))
        .is_some()
    {
        ui.label(
            egui::RichText::new(HEX_HELP)
                .small()
                .color(ui.visuals().error_fg_color),
        );
    }
}

/// The eyedropper button: arms the canvas to pick a colour for `key`
/// (click on the canvas; `Esc` or a second click here cancels).
fn eyedropper(ui: &mut egui::Ui, s: &mut Session, key: egui::Id, size: egui::Vec2) {
    let armed = s.eyedropper == Some(key.value());
    let tip = "Pick a colour from the canvas";
    if icons::quiet_button(ui, Icon::Eyedropper, size, armed, true, tip, "").clicked() {
        s.eyedropper = (!armed).then_some(key.value());
    }
}

/// R, G, B and A rows: a slider (no number of its own) under the fields'
/// left side, its number (0–255) in the right-hand field column. A drag is
/// one undo step.
fn sliders(ui: &mut egui::Ui, s: &mut Session, color: Rgba) -> Option<Rgba> {
    let mut out = color;
    for name in ["R", "G", "B", "A"] {
        let channel = match name {
            "R" => &mut out.r,
            "G" => &mut out.g,
            "B" => &mut out.b,
            _ => &mut out.a,
        };
        let row = Row::new(ui, name);
        let slid = row.put(ui, row.slider(), |ui| {
            ui.spacing_mut().slider_width = ui.available_width();
            ui.add(egui::Slider::new(channel, 0..=255).show_value(false))
        });
        let typed = row.put(ui, row.field2(), |ui| {
            ui.add_sized(
                ui.available_size(),
                egui::DragValue::new(channel).range(0..=255),
            )
        });
        for r in [slid, typed] {
            if r.drag_started() {
                s.begin_gesture();
            }
            if r.drag_stopped() {
                s.end_gesture();
            }
        }
    }
    (out != color).then_some(out)
}

/// The library's `kind` swatches (fill or stroke list, shared by every
/// document): a click applies one, `+` adds `color` to that list only,
/// the right-click menu renames or deletes. Returns the swatch clicked.
fn swatches(
    ui: &mut egui::Ui,
    s: &Session,
    key: egui::Id,
    color: Rgba,
    kind: SwatchKind,
) -> Option<Rgba> {
    let mut picked = None;
    let library = &s.swatches;
    // `+` in the label column (under Colour, R, G…); the tiles from under
    // the Colour tile, wrapping before the grid's right edge.
    ui.horizontal_top(|ui| {
        let can = library.can_add(kind, color);
        let why = "This colour is in this swatch list already (or there are 256 swatches)";
        let tip = "Add to swatches";
        // Only as wide as the icon, so the `+` starts where the labels do.
        let add = icons::tight_button(ui, Icon::Add, can, tip, why);
        if add.clicked() {
            library.add(kind, color);
        }
        let used = add.rect.right() - ui.min_rect().left() + ui.spacing().item_spacing.x;
        ui.add_space((WIDE_START - used).max(0.0));
        ui.vertical(|ui| {
            ui.set_max_width(super::super::GRID_WIDTH - WIDE_START);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                // `i` indexes the whole library, which rename and delete edit.
                for (i, sw) in library.list().into_iter().filter(|(_, sw)| sw.kind == kind) {
                    let tip = sw.name.clone().unwrap_or_else(|| sw.color.to_hex());
                    let r = swatch_tile(ui, key.with(("swatch", i)), sw.color).on_hover_text(tip);
                    if r.clicked() {
                        picked = Some(sw.color);
                    }
                    // Clicks inside (into the name field) keep the menu open; a
                    // click outside, Esc, Enter or Delete closes it.
                    egui::Popup::context_menu(&r)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| swatch_menu(ui, library, key.with(i), i, sw.name.as_deref()));
                }
            });
        });
    });
    picked
}

/// Rename (a field committed on Enter) and Delete for swatch `index`,
/// named `name` so far.
fn swatch_menu(
    ui: &mut egui::Ui,
    library: &Swatches,
    key: egui::Id,
    index: usize,
    name: Option<&str>,
) {
    let draft = key.with("name");
    let mut text = ui
        .data(|d| d.get_temp::<String>(draft))
        .unwrap_or_else(|| name.unwrap_or_default().to_owned());
    ui.label("Name");
    let r = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(key.with("name-field"))
            .hint_text("Type, then Enter")
            .char_limit(vextor_core::MAX_SWATCH_NAME_CHARS)
            .desired_width(140.0),
    );
    if r.changed() {
        ui.data_mut(|d| d.insert_temp(draft, text.clone()));
    }
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        library.rename(index, &text);
        ui.data_mut(|d| d.remove::<String>(draft));
        ui.close();
    }
    if ui.button("Delete swatch").clicked() {
        library.delete(index);
        ui.data_mut(|d| d.remove::<String>(draft));
        ui.close();
    }
}

#[cfg(test)]
mod tests {
    use vextor_core::Document;

    use super::*;

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// One frame: Esc goes to [`escape_bad_hex`] first, as in the app,
    /// then the editor is shown. Returns where it started, whether the Esc
    /// was taken, and the hex value the field would show.
    fn frame(
        ctx: &egui::Context,
        s: &mut Session,
        events: Vec<egui::Event>,
    ) -> (egui::Pos2, bool, String) {
        let id = egui::Id::unique("test-colour");
        let input = egui::RawInput {
            events,
            ..egui::RawInput::default()
        };
        let mut out = (egui::Pos2::ZERO, false, String::new());
        ctx.run_ui(input, |ui| {
            let taken = escape_bad_hex(ui.ctx());
            let at = ui.cursor().min;
            let _ = editor(ui, s, id, Rgba::rgb(0x11, 0x22, 0x33), SwatchKind::Fill);
            let shown = ui
                .data(|d| d.get_temp::<String>(id.with("hex-draft")))
                .unwrap_or_else(|| Rgba::rgb(0x11, 0x22, 0x33).to_hex());
            out = (at, taken, shown);
        })
        .textures_delta
        .clear();
        out
    }

    #[test]
    fn esc_after_leaving_a_bad_hex_restores_the_colour_instead_of_deselecting() {
        let mut s = Session::new(Document::new("hex"));
        let ctx = egui::Context::default();
        let (origin, ..) = frame(&ctx, &mut s, vec![]);
        // Click into the hex field, in the grid's right half.
        let field = origin + egui::vec2(super::super::super::GRID_WIDTH - 60.0, 10.0);
        let click = |pressed| egui::Event::PointerButton {
            pos: field,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        for events in [
            vec![egui::Event::PointerMoved(field)],
            vec![click(true)],
            vec![click(false)],
            vec![key(egui::Key::A, egui::Modifiers::COMMAND)],
            vec![egui::Event::Text("#11".to_owned())],
            vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
            vec![],
        ] {
            let (_, taken, _) = frame(&ctx, &mut s, events);
            assert!(!taken);
        }
        let (_, _, shown) = frame(&ctx, &mut s, vec![]);
        assert_eq!(shown, "#11", "the bad value stays, with its error");
        let (_, taken, shown) = frame(
            &ctx,
            &mut s,
            vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        assert!(taken, "Esc is taken, so the app does not deselect");
        assert_eq!(shown, "#112233", "the colour is back");
        let (_, taken, _) = frame(
            &ctx,
            &mut s,
            vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        assert!(!taken, "a later Esc deselects as usual");
    }

    #[test]
    fn a_swatch_is_renamed_by_typing_in_its_menu_which_stays_open() {
        let mut s = Session::new(Document::new("swatches"));
        assert!(s.swatches.add(SwatchKind::Fill, Rgba::rgb(255, 0, 0)));
        let ctx = egui::Context::default();
        let id = egui::Id::unique("test-colour");
        let run = |s: &mut Session, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                ..egui::RawInput::default()
            };
            ctx.run_ui(input, |ui| {
                let _ = editor(ui, s, id, Rgba::BLACK, SwatchKind::Fill);
            })
            .textures_delta
            .clear();
        };
        let press = |pos, button, pressed| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(&mut s, vec![]);
        let tile = ctx
            .read_response(id.with(("swatch", 0_usize)))
            .expect("swatch shown")
            .rect
            .center();
        let right = egui::PointerButton::Secondary;
        for events in [
            vec![egui::Event::PointerMoved(tile)],
            vec![press(tile, right, true)],
            vec![press(tile, right, false)],
            vec![],
        ] {
            run(&mut s, events);
        }
        let field = ctx
            .read_response(id.with(0_usize).with("name-field"))
            .expect("the menu is open")
            .rect
            .center();
        let left = egui::PointerButton::Primary;
        for events in [
            vec![egui::Event::PointerMoved(field)],
            vec![press(field, left, true)],
            vec![press(field, left, false)],
            vec![egui::Event::Text("brand".to_owned())],
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
            vec![],
        ] {
            run(&mut s, events);
        }
        assert_eq!(s.swatches.list()[0].1.name.as_deref(), Some("brand"));
        assert_eq!(
            s.edit_menu().undo,
            None,
            "the library is not in the document's history"
        );
    }

    #[test]
    fn swatches_start_under_the_colour_tile_after_the_add_button() {
        let mut s = Session::new(Document::new("swatches"));
        assert!(s.swatches.add(SwatchKind::Fill, Rgba::rgb(255, 0, 0)));
        let ctx = egui::Context::default();
        let id = egui::Id::unique("test-colour-row");
        let mut left = 0.0;
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                left = ui.cursor().min.x;
                let _ = editor(ui, &mut s, id, Rgba::BLACK, SwatchKind::Fill);
            })
            .textures_delta
            .clear();
        }
        let tile = ctx
            .read_response(id.with(("swatch", 0_usize)))
            .expect("swatch shown")
            .rect;
        assert!((tile.left() - (left + WIDE_START)).abs() < 0.5, "{tile:?}");
    }
}
