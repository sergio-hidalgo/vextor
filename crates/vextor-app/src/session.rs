//! One open document with its history and editor state.

use vextor_core::{Command, CoreError, Document, EditorState, History};

/// The document being edited, its undo history and what the user is
/// working with. Every edit goes through [`Session::apply`].
pub struct Session {
    pub doc: Document,
    history: History,
    pub editor: EditorState,
    /// Changes whenever `doc` changes; the canvas re-renders on change.
    revision: u64,
    /// The canvas should fit the artboards on its next frame.
    pub needs_fit: bool,
}

/// What the Edit menu shows: the label of the step undo/redo would act on,
/// or `None` when the item is disabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditMenu {
    pub undo: Option<&'static str>,
    pub redo: Option<&'static str>,
}

impl Session {
    /// A session for a freshly created or loaded document: empty history,
    /// fit on first show.
    #[must_use]
    pub fn new(doc: Document) -> Self {
        Self {
            doc,
            history: History::new(),
            editor: EditorState::default(),
            revision: 0,
            needs_fit: true,
        }
    }

    /// Changes whenever the document changes.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Applies and records `cmd`.
    ///
    /// # Errors
    /// The command's [`CoreError`]; nothing changes.
    pub fn apply(&mut self, cmd: Command) -> Result<(), CoreError> {
        self.history
            .apply(&mut self.doc, cmd, &self.editor.selection)?;
        self.revision += 1;
        Ok(())
    }

    /// Reverts the last step and restores the selection it had. Returns
    /// false when there was nothing to undo.
    pub fn undo(&mut self) -> bool {
        let Some(selection) = self.history.undo(&mut self.doc) else {
            return false;
        };
        self.editor.selection = selection;
        self.revision += 1;
        true
    }

    /// Re-applies the last undone step. Returns false when there was none.
    pub fn redo(&mut self) -> bool {
        let done = self.history.redo(&mut self.doc);
        if done {
            self.revision += 1;
        }
        done
    }

    /// The state of the Undo and Redo menu items.
    #[must_use]
    pub fn edit_menu(&self) -> EditMenu {
        EditMenu {
            undo: self.history.undo_label(),
            redo: self.history.redo_label(),
        }
    }
}

#[cfg(test)]
mod tests {
    use vextor_core::{Command, Transform};

    use super::*;

    fn nudge(s: &Session, x: f64) -> Command {
        Command::SetTransform {
            ids: vec![s.doc.layers[0]],
            transform: Transform::translate(x, 0.0),
        }
    }

    #[test]
    fn new_session_has_both_items_disabled() {
        let s = Session::new(Document::new("t"));
        assert_eq!(
            s.edit_menu(),
            EditMenu {
                undo: None,
                redo: None
            }
        );
    }

    #[test]
    fn apply_enables_undo_with_the_command_label() {
        let mut s = Session::new(Document::new("t"));
        s.apply(nudge(&s, 10.0)).expect("valid command");
        assert_eq!(s.edit_menu().undo, Some("Transform"));
        assert_eq!(s.edit_menu().redo, None);
    }

    #[test]
    fn undo_enables_redo_and_disables_undo_when_empty() {
        let mut s = Session::new(Document::new("t"));
        s.apply(nudge(&s, 10.0)).expect("valid command");
        assert!(s.undo());
        assert_eq!(
            s.edit_menu(),
            EditMenu {
                undo: None,
                redo: Some("Transform")
            }
        );
    }

    #[test]
    fn redo_moves_the_step_back_to_undo() {
        let mut s = Session::new(Document::new("t"));
        s.apply(nudge(&s, 10.0)).expect("valid command");
        s.undo();
        assert!(s.redo());
        assert_eq!(
            s.edit_menu(),
            EditMenu {
                undo: Some("Transform"),
                redo: None
            }
        );
    }

    #[test]
    fn a_new_edit_clears_redo() {
        let mut s = Session::new(Document::new("t"));
        s.apply(nudge(&s, 10.0)).expect("valid command");
        s.undo();
        s.apply(nudge(&s, 20.0)).expect("valid command");
        assert_eq!(s.edit_menu().redo, None);
    }

    #[test]
    fn undo_and_redo_on_empty_history_do_nothing() {
        let mut s = Session::new(Document::new("t"));
        assert!(!s.undo());
        assert!(!s.redo());
        assert_eq!(s.revision(), 0);
    }

    #[test]
    fn revision_changes_on_apply_undo_and_redo() {
        let mut s = Session::new(Document::new("t"));
        s.apply(nudge(&s, 10.0)).expect("valid command");
        let r1 = s.revision();
        s.undo();
        let r2 = s.revision();
        s.redo();
        let r3 = s.revision();
        assert!(r1 > 0 && r2 > r1 && r3 > r2);
    }

    #[test]
    fn failed_command_keeps_revision_and_menu() {
        let mut s = Session::new(Document::new("t"));
        let bad = Command::SetTransform {
            ids: vec![s.doc.layers[0]],
            transform: Transform([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]),
        };
        assert!(s.apply(bad).is_err());
        assert_eq!(s.revision(), 0);
        assert_eq!(s.edit_menu().undo, None);
    }
}
