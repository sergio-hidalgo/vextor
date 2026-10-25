//! The canvas side of text (spec 05c): keys to the text being edited,
//! pointer input for the Text tool and for editing, and the caret and
//! selection overlay. The editing rules live in `text_tool`.

use eframe::egui::{self, Key, Painter, Pos2, Stroke};
use vextor_core::{NodeId, NodeKind, ToolId};

use super::{Canvas, to_egui};
use crate::select::PICK_TOLERANCE;
use crate::session::Session;
use crate::text_edit::EditKey;
use crate::{theme, view};

/// Family of a new text while no font has been picked yet and the system
/// fonts have not loaded.
const DEFAULT_FAMILY: &str = "Helvetica";

impl Canvas {
    /// True while a text object is being edited (the keyboard types).
    #[must_use]
    pub fn is_editing_text(&self) -> bool {
        self.text.editing().is_some()
    }

    /// Ends the text edit in progress, if any (one undo step).
    pub fn finish_text(&mut self, session: &mut Session) {
        self.text.finish(session);
    }

    /// While editing, takes the typing keys from egui's input and applies
    /// them; otherwise `Enter` with one text selected starts editing it.
    pub(super) fn text_keys(&mut self, ui: &egui::Ui, session: &mut Session) -> Option<String> {
        if ui.ctx().egui_wants_keyboard_input() {
            return None;
        }
        if self.text.editing().is_none() {
            let selected = session.selected();
            let one_text = match selected.as_slice() {
                [id] => is_text(session, *id).then_some(*id),
                _ => None,
            };
            if let Some(id) = one_text
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter))
            {
                self.text.start(session, id, false);
            }
            return None;
        }
        let keys = ui.input_mut(|i| {
            let mut keys = Vec::new();
            i.events.retain(|e| match edit_key(e) {
                Some(k) => {
                    keys.push(k);
                    false
                }
                None => true,
            });
            keys
        });
        let mut message = None;
        for k in keys {
            match self.text.key(session, k) {
                Ok(Some(text)) => ui.ctx().copy_text(text),
                Ok(None) => {}
                Err(e) => message = Some(e),
            }
        }
        if self.text.editing().is_some() {
            // The caret is drawn every frame; keep frames coming while typing.
            ui.ctx().request_repaint();
        }
        message
    }

    /// A press at `p` (document space) for text: inside the text being
    /// edited it moves the caret; with the Text tool it edits the text
    /// under the pointer or places a new one. Returns true when the press
    /// was used, and a message when placing failed.
    pub(super) fn text_press(
        &mut self,
        ctx: &egui::Context,
        session: &mut Session,
        p: kurbo::Point,
        shift: bool,
    ) -> (bool, Option<String>) {
        if self.text.editing().is_some() && self.text.press(session, p, shift) {
            return (true, None);
        }
        if session.editor.tool != ToolId::Text {
            return (false, None);
        }
        let zoom = session.editor.view.zoom;
        if let Some(id) = session
            .hit_index()
            .pick(p, PICK_TOLERANCE / zoom)
            .filter(|&id| is_text(session, id))
        {
            self.text.start_at(session, id, p);
            return (true, None);
        }
        let family = new_text_family(ctx, session);
        let message = self.text.place(session, p, &family).err();
        (true, message)
    }

    /// The pointer moved with the button down, or was released.
    pub(super) fn text_drag(&mut self, session: &Session, at: Option<kurbo::Point>, down: bool) {
        match at {
            Some(p) if down => self.text.drag_to(session, p),
            _ => self.text.release(),
        }
    }

    /// A double-click at `p` with the selection tool: edits the text there.
    pub(super) fn text_double_click(&mut self, session: &mut Session, p: kurbo::Point) {
        let zoom = session.editor.view.zoom;
        if let Some(id) = session
            .hit_index()
            .pick(p, PICK_TOLERANCE / zoom)
            .filter(|&id| is_text(session, id))
        {
            self.text.start_at(session, id, p);
        }
    }

    /// The edited text's box, its selection and the caret.
    pub(super) fn draw_text_overlay(&self, painter: &Painter, origin: Pos2, session: &Session) {
        let Some(o) = self.text.overlay(session) else {
            return;
        };
        let view = session.editor.view;
        let screen = |p: kurbo::Point| {
            origin + to_egui(view::doc_to_screen(view, vextor_core::Point::new(p.x, p.y)))
        };
        let thin = Stroke::new(1.0, theme::YELLOW_SOFT);
        let mut frame: Vec<Pos2> = o.frame.iter().map(|&p| screen(p)).collect();
        frame.push(frame[0]);
        painter.add(egui::Shape::line(frame, thin));
        for quad in &o.selection {
            painter.add(egui::Shape::convex_polygon(
                quad.iter().map(|&p| screen(p)).collect(),
                theme::YELLOW.gamma_multiply(0.3),
                Stroke::NONE,
            ));
        }
        painter.line_segment(
            [screen(o.caret.0), screen(o.caret.1)],
            Stroke::new(1.5, theme::YELLOW),
        );
    }
}

fn is_text(session: &Session, id: NodeId) -> bool {
    matches!(
        session.doc.nodes.get(&id).map(|n| &n.kind),
        Some(NodeKind::Text(_))
    )
}

/// The family a new text starts in: the last one picked in the inspector,
/// else the system's sans-serif fallback.
fn new_text_family(ctx: &egui::Context, session: &Session) -> String {
    crate::panels::recent_families(ctx)
        .into_iter()
        .next()
        .or_else(|| session.fonts.db().fallback_family().map(str::to_owned))
        .unwrap_or_else(|| DEFAULT_FAMILY.to_owned())
}

/// The editing key an egui event means, if it means one.
pub(crate) fn edit_key(event: &egui::Event) -> Option<EditKey> {
    match event {
        egui::Event::Text(t) | egui::Event::Paste(t) => Some(EditKey::Insert(t.clone())),
        egui::Event::Copy => Some(EditKey::Copy),
        egui::Event::Cut => Some(EditKey::Cut),
        egui::Event::Key {
            key,
            pressed: true,
            modifiers: m,
            ..
        } => {
            let extend = m.shift;
            Some(match key {
                Key::Enter if !m.command => EditKey::NewLine,
                Key::Backspace => EditKey::Backspace,
                Key::Delete => EditKey::Delete,
                Key::Escape => EditKey::Finish,
                Key::A if m.command => EditKey::SelectAll,
                Key::ArrowLeft => EditKey::Left {
                    word: m.alt,
                    line: m.command,
                    extend,
                },
                Key::ArrowRight => EditKey::Right {
                    word: m.alt,
                    line: m.command,
                    extend,
                },
                Key::Home => EditKey::Left {
                    word: false,
                    line: true,
                    extend,
                },
                Key::End => EditKey::Right {
                    word: false,
                    line: true,
                    extend,
                },
                Key::ArrowUp => EditKey::Up { extend },
                Key::ArrowDown => EditKey::Down { extend },
                _ => return None,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use egui::Modifiers;

    use super::*;

    fn key(key: Key, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn typing_events_become_edit_keys() {
        assert_eq!(
            edit_key(&egui::Event::Text("é".into())),
            Some(EditKey::Insert("é".into()))
        );
        assert_eq!(
            edit_key(&egui::Event::Paste("a\nb".into())),
            Some(EditKey::Insert("a\nb".into()))
        );
        assert_eq!(edit_key(&egui::Event::Copy), Some(EditKey::Copy));
        assert_eq!(edit_key(&egui::Event::Cut), Some(EditKey::Cut));
        assert_eq!(
            edit_key(&key(Key::Enter, Modifiers::NONE)),
            Some(EditKey::NewLine)
        );
        assert_eq!(
            edit_key(&key(Key::Escape, Modifiers::NONE)),
            Some(EditKey::Finish)
        );
        assert_eq!(
            edit_key(&key(Key::A, Modifiers::COMMAND)),
            Some(EditKey::SelectAll)
        );
    }

    #[test]
    fn arrow_modifiers_choose_word_line_and_extend() {
        assert_eq!(
            edit_key(&key(Key::ArrowLeft, Modifiers::ALT)),
            Some(EditKey::Left {
                word: true,
                line: false,
                extend: false
            })
        );
        assert_eq!(
            edit_key(&key(
                Key::ArrowRight,
                Modifiers::COMMAND.plus(Modifiers::SHIFT)
            )),
            Some(EditKey::Right {
                word: false,
                line: true,
                extend: true
            })
        );
        assert_eq!(
            edit_key(&key(Key::ArrowDown, Modifiers::SHIFT)),
            Some(EditKey::Down { extend: true })
        );
    }

    #[test]
    fn other_keys_are_left_alone() {
        // ⌘Z and ⌘S still reach the app (it finishes the edit first).
        assert_eq!(edit_key(&key(Key::Z, Modifiers::COMMAND)), None);
        assert_eq!(edit_key(&key(Key::S, Modifiers::COMMAND)), None);
        assert_eq!(
            edit_key(&key(Key::A, Modifiers::NONE)),
            None,
            "arrives as Text"
        );
        let released = egui::Event::Key {
            key: Key::Backspace,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        assert_eq!(edit_key(&released), None);
    }
}
