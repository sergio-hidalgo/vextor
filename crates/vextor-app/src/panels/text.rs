//! The inspector's Text section (spec 05c): font family and style, size,
//! line height, letter spacing, alignment, text colour, background and
//! border. Values show the first selected text; a change applies to every
//! selected text, each keeping its other values.

use eframe::egui::{self, RichText};
use vextor_core::{
    Command, CornerRadii, FontSpec, NodeId, NodeKind, Paint, Rgba, Stroke, Text, TextAlign,
    TextFrame,
};
use vextor_geom::{FaceStyle, FontDb};

use crate::session::Session;
use crate::theme;

/// Most families remembered as recently used.
const RECENT_LIMIT: usize = 8;
/// Rows of the family list shown at once.
const FAMILY_ROWS: f32 = 12.0;
/// Size the family names are shown at in the list.
const FAMILY_PREVIEW_SIZE: f32 = 15.0;

fn recent_id() -> egui::Id {
    egui::Id::unique("vextor-recent-font-families")
}

fn search_id() -> egui::Id {
    egui::Id::unique("vextor-font-family-search")
}

/// Font families picked most recently, newest first (this run only).
#[must_use]
pub fn recent_families(ctx: &egui::Context) -> Vec<String> {
    ctx.data(|d| d.get_temp::<Vec<String>>(recent_id()))
        .unwrap_or_default()
}

/// Puts `family` first in the recently used list.
pub fn remember_family(ctx: &egui::Context, family: &str) {
    let mut recent = recent_families(ctx);
    recent.retain(|f| f != family);
    recent.insert(0, family.to_owned());
    recent.truncate(RECENT_LIMIT);
    ctx.data_mut(|d| d.insert_temp(recent_id(), recent));
}

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
    let (first_id, first) = texts.first()?.clone();
    ui.add_space(12.0);
    ui.separator();
    ui.strong("Text");
    font_note(ui, s, first_id, &first);

    let mut edit: Option<Edit> = None;
    let db = std::sync::Arc::clone(s.fonts.db());
    egui::Grid::new("text-inspector")
        .num_columns(2)
        .spacing([6.0, 6.0])
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
                for (align, name) in [
                    (TextAlign::Left, "Left"),
                    (TextAlign::Centre, "Centre"),
                    (TextAlign::Right, "Right"),
                ] {
                    if ui.selectable_label(first.align == align, name).clicked() {
                        edit = Some(Edit::Align(align));
                    }
                }
            });
            ui.end_row();
            ui.label("Colour");
            if let Some(c) = color_field(ui, "text-colour", solid(first.style.fill)) {
                edit = Some(Edit::Color(c));
            }
            ui.end_row();
        });

    frame_fields(ui, s, &first, &mut edit);

    let edit = edit?;
    let cmd = restyle(&texts, &edit);
    s.apply(cmd)
        .err()
        .map(|e| format!("Change text failed: {e}"))
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

/// Background (fill, padding, radius) and border (colour, width).
fn frame_fields(ui: &mut egui::Ui, s: &mut Session, t: &Text, edit: &mut Option<Edit>) {
    let frame = t.frame.clone().unwrap_or_else(empty_frame);
    let background = frame.fill != Paint::None;
    let border = frame.stroke.clone();
    egui::Grid::new("text-frame-inspector")
        .num_columns(2)
        .spacing([6.0, 6.0])
        .show(ui, |ui| {
            let mut on = background;
            if ui.checkbox(&mut on, "Background").changed() {
                *edit = Some(Edit::Background(on.then_some(Rgba::rgb(0xE7, 0xE7, 0xE7))));
            }
            if background && let Some(c) = color_field(ui, "text-background", solid(frame.fill)) {
                *edit = Some(Edit::Background(Some(c)));
            }
            ui.end_row();
            if background {
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
            }
            let mut on = border.is_some();
            if ui.checkbox(&mut on, "Border").changed() {
                *edit = Some(Edit::Border(on.then(|| Stroke {
                    paint: Paint::Solid(Rgba::BLACK),
                    ..Stroke::default()
                })));
            }
            if let Some(b) = &border
                && let Some(c) = color_field(ui, "text-border", solid(b.paint))
            {
                *edit = Some(Edit::Border(Some(Stroke {
                    paint: Paint::Solid(c),
                    ..b.clone()
                })));
            }
            ui.end_row();
            if border.is_some() {
                let width = border.as_ref().map_or(1.0, |b| b.width);
                number(
                    ui,
                    s,
                    "Border width",
                    width,
                    0.1,
                    0.0..=100.0,
                    edit,
                    Edit::BorderWidth,
                );
            }
        });
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
    let r = ui.add(
        egui::DragValue::new(&mut v)
            .speed(speed)
            .range(range)
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

/// A hex field with a swatch (spec 09's paint picker replaces it).
fn color_field(ui: &mut egui::Ui, id: &str, current: Option<Rgba>) -> Option<Rgba> {
    let mut picked = None;
    ui.horizontal(|ui| {
        let color = current.unwrap_or(Rgba::BLACK);
        let mut swatch = egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a);
        if ui.color_edit_button_srgba(&mut swatch).changed() {
            let [red, green, blue, alpha] = swatch.to_srgba_unmultiplied();
            picked = Some(Rgba {
                r: red,
                g: green,
                b: blue,
                a: alpha,
            });
        }
        let key = egui::Id::unique(id);
        let mut text = ui
            .data(|d| d.get_temp::<String>(key))
            .unwrap_or_else(|| color.to_hex());
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(72.0));
        if r.changed() {
            ui.data_mut(|d| d.insert_temp(key, text.clone()));
        }
        if r.lost_focus() {
            if let Ok(parsed) = Rgba::from_hex(text.trim()) {
                picked = Some(parsed);
            }
            ui.data_mut(|d| d.remove::<String>(key));
        } else if !r.has_focus() {
            ui.data_mut(|d| d.remove::<String>(key));
        }
    });
    picked
}

/// The family list: a search field, recently used first, each family's
/// name in its own face once that face is loaded (only rows on screen
/// load theirs). Returns the family picked.
fn family_picker(ui: &mut egui::Ui, db: &FontDb, current: &str) -> Option<String> {
    let mut picked = None;
    egui::ComboBox::from_id_salt("text-font-family")
        .selected_text(current)
        .width(150.0)
        .show_ui(ui, |ui| {
            let ctx = ui.ctx().clone();
            let mut query = ctx
                .data(|d| d.get_temp::<String>(search_id()))
                .unwrap_or_default();
            let r = ui.add(
                egui::TextEdit::singleline(&mut query)
                    .hint_text("Search fonts")
                    .desired_width(f32::INFINITY),
            );
            if r.changed() {
                ctx.data_mut(|d| d.insert_temp(search_id(), query.clone()));
            }
            r.request_focus();
            let list = family_list(db.families(), &recent_families(&ctx), &query);
            if db.is_empty() {
                ui.weak("Loading fonts…");
                return;
            }
            let row = FAMILY_PREVIEW_SIZE + 8.0;
            egui::ScrollArea::vertical()
                .max_height(row * FAMILY_ROWS)
                .show_rows(ui, row, list.len(), |ui, rows| {
                    for family in &list[rows] {
                        let text = family_label(&ctx, db, family);
                        if ui.selectable_label(family == current, text).clicked() {
                            picked = Some(family.clone());
                        }
                    }
                });
        });
    if picked.is_some() {
        ui.ctx().data_mut(|d| d.remove::<String>(search_id()));
    }
    picked
}

/// `family` written in its own face when egui has it, else in the UI font
/// while the face loads (it is usable from the next frame).
fn family_label(ctx: &egui::Context, db: &FontDb, family: &str) -> RichText {
    let egui_family = egui::FontFamily::Name(format!("vextor:{family}").into());
    let loaded = ctx.fonts(|f| f.definitions().families.contains_key(&egui_family));
    if loaded {
        return RichText::new(family).font(egui::FontId::new(FAMILY_PREVIEW_SIZE, egui_family));
    }
    if let Some((data, index)) = db.sample_face(family) {
        let mut font = egui::FontData::from_owned(data.as_ref().clone());
        font.index = index;
        ctx.add_font(egui::epaint::text::FontInsert::new(
            &format!("vextor:{family}"),
            font,
            vec![egui::epaint::text::InsertFontFamily {
                family: egui_family,
                priority: egui::epaint::text::FontPriority::Highest,
            }],
        ));
        ctx.request_repaint();
    }
    RichText::new(family).size(FAMILY_PREVIEW_SIZE)
}

/// The families to list: recently used (still installed) first, then
/// the rest, filtered by `query` (case-insensitive, anywhere in the name).
fn family_list(all: &[String], recent: &[String], query: &str) -> Vec<String> {
    let q = query.trim().to_lowercase();
    let hit = |f: &String| q.is_empty() || f.to_lowercase().contains(&q);
    let recent: Vec<String> = recent
        .iter()
        .filter(|r| all.contains(r) && hit(r))
        .cloned()
        .collect();
    recent
        .iter()
        .cloned()
        .chain(
            all.iter()
                .filter(|f| hit(f) && !recent.contains(f))
                .cloned(),
        )
        .collect()
}

/// The faces the family has, by name. Returns the style picked.
fn style_picker(ui: &mut egui::Ui, db: &FontDb, font: &FontSpec) -> Option<FaceStyle> {
    let styles = db.styles(&font.family);
    let current = FaceStyle {
        weight: font.weight,
        italic: font.italic,
    };
    let mut picked = None;
    ui.add_enabled_ui(!styles.is_empty(), |ui| {
        egui::ComboBox::from_id_salt("text-font-style")
            .selected_text(style_name(current))
            .width(150.0)
            .show_ui(ui, |ui| {
                for style in styles {
                    if ui
                        .selectable_label(style == current, style_name(style))
                        .clicked()
                    {
                        picked = Some(style);
                    }
                }
            });
    });
    picked
}

/// "Bold", "Light Italic", "Regular"…: CSS weight names.
fn style_name(style: FaceStyle) -> String {
    let weight = match style.weight {
        0..=149 => "Thin",
        150..=249 => "ExtraLight",
        250..=349 => "Light",
        350..=449 => "Regular",
        450..=549 => "Medium",
        550..=649 => "SemiBold",
        650..=749 => "Bold",
        750..=849 => "ExtraBold",
        _ => "Black",
    };
    match (weight, style.italic) {
        ("Regular", true) => "Italic".to_owned(),
        (w, true) => format!("{w} Italic"),
        (w, false) => w.to_owned(),
    }
}

fn solid(p: Paint) -> Option<Rgba> {
    match p {
        Paint::Solid(c) => Some(c),
        _ => None,
    }
}

fn empty_frame() -> TextFrame {
    TextFrame {
        padding: [8.0, 16.0, 8.0, 16.0],
        radius: CornerRadii {
            tl: 6.0,
            tr: 6.0,
            br: 6.0,
            bl: 6.0,
        },
        fill: Paint::None,
        stroke: None,
    }
}

/// One change from the Text section.
#[derive(Clone, Debug, PartialEq)]
enum Edit {
    Family(String),
    Style(FaceStyle),
    Size(f64),
    LineHeight(f64),
    Spacing(f64),
    Align(TextAlign),
    Color(Rgba),
    /// Background colour, or `None` to remove it.
    Background(Option<Rgba>),
    PaddingX(f64),
    PaddingY(f64),
    Radius(f64),
    /// Border, or `None` to remove it.
    Border(Option<Stroke>),
    BorderWidth(f64),
}

impl Edit {
    fn apply(&self, t: &mut Text) {
        let frame = || t.frame.clone().unwrap_or_else(empty_frame);
        match self {
            Self::Family(f) => t.font.family.clone_from(f),
            Self::Style(s) => {
                t.font.weight = s.weight;
                t.font.italic = s.italic;
            }
            Self::Size(v) => t.size = *v,
            Self::LineHeight(v) => t.line_height = *v,
            Self::Spacing(v) => t.letter_spacing = *v,
            Self::Align(a) => t.align = *a,
            Self::Color(c) => t.style.fill = Paint::Solid(*c),
            Self::Background(c) => {
                let mut f = frame();
                f.fill = c.map_or(Paint::None, Paint::Solid);
                t.frame = Some(f);
            }
            Self::PaddingX(v) => {
                let mut f = frame();
                f.padding[1] = *v;
                f.padding[3] = *v;
                t.frame = Some(f);
            }
            Self::PaddingY(v) => {
                let mut f = frame();
                f.padding[0] = *v;
                f.padding[2] = *v;
                t.frame = Some(f);
            }
            Self::Radius(v) => {
                let mut f = frame();
                f.radius = CornerRadii {
                    tl: *v,
                    tr: *v,
                    br: *v,
                    bl: *v,
                };
                t.frame = Some(f);
            }
            Self::Border(b) => {
                let mut f = frame();
                f.stroke.clone_from(b);
                t.frame = Some(f);
            }
            Self::BorderWidth(w) => {
                let mut f = frame();
                if let Some(b) = &mut f.stroke {
                    b.width = *w;
                }
                t.frame = Some(f);
            }
        }
        // Neither background nor border: no frame at all.
        if t.frame
            .as_ref()
            .is_some_and(|f| f.fill == Paint::None && f.stroke.is_none())
        {
            t.frame = None;
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Family(_) | Self::Style(_) => "Change font",
            Self::Size(_) | Self::LineHeight(_) | Self::Spacing(_) | Self::Align(_) => {
                "Change text style"
            }
            Self::Color(_) => "Change text colour",
            _ => "Change text frame",
        }
    }
}

/// One undo step applying `edit` to every text, each keeping its other
/// values.
fn restyle(texts: &[(NodeId, Text)], edit: &Edit) -> Command {
    let mut commands = Vec::new();
    for (id, before) in texts {
        let mut t = before.clone();
        edit.apply(&mut t);
        let ids = vec![*id];
        if t.style != before.style {
            commands.push(Command::SetStyle {
                ids: ids.clone(),
                style: t.style.clone(),
            });
        }
        if t.frame != before.frame {
            commands.push(Command::SetTextFrame {
                ids: ids.clone(),
                frame: t.frame.clone(),
            });
        }
        commands.push(Command::SetTextStyle {
            ids,
            font: t.font,
            size: t.size,
            line_height: t.line_height,
            letter_spacing: t.letter_spacing,
            align: t.align,
        });
    }
    Command::Batch {
        label: edit.label(),
        commands,
    }
}

#[cfg(test)]
mod tests {
    use vextor_core::{Document, Node};

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
    fn style_names_follow_css_weights() {
        let name = |weight, italic| style_name(FaceStyle { weight, italic });
        assert_eq!(name(400, false), "Regular");
        assert_eq!(name(400, true), "Italic");
        assert_eq!(name(700, false), "Bold");
        assert_eq!(name(300, true), "Light Italic");
        assert_eq!(name(950, false), "Black");
    }

    #[test]
    fn family_list_puts_recent_first_and_filters() {
        let all: Vec<String> = ["Arial", "Helvetica", "Inter", "Menlo"]
            .map(String::from)
            .to_vec();
        let recent = vec!["Menlo".to_owned(), "Gone".to_owned()];
        assert_eq!(
            family_list(&all, &recent, ""),
            ["Menlo", "Arial", "Helvetica", "Inter"]
        );
        assert_eq!(family_list(&all, &recent, "  he"), ["Helvetica"]);
        assert_eq!(family_list(&all, &recent, "N"), ["Menlo", "Inter"]);
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
        step(&mut s, Edit::Background(Some(Rgba::rgb(30, 144, 255))));
        let f = text(&s, ids[0]).frame.clone().expect("framed");
        assert_eq!(f.fill, Paint::Solid(Rgba::rgb(30, 144, 255)));
        assert_eq!(f.padding, [8.0, 16.0, 8.0, 16.0]);
        step(&mut s, Edit::PaddingX(4.0));
        step(&mut s, Edit::Radius(12.0));
        step(&mut s, Edit::Border(Some(Stroke::default())));
        step(&mut s, Edit::BorderWidth(2.0));
        let f = text(&s, ids[0]).frame.clone().expect("framed");
        assert_eq!(f.padding, [8.0, 4.0, 8.0, 4.0]);
        assert!((f.radius.br - 12.0).abs() < 1e-9);
        assert!((f.stroke.expect("border").width - 2.0).abs() < 1e-9);

        step(&mut s, Edit::Background(None));
        assert!(text(&s, ids[0]).frame.is_some(), "the border remains");
        step(&mut s, Edit::Border(None));
        assert_eq!(text(&s, ids[0]).frame, None);
        assert_eq!(s.edit_menu().undo, Some("Change text frame"));
    }

    #[test]
    fn colour_size_and_alignment() {
        let (mut s, ids) = session_with(vec![Text::new("x", "Inter")]);
        for e in [
            Edit::Color(Rgba::rgb(255, 0, 0)),
            Edit::Size(18.0),
            Edit::LineHeight(1.5),
            Edit::Spacing(0.5),
            Edit::Align(TextAlign::Centre),
        ] {
            let texts = selected_texts(&s);
            s.apply(restyle(&texts, &e)).expect("applies");
        }
        let t = text(&s, ids[0]);
        assert_eq!(t.style.fill, Paint::Solid(Rgba::rgb(255, 0, 0)));
        assert!((t.size - 18.0).abs() < 1e-9);
        assert!((t.line_height - 1.5).abs() < 1e-9);
        assert!((t.letter_spacing - 0.5).abs() < 1e-9);
        assert_eq!(t.align, TextAlign::Centre);
    }

    #[test]
    fn recent_families_are_newest_first_and_bounded() {
        let ctx = egui::Context::default();
        for f in ["A", "B", "A", "C"] {
            remember_family(&ctx, f);
        }
        assert_eq!(recent_families(&ctx), ["C", "A", "B"]);
        for i in 0..20 {
            remember_family(&ctx, &format!("F{i}"));
        }
        assert_eq!(recent_families(&ctx).len(), RECENT_LIMIT);
    }
}
