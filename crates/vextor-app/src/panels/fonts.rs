//! Picking a font in the Text section (spec 05c): the searchable family
//! list, recently used families first and each name in its own face, and
//! the family's styles by CSS weight name.

use eframe::egui::{self, RichText};
use vextor_core::FontSpec;
use vextor_geom::{FaceStyle, FontDb};

/// Most families remembered as recently used.
const RECENT_LIMIT: usize = 8;
/// Rows of the family list shown at once.
const FAMILY_ROWS: f32 = 12.0;
/// Size the family names are shown at in the list.
const FAMILY_PREVIEW_SIZE: f32 = 15.0;
/// Largest font file loaded into the UI to show a family in its own face;
/// bigger ones (CJK faces run to 20 MB+) show in the UI font, so scrolling
/// the list cannot pile up hundreds of MB.
const MAX_PREVIEW_BYTES: usize = 4 * 1024 * 1024;
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

/// The family list: a search field, recently used first, each family's
/// name in its own face once that face is loaded (only rows on screen
/// load theirs). Returns the family picked.
pub(super) fn family_picker(ui: &mut egui::Ui, db: &FontDb, current: &str) -> Option<String> {
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
    if let Some((data, index)) = db
        .sample_face(family)
        .filter(|(data, _)| data.len() <= MAX_PREVIEW_BYTES)
    {
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
pub(super) fn style_picker(ui: &mut egui::Ui, db: &FontDb, font: &FontSpec) -> Option<FaceStyle> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
