//! Placing and editing text on the canvas (spec 05c), as a state machine
//! over a [`Session`] fed with document-space points and [`EditKey`]s, so
//! it is tested without a window.
//!
//! Typing applies `SetText` inside one history gesture: the canvas shows
//! every keystroke and a finished edit is one undo step. A text left empty
//! when editing finishes is removed: a new one as if it was never placed,
//! an existing one by a "Delete" step that undo turns back into the
//! original text.

use kurbo::{Affine, Point};
use vextor_core::{Command, NodeId, NodeKind, Text, ToolId, Transform};
use vextor_geom::{TextLayout, text_box, to_affine};

use crate::edit_ops;
use crate::session::Session;
use crate::text_edit::{EditKey, Outcome, TextEditor};

/// What a new text says before the first key replaces it.
pub const PLACEHOLDER: &str = "Text";

/// Font size of a new text, in px.
pub const NEW_TEXT_SIZE: f64 = 16.0;

/// One text being edited.
#[derive(Debug)]
struct Editing {
    id: NodeId,
    editor: TextEditor,
    /// Placed by this edit (removed again when it ends empty).
    created: bool,
    /// Some key changed the content (there is an edit step to undo).
    changed: bool,
    /// The pointer is down selecting characters.
    selecting: bool,
}

/// The text editing state between events.
#[derive(Debug, Default)]
pub struct TextTool {
    editing: Option<Editing>,
}

/// Where to draw the caret and the selection, in document space.
#[derive(Clone, Debug, PartialEq)]
pub struct Overlay {
    /// The text's box (layout + frame padding) as four corners.
    pub frame: [Point; 4],
    /// Top and bottom of the caret line.
    pub caret: (Point, Point),
    /// One quad (four corners) per selected line part.
    pub selection: Vec<[Point; 4]>,
}

impl TextTool {
    /// The text being edited, if any.
    #[must_use]
    pub fn editing(&self) -> Option<NodeId> {
        self.editing.as_ref().map(|e| e.id)
    }

    /// Places a new text with its top-left at `at` (document space) in
    /// `family`, selects it and starts editing with the placeholder
    /// selected. One "Insert" undo step.
    ///
    /// # Errors
    /// A message for the user when there is no unlocked layer to put it in.
    pub fn place(&mut self, s: &mut Session, at: Point, family: &str) -> Result<(), String> {
        self.finish(s);
        let Some((parent, index)) = edit_ops::paste_target(s) else {
            return Err("Add text failed: every layer is hidden or locked".to_owned());
        };
        let id = s.doc.alloc_node_id();
        let mut node = vextor_core::Node::new(
            id,
            NodeKind::Text(Text {
                size: NEW_TEXT_SIZE,
                ..Text::new(PLACEHOLDER, family)
            }),
        );
        let place = Transform([1.0, 0.0, 0.0, 1.0, at.x, at.y]);
        let to_parent = s
            .doc
            .world_transform(parent)
            .invert()
            .unwrap_or(Transform::IDENTITY);
        node.transform = place.then(to_parent);
        s.apply(Command::Insert {
            parent: Some(parent),
            index,
            nodes: vec![node],
        })
        .map_err(|e| format!("Add text failed: {e}"))?;
        s.select(vec![id]);
        self.start(s, id, true);
        Ok(())
    }

    /// Starts editing text `id` with everything selected (`Enter`, or a
    /// new text). Does nothing when `id` is not a text.
    pub fn start(&mut self, s: &mut Session, id: NodeId, created: bool) {
        if self.editing() == Some(id) {
            return;
        }
        self.finish(s);
        let Some(NodeKind::Text(t)) = s.doc.nodes.get(&id).map(|n| &n.kind) else {
            return;
        };
        let editor = TextEditor::new(&t.content);
        s.select(vec![id]);
        s.begin_gesture();
        self.editing = Some(Editing {
            id,
            editor,
            created,
            changed: false,
            selecting: false,
        });
    }

    /// Starts editing text `id` with the caret at `p` (a double-click).
    pub fn start_at(&mut self, s: &mut Session, id: NodeId, p: Point) {
        self.start(s, id, false);
        self.press(s, p, false);
        if let Some(e) = &mut self.editing {
            e.selecting = false;
        }
    }

    /// Applies one key. Returns text for the system clipboard, if any.
    ///
    /// # Errors
    /// A message for the user when the change is refused.
    pub fn key(&mut self, s: &mut Session, key: EditKey) -> Result<Option<String>, String> {
        let Some(e) = &mut self.editing else {
            return Ok(None);
        };
        let layout = layout_of(s, e.id);
        let (changed, clipboard) = match e.editor.key(key, layout.as_deref()) {
            Outcome::Continue { changed } => (changed, None),
            Outcome::Clipboard { text, changed } => (changed, Some(text)),
            Outcome::Finish => {
                self.finish(s);
                return Ok(None);
            }
        };
        if changed {
            e.changed = true;
            let cmd = Command::SetText {
                id: e.id,
                content: e.editor.content().to_owned(),
            };
            s.apply(cmd)
                .map_err(|err| format!("Edit text failed: {err}"))?;
        }
        Ok(clipboard)
    }

    /// The pointer pressed at `p` while editing. Inside the text it places
    /// the caret (`extend`: Shift grows the selection) and returns true;
    /// outside it finishes the edit and returns false, so the press goes
    /// on to the active tool.
    pub fn press(&mut self, s: &mut Session, p: Point, extend: bool) -> bool {
        let Some(e) = &mut self.editing else {
            return false;
        };
        if let Some(index) = local_index(s, e.id, p) {
            e.editor.place(index, extend);
            e.selecting = true;
            true
        } else {
            self.finish(s);
            false
        }
    }

    /// The pointer moved to `p` with the button held: the selection follows.
    pub fn drag_to(&mut self, s: &Session, p: Point) {
        let Some(e) = self.editing.as_mut().filter(|e| e.selecting) else {
            return;
        };
        let Some(layout) = layout_of(s, e.id) else {
            return;
        };
        let local = to_local(s, e.id) * p;
        e.editor.place(layout.index_at(local), true);
    }

    /// The pointer button was released.
    pub fn release(&mut self) {
        if let Some(e) = &mut self.editing {
            e.selecting = false;
        }
    }

    /// Ends the edit: closes the undo step, removes a text left empty, and
    /// returns to the selection tool with the text selected.
    pub fn finish(&mut self, s: &mut Session) {
        let Some(e) = self.editing.take() else {
            return;
        };
        s.end_gesture();
        s.editor.tool = ToolId::Select;
        if !e.editor.content().is_empty() || !s.doc.nodes.contains_key(&e.id) {
            return;
        }
        if e.changed {
            s.undo();
        }
        if e.created {
            s.undo();
            s.select(Vec::new());
        } else {
            // Undo shows the original text again in one step.
            let _ = s.apply(Command::Delete { ids: vec![e.id] });
        }
    }

    /// The caret, selection and box of the text being edited.
    #[must_use]
    pub fn overlay(&self, s: &Session) -> Option<Overlay> {
        let e = self.editing.as_ref()?;
        let NodeKind::Text(t) = &s.doc.nodes.get(&e.id)?.kind else {
            return None;
        };
        let layout = s.fonts.layout(e.id, t);
        let world = to_affine(s.doc.world_transform(e.id));
        let quad = |r: kurbo::Rect| {
            [
                world * Point::new(r.x0, r.y0),
                world * Point::new(r.x1, r.y0),
                world * Point::new(r.x1, r.y1),
                world * Point::new(r.x0, r.y1),
            ]
        };
        let (line, x) = layout.caret(e.editor.caret());
        let lb = &layout.lines[line];
        let caret = (
            world * Point::new(x, lb.rect.y0),
            world * Point::new(x, lb.rect.y1),
        );
        let sel = e.editor.selection();
        let selection = if sel.is_empty() {
            Vec::new()
        } else {
            layout
                .lines
                .iter()
                .filter(|l| sel.start <= l.range.end && sel.end >= l.range.start)
                .map(|l| {
                    let x_at = |i: usize| layout.caret(i.clamp(l.range.start, l.range.end)).1;
                    let x0 = x_at(sel.start.max(l.range.start));
                    // A selection running past the line end shows its break.
                    let past = sel.end > l.range.end;
                    let x1 =
                        x_at(sel.end.min(l.range.end)) + if past { t.size * 0.25 } else { 0.0 };
                    quad(kurbo::Rect::new(x0, l.rect.y0, x1, l.rect.y1))
                })
                .collect()
        };
        Some(Overlay {
            frame: quad(text_box(t, &layout)),
            caret,
            selection,
        })
    }
}

/// The layout of text `id`, if it is one.
fn layout_of(s: &Session, id: NodeId) -> Option<std::sync::Arc<TextLayout>> {
    match &s.doc.nodes.get(&id)?.kind {
        NodeKind::Text(t) => Some(s.fonts.layout(id, t)),
        _ => None,
    }
}

/// Document space → the text's own space.
fn to_local(s: &Session, id: NodeId) -> Affine {
    to_affine(s.doc.world_transform(id)).inverse()
}

/// The character index under `p` (document space) when `p` is inside the
/// text's box.
fn local_index(s: &Session, id: NodeId, p: Point) -> Option<usize> {
    let NodeKind::Text(t) = &s.doc.nodes.get(&id)?.kind else {
        return None;
    };
    let layout = s.fonts.layout(id, t);
    let local = to_local(s, id) * p;
    text_box(t, &layout)
        .contains(local)
        .then(|| layout.index_at(local))
}

#[cfg(test)]
mod tests {
    use vextor_core::Document;

    use super::*;
    use crate::new_doc::new_document;

    fn session() -> Session {
        Session::new(new_document("a", 400.0, 200.0))
    }

    fn text_of(s: &Session, id: NodeId) -> String {
        match &s.doc.nodes[&id].kind {
            NodeKind::Text(t) => t.content.clone(),
            _ => panic!("text"),
        }
    }

    fn typed(t: &mut TextTool, s: &mut Session, text: &str) {
        for c in text.chars() {
            t.key(s, EditKey::Insert(c.to_string())).expect("types");
        }
    }

    fn placed(t: &mut TextTool, s: &mut Session) -> NodeId {
        t.place(s, Point::new(50.0, 40.0), "Inter").expect("places");
        t.editing().expect("editing")
    }

    #[test]
    fn placing_types_over_the_placeholder_in_one_undo_step_after_insert() {
        let mut s = session();
        let mut t = TextTool::default();
        let id = placed(&mut t, &mut s);
        assert_eq!(text_of(&s, id), PLACEHOLDER);
        assert_eq!(s.selected(), vec![id]);
        let world = s.doc.world_transform(id);
        assert_eq!((world.0[4], world.0[5]), (50.0, 40.0));

        typed(&mut t, &mut s, "Buy now");
        assert_eq!(text_of(&s, id), "Buy now", "the canvas shows each key");
        t.key(&mut s, EditKey::Finish).expect("finishes");
        assert_eq!(t.editing(), None);
        assert_eq!(s.editor.tool, ToolId::Select);

        assert_eq!(s.edit_menu().undo, Some("Edit text"));
        assert!(s.undo());
        assert_eq!(text_of(&s, id), PLACEHOLDER);
        assert_eq!(s.edit_menu().undo, Some("Insert"));
        assert!(s.undo());
        assert!(!s.doc.nodes.contains_key(&id));
    }

    #[test]
    fn a_new_text_left_empty_disappears() {
        let mut s = session();
        let mut t = TextTool::default();
        let id = placed(&mut t, &mut s);
        t.key(&mut s, EditKey::Backspace).expect("erases");
        t.finish(&mut s);
        assert!(!s.doc.nodes.contains_key(&id));
        assert_eq!(s.edit_menu().undo, None, "as if never placed");
        assert_eq!(s.selected(), vec![]);
    }

    #[test]
    fn an_existing_text_emptied_is_deleted_in_one_step() {
        let mut s = session();
        let mut t = TextTool::default();
        let id = placed(&mut t, &mut s);
        typed(&mut t, &mut s, "Label");
        t.finish(&mut s);

        t.start(&mut s, id, false);
        t.key(&mut s, EditKey::SelectAll).expect("selects");
        t.key(&mut s, EditKey::Backspace).expect("erases");
        t.finish(&mut s);
        assert!(!s.doc.nodes.contains_key(&id));
        assert_eq!(s.edit_menu().undo, Some("Delete"));
        s.undo();
        assert_eq!(text_of(&s, id), "Label");
    }

    #[test]
    fn finishing_without_typing_records_nothing() {
        let mut s = session();
        let mut t = TextTool::default();
        let id = placed(&mut t, &mut s);
        t.finish(&mut s);
        t.start(&mut s, id, false);
        t.key(
            &mut s,
            EditKey::Right {
                word: false,
                line: false,
                extend: false,
            },
        )
        .expect("moves");
        t.finish(&mut s);
        assert_eq!(s.edit_menu().undo, Some("Insert"));
    }

    #[test]
    fn copy_and_cut_hand_plain_text_to_the_clipboard() {
        let mut s = session();
        let mut t = TextTool::default();
        let id = placed(&mut t, &mut s);
        typed(&mut t, &mut s, "Buy now");
        t.key(
            &mut s,
            EditKey::Left {
                word: true,
                line: false,
                extend: true,
            },
        )
        .expect("selects");
        assert_eq!(
            t.key(&mut s, EditKey::Copy).expect("copies"),
            Some("now".to_owned())
        );
        assert_eq!(
            t.key(&mut s, EditKey::Cut).expect("cuts"),
            Some("now".to_owned())
        );
        assert_eq!(text_of(&s, id), "Buy ");
        t.key(&mut s, EditKey::Insert("later".into()))
            .expect("pastes");
        assert_eq!(text_of(&s, id), "Buy later");
    }

    #[test]
    fn a_press_outside_finishes_and_one_inside_moves_the_caret() {
        let mut s = session();
        let mut t = TextTool::default();
        let id = placed(&mut t, &mut s);
        typed(&mut t, &mut s, "Hello");
        // Without fonts the estimate is 8 px a character at 16 px.
        assert!(t.press(&mut s, Point::new(50.0 + 17.0, 45.0), false));
        t.release();
        t.key(&mut s, EditKey::Insert("_".into())).expect("types");
        assert_eq!(text_of(&s, id), "He_llo");
        assert!(!t.press(&mut s, Point::new(300.0, 150.0), false));
        assert_eq!(t.editing(), None);
    }

    #[test]
    fn dragging_selects_characters() {
        let mut s = session();
        let mut t = TextTool::default();
        let id = placed(&mut t, &mut s);
        typed(&mut t, &mut s, "Hello");
        t.press(&mut s, Point::new(50.0 + 1.0, 45.0), false);
        t.drag_to(&s, Point::new(50.0 + 23.0, 45.0));
        t.release();
        t.key(&mut s, EditKey::Insert("J".into())).expect("types");
        assert_eq!(text_of(&s, id), "Jlo");
    }

    #[test]
    fn start_at_puts_the_caret_under_the_pointer() {
        let mut s = session();
        let mut t = TextTool::default();
        let id = placed(&mut t, &mut s);
        typed(&mut t, &mut s, "Hello");
        t.finish(&mut s);
        t.start_at(&mut s, id, Point::new(50.0 + 39.0, 45.0));
        t.key(&mut s, EditKey::Insert("!".into())).expect("types");
        assert_eq!(text_of(&s, id), "Hello!");
    }

    #[test]
    fn the_overlay_follows_caret_and_selection() {
        let mut s = session();
        let mut t = TextTool::default();
        placed(&mut t, &mut s);
        let o = t.overlay(&s).expect("editing");
        assert_eq!(o.selection.len(), 1, "the placeholder is selected");
        assert_eq!(o.frame[0], Point::new(50.0, 40.0));
        typed(&mut t, &mut s, "ab\ncd");
        let o = t.overlay(&s).expect("editing");
        assert_eq!(o.selection, Vec::<[Point; 4]>::new());
        assert!((o.caret.0.x - 66.0).abs() < 1e-9, "after 'cd': 2 × 8 px");
        assert!(o.caret.0.y > 40.0, "on the second line");
        t.key(&mut s, EditKey::SelectAll).expect("selects");
        assert_eq!(t.overlay(&s).expect("editing").selection.len(), 2);
    }

    #[test]
    fn placing_inside_a_moved_layer_keeps_the_click_point() {
        let mut doc: Document = new_document("a", 400.0, 200.0);
        let layer = doc.layers[0];
        doc.nodes.get_mut(&layer).expect("layer").transform =
            Transform([1.0, 0.0, 0.0, 1.0, 100.0, 0.0]);
        let mut s = Session::new(doc);
        let mut t = TextTool::default();
        let id = placed(&mut t, &mut s);
        let world = s.doc.world_transform(id);
        assert_eq!((world.0[4], world.0[5]), (50.0, 40.0));
    }
}
