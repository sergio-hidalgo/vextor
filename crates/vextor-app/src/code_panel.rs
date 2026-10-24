//! The code panel (spec 12, bottom): the live code of the current artboard
//! or of the selection as SVG, inline HTML, JSX or a CSS data URI, with
//! the selected objects' elements highlighted, the export options and a
//! Copy button.
//!
//! The code is written again only when the document, the format or what
//! is shown changes; a new selection only re-colours it.

mod highlight;
mod options;

use std::ops::Range;

use eframe::egui::{self, text::LayoutJob};
use vextor_core::{ArtboardId, NodeId};
use vextor_svg::{Export, Format, Scope};

pub use self::options::{Checks, options_ui};
use crate::documents::DocId;
use crate::export::current_artboard;
use crate::panels::panel_title;
use crate::session::Session;

/// What the panel shows code of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shows {
    /// The current artboard (`export::current_artboard`).
    #[default]
    Artboard,
    Selection,
}

/// The panel's choices and the code it last wrote.
#[derive(Default)]
pub struct CodePanel {
    format: Format,
    shows: Shows,
    code: Option<Code>,
    /// The selection the code was last scrolled to.
    scrolled: Vec<NodeId>,
    /// The Options popup is open.
    options_open: bool,
}

/// What a piece of code was written from.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    doc: DocId,
    revision: u64,
    format: Format,
    shows: Shows,
    artboard: Option<ArtboardId>,
    /// Only when the selection is shown.
    selection: Vec<NodeId>,
}

/// Written code and its coloured layout.
struct Code {
    key: Key,
    /// `None`: nothing to show (no artboard, nothing selected).
    export: Option<Export>,
    /// The selection `job` highlights.
    highlighted: Vec<NodeId>,
    job: LayoutJob,
}

impl CodePanel {
    /// Draws the panel for the active document, if one shows.
    pub fn ui(&mut self, ui: &mut egui::Ui, doc: Option<(DocId, &mut Session)>) {
        ui.add_space(4.0);
        let Some((id, s)) = doc else {
            ui.horizontal(|ui| panel_title(ui, "Code"));
            ui.weak("Open or create a document to see its code.");
            return;
        };
        self.toolbar(ui, s);
        ui.separator();
        self.update(ui, id, s);
        let Some(code) = &self.code else {
            return;
        };
        if code.export.is_none() {
            ui.weak(match code.key.shows {
                Shows::Artboard => "Add an artboard (Object ▸ Add Artboard…) to see its code.",
                Shows::Selection if code.key.selection.is_empty() => {
                    "Select objects to see their code."
                }
                Shows::Selection => "The selection paints nothing.",
            });
            return;
        }
        let selected = s.selected();
        let scroll_to = (selected != self.scrolled)
            .then(|| first_span(code, &selected))
            .flatten();
        self.scrolled = selected;
        egui::ScrollArea::both()
            .id_salt(("code", id))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let galley = ui.ctx().fonts_mut(|f| f.layout_job(code.job.clone()));
                let response = ui.add(egui::Label::new(galley.clone()).selectable(true));
                if let Some(at) = scroll_to {
                    let text = &galley.job.text;
                    let chars = text[..at.min(text.len())].chars().count();
                    let rect = galley
                        .pos_from_cursor(egui::text::CCursor::new(chars))
                        .translate(response.rect.min.to_vec2());
                    ui.scroll_to_rect(rect, Some(egui::Align::Center));
                }
            });
    }

    /// Title, format, what is shown, Options and Copy.
    fn toolbar(&mut self, ui: &mut egui::Ui, s: &mut Session) {
        ui.horizontal(|ui| {
            panel_title(ui, "Code");
            ui.add_space(8.0);
            egui::ComboBox::from_id_salt("code_format")
                .selected_text(self.format.title())
                .show_ui(ui, |ui| {
                    for format in Format::ALL {
                        ui.selectable_value(&mut self.format, format, format.title());
                    }
                });
            let artboard = current_artboard(s)
                .and_then(|id| s.doc.artboards.iter().find(|a| a.id == id))
                .map_or_else(|| "No artboard".to_owned(), |a| a.name.clone());
            ui.selectable_value(&mut self.shows, Shows::Artboard, "Artboard")
                .on_hover_text(artboard);
            ui.selectable_value(&mut self.shows, Shows::Selection, "Selection");
            let _ = options_menu(ui, s, &mut self.options_open);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let code = self.code.as_ref().and_then(|c| c.export.as_ref());
                let copy = ui
                    .add_enabled(code.is_some(), egui::Button::new("Copy"))
                    .on_hover_text(format!("Copy the {}", self.format.title()))
                    .on_disabled_hover_text("Nothing to copy");
                if copy.clicked()
                    && let Some(e) = code
                {
                    ui.ctx().copy_text(e.code.clone());
                }
            });
        });
    }

    /// Writes the code again when what it shows changed, and re-colours
    /// it when the selection changed.
    fn update(&mut self, ui: &egui::Ui, doc: DocId, s: &Session) {
        let selected = s.selected();
        let key = Key {
            doc,
            revision: s.revision(),
            format: self.format,
            shows: self.shows,
            artboard: current_artboard(s),
            selection: match self.shows {
                Shows::Artboard => Vec::new(),
                Shows::Selection => selected.clone(),
            },
        };
        let stale = self.code.as_ref().is_none_or(|c| c.key != key);
        if !stale
            && self
                .code
                .as_ref()
                .is_some_and(|c| c.highlighted == selected)
        {
            return;
        }
        let export = if stale {
            write(s, &key)
        } else {
            self.code.take().and_then(|c| c.export)
        };
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let job = export.as_ref().map_or_else(LayoutJob::default, |e| {
            highlight::layout_job(&e.code, &font, &marked(e, &selected))
        });
        self.code = Some(Code {
            key,
            export,
            highlighted: selected,
            job,
        });
    }
}

/// Space between the Options button and its popup, above or below it.
const OPTIONS_GAP: f32 = 6.0;

/// The Options button and its popup (`open` holds whether it shows). The
/// popup stays open while options change, including over its drop-down
/// lists; a click anywhere else, or `Esc`, closes it.
///
/// Its open state is kept here, not in egui's popup memory: that memory
/// holds one open popup at a time, so a drop-down opening would close the
/// Options popup (or never open). egui's menus also count a click on a
/// drop-down's list as outside and close; this popup ignores egui's click
/// handling and closes on a click outside every popup instead. Returns the
/// button and the popup's id.
fn options_menu(ui: &mut egui::Ui, s: &mut Session, open: &mut bool) -> (egui::Response, egui::Id) {
    let button = ui.button("Options");
    if button.clicked() {
        *open = !*open;
    }
    let popup = egui::Popup::menu(&button)
        .open_bool(open)
        .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
        .gap(OPTIONS_GAP);
    let id = popup.get_id();
    let shown = popup.show(|ui| {
        if let Some(o) = options_ui(ui, &s.doc.export, Checks::Left) {
            s.set_export(o);
        }
    });
    if shown.is_some() && clicked_outside_popups(ui.ctx(), button.rect) {
        *open = false;
    }
    (button, id)
}

/// A click this frame that is neither on `button` nor on any popup.
fn clicked_outside_popups(ctx: &egui::Context, button: egui::Rect) -> bool {
    let click = ctx.input(|i| {
        i.pointer
            .any_click()
            .then(|| i.pointer.interact_pos())
            .flatten()
    });
    click.is_some_and(|pos| {
        !button.contains(pos)
            && ctx
                .layer_id_at(pos)
                .is_none_or(|layer| layer.order != egui::Order::Foreground)
    })
}

/// The code `key` asks for.
fn write(s: &Session, key: &Key) -> Option<Export> {
    let scope = match key.shows {
        Shows::Artboard => Scope::Artboard(key.artboard?),
        Shows::Selection if key.selection.is_empty() => return None,
        Shows::Selection => Scope::Selection(&key.selection),
    };
    vextor_svg::export(&s.doc, &s.fonts, scope, key.format, &s.doc.export)
}

/// The spans of the selected nodes' elements.
fn marked(export: &Export, selected: &[NodeId]) -> Vec<Range<usize>> {
    export
        .spans
        .iter()
        .filter(|(id, _)| selected.contains(id))
        .map(|(_, span)| span.clone())
        .collect()
}

/// Where the first selected element starts, if it is in the code.
fn first_span(code: &Code, selected: &[NodeId]) -> Option<usize> {
    let export = code.export.as_ref()?;
    marked(export, selected).iter().map(|r| r.start).min()
}

#[cfg(test)]
mod tests {
    use vextor_core::{Command, Geometry, Node, NodeKind, Rect, Shape, Style};

    use super::*;
    use crate::documents::Documents;
    use crate::new_doc::new_document;

    /// One open document with a 20 px square on a 24 px artboard.
    fn docs() -> (Documents, NodeId) {
        let mut doc = new_document("icon", 24.0, 24.0);
        let layer = doc.layers[0];
        let id = doc.alloc_node_id();
        let shape = Shape {
            geometry: Geometry::Rect {
                rect: Rect::new(2.0, 2.0, 20.0, 20.0),
                radius: vextor_core::CornerRadii::default(),
            },
            style: Style::default(),
        };
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![Node::new(id, NodeKind::Shape(shape))],
        })
        .expect("inserts");
        let mut docs = Documents::default();
        docs.add(Session::new(doc), None);
        (docs, id)
    }

    fn session(docs: &mut Documents) -> &mut Session {
        &mut docs.active_mut().expect("open").session
    }

    /// Shows the panel once, headless, for the active document.
    fn show(panel: &mut CodePanel, docs: &mut Documents) {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let tab = docs.active_mut().expect("open");
            panel.ui(ui, Some((tab.id, &mut tab.session)));
        });
        out.textures_delta.clear();
    }

    fn code(panel: &CodePanel) -> Option<&str> {
        let export = panel.code.as_ref()?.export.as_ref()?;
        Some(&export.code)
    }

    #[test]
    fn the_panel_shows_the_artboard_and_never_edits_the_document() {
        let (mut docs, _) = docs();
        let before = (
            session(&mut docs).doc.clone(),
            session(&mut docs).revision(),
        );
        let mut panel = CodePanel::default();
        show(&mut panel, &mut docs);
        let s = session(&mut docs);
        assert_eq!((s.doc.clone(), s.revision()), before);
        let svg = code(&panel).expect("code");
        assert!(
            svg.contains(r#"<rect x="2" y="2" width="20" height="20"/>"#),
            "{svg}"
        );
    }

    #[test]
    fn selecting_recolours_without_writing_again_and_edits_write_again() {
        let (mut docs, id) = docs();
        let mut panel = CodePanel::default();
        show(&mut panel, &mut docs);
        let key = panel.code.as_ref().map(|c| c.key.clone());
        session(&mut docs).select(vec![id]);
        show(&mut panel, &mut docs);
        let c = panel.code.as_ref().expect("code");
        assert_eq!(Some(&c.key), key.as_ref(), "same code");
        assert_eq!(c.highlighted, vec![id]);
        let lit =
            |sec: &egui::text::LayoutSection| sec.format.background != egui::Color32::TRANSPARENT;
        assert!(c.job.sections.iter().any(lit));
        let s = session(&mut docs);
        let mut o = s.doc.export.clone();
        o.precision = 4;
        assert!(s.set_export(o));
        show(&mut panel, &mut docs);
        assert_ne!(
            panel.code.as_ref().map(|c| c.key.clone()),
            key,
            "written again"
        );
    }

    #[test]
    fn the_selection_shows_only_once_something_is_selected() {
        let (mut docs, id) = docs();
        let mut panel = CodePanel {
            shows: Shows::Selection,
            format: Format::Jsx,
            ..CodePanel::default()
        };
        show(&mut panel, &mut docs);
        assert!(panel.code.as_ref().is_some_and(|c| c.export.is_none()));
        session(&mut docs).select(vec![id]);
        show(&mut panel, &mut docs);
        let jsx = code(&panel).expect("code");
        assert!(jsx.starts_with("export function Untitled(props)"), "{jsx}");
    }

    /// The Options button and popup, headless, with its open state.
    struct Options {
        ctx: egui::Context,
        open: bool,
        button: egui::Rect,
        id: egui::Id,
    }

    impl Options {
        fn new(s: &mut Session) -> Self {
            let mut o = Self {
                ctx: egui::Context::default(),
                open: false,
                button: egui::Rect::NOTHING,
                id: egui::Id::NULL,
            };
            o.frame(s, Vec::new());
            o
        }

        fn frame(&mut self, s: &mut Session, events: Vec<egui::Event>) {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let (mut button, mut id) = (self.button, self.id);
            let open = &mut self.open;
            let mut out = self.ctx.run_ui(input, |ui| {
                (button, id) = {
                    let (b, id) = options_menu(ui, s, open);
                    (b.rect, id)
                };
            });
            out.textures_delta.clear();
            (self.button, self.id) = (button, id);
        }

        /// A left click at `pos`, over four frames.
        fn click(&mut self, s: &mut Session, pos: egui::Pos2) {
            let press = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let frames = [
                vec![egui::Event::PointerMoved(pos)],
                vec![press(true)],
                vec![press(false)],
                vec![],
            ];
            for events in frames {
                self.frame(s, events);
            }
        }

        fn popup(&self) -> egui::Rect {
            self.ctx.memory(|m| m.area_rect(self.id)).expect("shown")
        }
    }

    #[test]
    fn options_stay_open_for_clicks_inside_and_close_for_clicks_outside() {
        let (mut docs, _) = docs();
        let s = session(&mut docs);
        let mut o = Options::new(s);
        o.click(s, o.button.center());
        assert!(o.open, "the button opens it");
        o.click(s, o.popup().left_top() + egui::vec2(4.0, 4.0));
        assert!(o.open, "a click inside keeps it");
        o.click(s, egui::pos2(790.0, 590.0));
        assert!(!o.open, "a click outside closes it");
        o.click(s, o.button.center());
        o.click(s, o.button.center());
        assert!(!o.open, "the button closes it again");
    }

    #[test]
    fn a_drop_down_opens_and_picks_inside_the_options_popup() {
        let (mut docs, _) = docs();
        let s = session(&mut docs);
        let mut o = Options::new(s);
        o.click(s, o.button.center());
        let popup = o.popup();
        // Find the Ids drop-down: the first click in the right-hand column
        // that opens a list.
        let x = popup.left() + popup.width() * 0.7;
        let mut y = popup.top() + 4.0;
        while !egui::Popup::is_any_open(&o.ctx) {
            assert!(y < popup.bottom(), "no drop-down found");
            o.click(s, egui::pos2(x, y));
            y += 4.0;
        }
        assert!(o.open, "opening the list keeps Options open");
        // Pick another entry from the list below the drop-down, opening the
        // list again whenever a click closed it.
        let combo = egui::pos2(x, y - 4.0);
        let before = s.doc.export.ids;
        let mut at = combo.y + 8.0;
        while s.doc.export.ids == before {
            assert!(at < 600.0, "no other entry found");
            if !egui::Popup::is_any_open(&o.ctx) {
                o.click(s, combo);
            }
            o.click(s, egui::pos2(x, at));
            at += 4.0;
        }
        assert!(o.open, "picking an entry keeps Options open");
    }

    #[test]
    fn the_options_popup_keeps_a_gap_from_its_button() {
        let (mut docs, _) = docs();
        let s = session(&mut docs);
        let mut o = Options::new(s);
        o.click(s, o.button.center());
        let (button, popup) = (o.button, o.popup());
        let gap = (popup.top() - button.bottom()).max(button.top() - popup.bottom());
        assert!(gap >= OPTIONS_GAP - 0.5, "gap {gap}");
    }
}
