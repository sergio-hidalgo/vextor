//! The tab strip above the canvas (spec 05f): one tab per open document,
//! a close button that shows on hover, `+` for a new document, and
//! renaming a document with a double-click on its tab.

use eframe::egui::{self, Align2, FontId, Rect, Sense, TextStyle, Vec2};

use crate::documents::{DocId, Documents, Tab};
use crate::files::MAX_NAME_CHARS;
use crate::session::Session;
use crate::theme;

/// Space left and right of a tab's name, in points.
const PAD: f32 = 8.0;
/// Side of the close button, in points. Its room is kept whether it shows
/// or not, so a tab never changes width on hover.
const CLOSE: f32 = 16.0;
/// Height of a tab, in points.
const HEIGHT: f32 = 22.0;
/// Width of the rename field, in points.
const RENAME_WIDTH: f32 = 160.0;

/// What the user did in the strip this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TabAction {
    Activate(DocId),
    Close(DocId),
    New,
    /// Rename the document to this text (not yet cleaned).
    Rename(DocId, String),
}

/// The strip's state between frames.
#[derive(Default)]
pub struct Strip {
    rename: Option<Rename>,
}

/// A tab being renamed.
struct Rename {
    id: DocId,
    text: String,
    /// The field has not been shown yet: focus it and select its text.
    fresh: bool,
}

impl Strip {
    /// Draws the strip; the tabs scroll sideways when they do not fit.
    /// Returns every action of this frame, in order: a click on another
    /// tab both finishes a rename and activates that tab.
    pub fn ui(&mut self, ui: &mut egui::Ui, docs: &Documents) -> Vec<TabAction> {
        if let Some(r) = &self.rename
            && docs.get(r.id).is_none()
        {
            self.rename = None;
        }
        let mut actions = Vec::new();
        let active = docs.active().map(|t| t.id);
        egui::ScrollArea::horizontal()
            .id_salt("tabs-scroll")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for tab in docs.tabs() {
                        let renaming = self.rename.as_ref().is_some_and(|r| r.id == tab.id);
                        let a = if renaming {
                            self.rename_ui(ui)
                        } else {
                            self.tab_ui(ui, tab, active == Some(tab.id))
                        };
                        actions.extend(a);
                    }
                    let plus = ui
                        .add(egui::Button::new("+").frame(false))
                        .on_hover_text("New document (⌘N)");
                    if plus.clicked() {
                        actions.push(TabAction::New);
                    }
                });
            });
        actions
    }

    /// One tab: its name (yellow when active), and × while the pointer is
    /// over the tab. Double-click starts a rename.
    fn tab_ui(&mut self, ui: &mut egui::Ui, tab: &Tab, active: bool) -> Option<TabAction> {
        let s = &tab.session;
        let visuals = ui.visuals().clone();
        let color = if active {
            theme::YELLOW_SOFT
        } else {
            visuals.text_color()
        };
        let font = TextStyle::Button.resolve(ui.style());
        let galley =
            ui.painter()
                .layout_no_wrap(label(&s.display_name(), s.is_dirty()), font, color);
        let size = Vec2::new(
            PAD + galley.size().x + PAD / 2.0 + CLOSE + PAD / 2.0,
            HEIGHT,
        );
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let close_rect = Rect::from_center_size(
            egui::pos2(rect.right() - PAD / 2.0 - CLOSE / 2.0, rect.center().y),
            Vec2::splat(CLOSE),
        );
        let close = ui.interact(close_rect, response.id.with("close"), Sense::click());
        let hovered = ui.rect_contains_pointer(rect);

        let painter = ui.painter();
        let fill = if active {
            visuals.widgets.active.weak_bg_fill
        } else if hovered {
            visuals.widgets.hovered.weak_bg_fill
        } else {
            egui::Color32::TRANSPARENT
        };
        painter.rect_filled(rect, 4.0, fill);
        let text_at = egui::pos2(rect.left() + PAD, rect.center().y - galley.size().y / 2.0);
        painter.galley(text_at, galley, color);
        if hovered {
            if close.hovered() {
                painter.rect_filled(close_rect, 3.0, visuals.widgets.hovered.bg_fill);
            }
            painter.text(
                close_rect.center(),
                Align2::CENTER_CENTER,
                "×",
                FontId::proportional(14.0),
                visuals.weak_text_color(),
            );
        }

        if hovered && close.on_hover_text("Close (⌘W)").clicked() {
            return Some(TabAction::Close(tab.id));
        }
        let response = response.on_hover_text(hint(s));
        if response.double_clicked() {
            self.rename = Some(Rename {
                id: tab.id,
                text: editable_name(s),
                fresh: true,
            });
            return None;
        }
        response.clicked().then_some(TabAction::Activate(tab.id))
    }

    /// The rename field in place of the tab. Enter or a click elsewhere
    /// renames; `Esc` cancels.
    fn rename_ui(&mut self, ui: &mut egui::Ui) -> Option<TabAction> {
        let rename = self.rename.as_mut()?;
        let output = egui::TextEdit::singleline(&mut rename.text)
            .char_limit(MAX_NAME_CHARS)
            .desired_width(RENAME_WIDTH)
            .show(ui);
        if rename.fresh {
            rename.fresh = false;
            let mut state = output.state;
            let all = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(rename.text.chars().count()),
            );
            state.cursor.set_char_range(Some(all));
            state.store(ui.ctx(), output.response.id);
            output.response.request_focus();
            return None;
        }
        if output.response.has_focus() {
            return None;
        }
        let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
        let rename = self.rename.take()?;
        (!cancelled).then_some(TabAction::Rename(rename.id, rename.text))
    }
}

/// A tab's text: the document's name, and `•` while it has unsaved
/// changes (as in the window title).
fn label(name: &str, dirty: bool) -> String {
    if dirty {
        format!("{name} •")
    } else {
        name.to_owned()
    }
}

/// A tab's tooltip: where the document is saved.
fn hint(s: &Session) -> String {
    s.path().map_or_else(
        || "Not saved yet — double-click to rename".to_owned(),
        |p| format!("{} — double-click to rename", p.display()),
    )
}

/// The text a rename starts from: the file name without its extension, or
/// the name of a document not saved yet.
fn editable_name(s: &Session) -> String {
    s.path()
        .and_then(|p| p.file_stem())
        .map_or_else(|| s.doc.name.clone(), |n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_doc::new_document;

    #[test]
    fn unsaved_tabs_show_a_dot() {
        assert_eq!(label("card", false), "card");
        assert_eq!(label("card", true), "card •");
    }

    #[test]
    fn a_rename_starts_from_the_name_without_the_extension() {
        let unsaved = Session::new(new_document("card", 1.0, 1.0));
        assert_eq!(editable_name(&unsaved), "Untitled");
        let path = std::env::temp_dir().join("pricing.vxtr");
        let saved = Session::opened(new_document("card", 1.0, 1.0), path);
        assert_eq!(editable_name(&saved), "pricing");
    }

    #[test]
    fn showing_the_strip_changes_nothing() {
        let mut docs = Documents::default();
        docs.add(Session::new(new_document("card", 320.0, 200.0)), None);
        docs.add(Session::new(new_document("icon-24", 24.0, 24.0)), None);
        let ctx = egui::Context::default();
        let mut strip = Strip::default();
        let mut actions = Vec::new();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            actions = strip.ui(ui, &docs);
        });
        out.textures_delta.clear();
        assert_eq!(actions, []);
        assert_eq!(docs.tabs().len(), 2);
    }
}
