//! The inspector's Text section (spec 05c): font family and style, size,
//! line height, letter spacing, alignment, background and border. The
//! glyphs' colour is Inspector ▸ Fill and Stroke; background and border
//! use the same paint picker (spec 09). Values show the first selected
//! text; a change applies to every selected text, each keeping its other
//! values.

use eframe::egui::{self, RichText};
use vextor_core::{NodeId, NodeKind, Paint, Rgba, Text, TextAlign};

use super::fonts::{family_picker, remember_family, style_picker};
use super::restyle::{Edit, restyle};
use super::{FIELD_SIZE, GRID_SPACING, LABEL_WIDTH};
use crate::icons::{self, Icon};
use crate::paint_edit::{self, Target};
use crate::session::Session;
use crate::theme;

/// The selected text objects (direct selection only).
fn selected_texts(s: &Session) -> Vec<(NodeId, Text)> {
    s.selected()
        .into_iter()
        .filter_map(|id| match &s.doc.nodes.get(&id)?.kind {
            NodeKind::Text(t) => Some((id, t.clone())),
            _ => None,
        })
        .collect()
}

/// The Text section, shown when text is selected. Returns a message when
/// an edit was refused.
pub fn section(ui: &mut egui::Ui, s: &mut Session) -> Option<String> {
    let texts = selected_texts(s);
    if texts.is_empty() {
        return None;
    }
    let mut message = None;
    super::section(ui, "Text", |ui| message = body(ui, s, &texts));
    message
}

/// The Text section's fields for `texts` (not empty).
fn body(ui: &mut egui::Ui, s: &mut Session, texts: &[(NodeId, Text)]) -> Option<String> {
    let (first_id, first) = texts.first()?.clone();
    font_note(ui, s, first_id, &first);

    let mut edit: Option<Edit> = None;
    let db = std::sync::Arc::clone(s.fonts.db());
    egui::Grid::new("text-inspector")
        .num_columns(2)
        .spacing(GRID_SPACING)
        .min_col_width(LABEL_WIDTH)
        .show(ui, |ui| {
            ui.label("Font");
            if let Some(family) = family_picker(ui, &db, &first.font.family) {
                remember_family(ui.ctx(), &family);
                edit = Some(Edit::Family(family));
            }
            ui.end_row();
            ui.label("Style");
            if let Some(style) = style_picker(ui, &db, &first.font) {
                edit = Some(Edit::Style(style));
            }
            ui.end_row();
            number(
                ui,
                s,
                "Size",
                first.size,
                0.5,
                1.0..=1000.0,
                &mut edit,
                Edit::Size,
            );
            number(
                ui,
                s,
                "Line height",
                first.line_height,
                0.01,
                0.5..=5.0,
                &mut edit,
                Edit::LineHeight,
            );
            number(
                ui,
                s,
                "Spacing",
                first.letter_spacing,
                0.1,
                -50.0..=200.0,
                &mut edit,
                Edit::Spacing,
            );
            ui.label("Align");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (align, icon, tip) in [
                    (TextAlign::Left, Icon::TextLeft, "Align text left"),
                    (TextAlign::Centre, Icon::TextCentre, "Centre text"),
                    (TextAlign::Right, Icon::TextRight, "Align text right"),
                ] {
                    if icons::button(ui, icon, first.align == align, true, tip, "").clicked() {
                        edit = Some(Edit::Align(align));
                    }
                }
            });
            ui.end_row();
        });

    let ids: Vec<NodeId> = texts.iter().map(|(id, _)| *id).collect();
    let frame_message = frame_fields(ui, s, &ids, &first, &mut edit);

    let Some(edit) = edit else {
        return frame_message;
    };
    let cmd = restyle(texts, &edit);
    s.apply(cmd)
        .err()
        .map(|e| format!("Change text failed: {e}"))
        .or(frame_message)
}

/// "Font missing: X — showing Y", or that fonts are still loading.
fn font_note(ui: &mut egui::Ui, s: &Session, id: NodeId, t: &Text) {
    let layout = s.fonts.layout(id, t);
    let Some(m) = &layout.missing else {
        return;
    };
    let note = match &m.shown {
        Some(shown) => format!("Font missing: {} — showing {shown}", m.wanted),
        None if s.fonts.db().is_empty() => "Loading fonts…".to_owned(),
        None => format!("Font missing: {}", m.wanted),
    };
    ui.label(RichText::new(note).color(theme::YELLOW).small());
}

/// Background (its paint, padding, radius) and Border (its paint and
/// stroke): each a ticked section title with its fields underneath while
/// it is on. Paint changes are applied here (`paint_edit`); padding and
/// radius go into `edit`. Returns a message when a paint edit was refused.
fn frame_fields(
    ui: &mut egui::Ui,
    s: &mut Session,
    ids: &[NodeId],
    t: &Text,
    edit: &mut Option<Edit>,
) -> Option<String> {
    let frame = t.frame.clone().unwrap_or_else(paint_edit::empty_frame);
    let mut message = None;
    let background = frame.fill != Paint::None;
    if section_toggle(ui, "Background", background) {
        let paint = if background {
            Paint::None
        } else {
            Paint::Solid(Rgba::rgb(0xE7, 0xE7, 0xE7))
        };
        message = toggle(s, ids, Target::Background, paint);
    }
    if background {
        message = super::paint::picker(ui, s, ids, Target::Background).or(message);
        egui::Grid::new("text-background-inspector")
            .num_columns(2)
            .spacing(GRID_SPACING)
            .min_col_width(LABEL_WIDTH)
            .show(ui, |ui| {
                let [top, right, ..] = frame.padding;
                number(
                    ui,
                    s,
                    "Padding X",
                    right,
                    0.5,
                    0.0..=500.0,
                    edit,
                    Edit::PaddingX,
                );
                number(
                    ui,
                    s,
                    "Padding Y",
                    top,
                    0.5,
                    0.0..=500.0,
                    edit,
                    Edit::PaddingY,
                );
                number(
                    ui,
                    s,
                    "Radius",
                    frame.radius.tl,
                    0.5,
                    0.0..=500.0,
                    edit,
                    Edit::Radius,
                );
            });
    }

    let border = frame.stroke.is_some();
    if section_toggle(ui, "Border", border) {
        let paint = if border {
            Paint::None
        } else {
            Paint::Solid(Rgba::BLACK)
        };
        message = toggle(s, ids, Target::Border, paint).or(message);
    }
    if border {
        message = super::paint::picker(ui, s, ids, Target::Border).or(message);
    }
    message
}

/// Turns a background or border on (with `paint`) or off (`Paint::None`).
fn toggle(s: &mut Session, ids: &[NodeId], target: Target, paint: Paint) -> Option<String> {
    let cmd = paint_edit::command(&s.doc, ids, target, &paint_edit::Edit::Paint(paint))?;
    s.apply(cmd)
        .err()
        .map(|e| format!("{} failed: {e}", target.label()))
}

/// A section title with a tick box that turns the section on and off.
/// Returns true when it was clicked.
pub(super) fn section_toggle(ui: &mut egui::Ui, title: &str, on: bool) -> bool {
    ui.add_space(8.0);
    let mut checked = on;
    let clicked = ui
        .checkbox(&mut checked, egui::RichText::new(title).strong())
        .clicked();
    ui.add_space(2.0);
    clicked
}

/// A labelled number; a drag is one undo step (a history gesture).
#[allow(clippy::too_many_arguments)] // one grid row's label, value and edit
fn number(
    ui: &mut egui::Ui,
    s: &mut Session,
    label: &str,
    value: f64,
    speed: f64,
    range: std::ops::RangeInclusive<f64>,
    edit: &mut Option<Edit>,
    make: impl Fn(f64) -> Edit,
) {
    ui.label(label);
    let mut v = value;
    let r = ui.add_sized(
        FIELD_SIZE,
        egui::DragValue::new(&mut v)
            .speed(speed)
            .range(range)
            // Only what the user enters is clamped: showing a text resized
            // past the range must not change it.
            .clamp_existing_to_range(false)
            .max_decimals(2),
    );
    if r.drag_started() {
        s.begin_gesture();
    }
    if r.changed() && (v - value).abs() > vextor_core::EPS {
        *edit = Some(make(v));
    }
    if r.drag_stopped() {
        s.end_gesture();
    }
    ui.end_row();
}

/// Artboard background (spec 05e): a colour tile that opens the colour picker, as wide as the layout's
/// interact size; the colour picked, if any.
pub(super) fn color_tile(ui: &mut egui::Ui, color: Rgba) -> Option<Rgba> {
    let mut swatch = egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a);
    // Normal blending only: SVG has no additive colour.
    let alpha = egui::color_picker::Alpha::OnlyBlend;
    if !egui::color_picker::color_edit_button_srgba(ui, &mut swatch, alpha).changed() {
        return None;
    }
    let [r, g, b, a] = swatch.to_srgba_unmultiplied();
    Some(Rgba { r, g, b, a })
}

/// A hex field `width` wide, committed when it loses focus; invalid text
/// is dropped.
pub(super) fn hex_field(ui: &mut egui::Ui, id: &str, color: Rgba, width: f32) -> Option<Rgba> {
    let key = egui::Id::unique(id);
    let mut text = ui
        .data(|d| d.get_temp::<String>(key))
        .unwrap_or_else(|| color.to_hex());
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(width));
    if r.changed() {
        ui.data_mut(|d| d.insert_temp(key, text.clone()));
    }
    let mut picked = None;
    if r.lost_focus() {
        picked = Rgba::from_hex(text.trim()).ok();
        ui.data_mut(|d| d.remove::<String>(key));
    } else if !r.has_focus() {
        ui.data_mut(|d| d.remove::<String>(key));
    }
    picked
}

#[cfg(test)]
mod tests {
    use vextor_core::{Command, Document, Node};
    use vextor_geom::FaceStyle;

    use super::*;
    use crate::new_doc::new_document;

    fn session_with(texts: Vec<Text>) -> (Session, Vec<NodeId>) {
        let mut doc: Document = new_document("a", 400.0, 200.0);
        let layer = doc.layers[0];
        let ids: Vec<NodeId> = texts.iter().map(|_| doc.alloc_node_id()).collect();
        let nodes = ids
            .iter()
            .zip(texts)
            .map(|(&id, t)| Node::new(id, NodeKind::Text(t)))
            .collect();
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes,
        })
        .expect("inserts");
        let mut s = Session::new(doc);
        s.select(ids.clone());
        (s, ids)
    }

    fn text(s: &Session, id: NodeId) -> &Text {
        match &s.doc.nodes[&id].kind {
            NodeKind::Text(t) => t,
            _ => panic!("text"),
        }
    }

    #[test]
    fn a_change_applies_to_every_selected_text_keeping_the_rest() {
        let a = Text {
            size: 12.0,
            ..Text::new("a", "Inter")
        };
        let b = Text {
            size: 30.0,
            ..Text::new("b", "Menlo")
        };
        let (mut s, ids) = session_with(vec![a, b]);
        let texts = selected_texts(&s);
        s.apply(restyle(
            &texts,
            &Edit::Style(FaceStyle {
                weight: 700,
                italic: false,
            }),
        ))
        .expect("applies");
        assert_eq!(text(&s, ids[0]).font.family, "Inter");
        assert_eq!(text(&s, ids[1]).font.family, "Menlo");
        assert_eq!(text(&s, ids[0]).font.weight, 700);
        assert_eq!(text(&s, ids[1]).font.weight, 700);
        assert!((text(&s, ids[1]).size - 30.0).abs() < 1e-9);
        assert_eq!(s.edit_menu().undo, Some("Change font"));
    }

    #[test]
    fn background_and_border_make_and_remove_the_frame() {
        let (mut s, ids) = session_with(vec![Text::new("Buy now", "Inter")]);
        let step = |s: &mut Session, e: Edit| {
            let texts = selected_texts(s);
            s.apply(restyle(&texts, &e)).expect("applies");
        };
        let blue = Paint::Solid(Rgba::rgb(30, 144, 255));
        assert_eq!(toggle(&mut s, &ids, Target::Background, blue), None);
        let f = text(&s, ids[0]).frame.clone().expect("framed");
        assert_eq!(f.fill, blue);
        assert_eq!(f.padding, [8.0, 16.0, 8.0, 16.0]);
        step(&mut s, Edit::PaddingX(4.0));
        step(&mut s, Edit::Radius(12.0));
        let black = Paint::Solid(Rgba::BLACK);
        assert_eq!(toggle(&mut s, &ids, Target::Border, black), None);
        let width =
            paint_edit::command(&s.doc, &ids, Target::Border, &paint_edit::Edit::Width(2.0));
        s.apply(width.expect("a border")).expect("applies");
        let f = text(&s, ids[0]).frame.clone().expect("framed");
        assert_eq!(f.padding, [8.0, 4.0, 8.0, 4.0]);
        assert!((f.radius.br - 12.0).abs() < 1e-9);
        assert!((f.stroke.expect("border").width - 2.0).abs() < 1e-9);

        assert_eq!(toggle(&mut s, &ids, Target::Background, Paint::None), None);
        assert!(text(&s, ids[0]).frame.is_some(), "the border remains");
        assert_eq!(toggle(&mut s, &ids, Target::Border, Paint::None), None);
        assert_eq!(text(&s, ids[0]).frame, None);
        assert_eq!(s.edit_menu().undo, Some("Change border"));
    }

    #[test]
    fn padding_without_a_background_or_border_makes_no_frame() {
        let (mut s, ids) = session_with(vec![Text::new("x", "Inter")]);
        let texts = selected_texts(&s);
        s.apply(restyle(&texts, &Edit::PaddingX(4.0)))
            .expect("applies");
        assert_eq!(text(&s, ids[0]).frame, None);
    }

    #[test]
    fn size_and_alignment() {
        let (mut s, ids) = session_with(vec![Text::new("x", "Inter")]);
        for e in [
            Edit::Size(18.0),
            Edit::LineHeight(1.5),
            Edit::Spacing(0.5),
            Edit::Align(TextAlign::Centre),
        ] {
            let texts = selected_texts(&s);
            s.apply(restyle(&texts, &e)).expect("applies");
        }
        let t = text(&s, ids[0]);
        assert!((t.size - 18.0).abs() < 1e-9);
        assert!((t.line_height - 1.5).abs() < 1e-9);
        assert!((t.letter_spacing - 0.5).abs() < 1e-9);
        assert_eq!(t.align, TextAlign::Centre);
    }

    #[test]
    fn showing_values_outside_the_ranges_leaves_them_alone() {
        let big = Text {
            size: 1_500.0,
            letter_spacing: 400.0,
            line_height: 9.0,
            ..Text::new("Huge", "Inter")
        };
        let (mut s, ids) = session_with(vec![big]);
        let ctx = egui::Context::default();
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                let _ = section(ui, &mut s);
            })
            .textures_delta
            .clear();
        }
        let t = text(&s, ids[0]);
        assert!((t.size - 1_500.0).abs() < 1e-9);
        assert!((t.letter_spacing - 400.0).abs() < 1e-9);
        assert!((t.line_height - 9.0).abs() < 1e-9);
        assert_eq!(s.edit_menu().undo, None, "nothing was recorded");
    }
}
