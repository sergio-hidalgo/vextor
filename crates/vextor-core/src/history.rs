//! Undo/redo stacks with gesture coalescing.

use crate::change::Change;
use crate::command::Command;
use crate::document::Document;
use crate::editor::Selection;
use crate::error::CoreError;

/// Default number of undo steps kept.
pub const DEFAULT_LIMIT: usize = 200;

/// Undo and redo stacks for one document.
///
/// Between [`History::begin_gesture`] and [`History::end_gesture`],
/// consecutive changes with the same label and the same targets merge into
/// one step, so one drag is one undo.
#[derive(Clone, Debug)]
pub struct History {
    undo: Vec<Change>,
    redo: Vec<Change>,
    limit: usize,
    in_gesture: bool,
    /// The top undo step was pushed during the open gesture.
    can_merge: bool,
}

impl Default for History {
    fn default() -> Self {
        Self::with_limit(DEFAULT_LIMIT)
    }
}

impl History {
    /// An empty history keeping [`DEFAULT_LIMIT`] steps.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty history keeping at most `limit` (≥ 1) steps.
    #[must_use]
    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
            in_gesture: false,
            can_merge: false,
        }
    }

    /// Applies `cmd` to `doc` and records it. `selection` is the selection
    /// before the command, restored by its undo.
    ///
    /// # Errors
    /// The command's [`CoreError`]; nothing is recorded and `doc` is unchanged.
    pub fn apply(
        &mut self,
        doc: &mut Document,
        cmd: Command,
        selection: &Selection,
    ) -> Result<(), CoreError> {
        let change = doc.apply(cmd)?;
        self.push(change, selection.clone());
        Ok(())
    }

    /// Records a change already applied to the document. A change that
    /// altered nothing is dropped.
    pub fn push(&mut self, mut change: Change, selection_before: Selection) {
        if change.before == change.after {
            return;
        }
        self.redo.clear();
        if self.can_merge
            && let Some(last) = self.undo.last_mut()
            && last.label == change.label
            && last.before.same_targets(&change.before)
        {
            last.before.fill_from(change.before);
            last.after.overwrite_with(change.after);
            return;
        }
        change.selection_before = selection_before;
        self.undo.push(change);
        self.can_merge = self.in_gesture;
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
    }

    /// Starts a gesture (pointer down): following changes may merge.
    pub fn begin_gesture(&mut self) {
        self.in_gesture = true;
        self.can_merge = false;
    }

    /// Ends the gesture (pointer up).
    pub fn end_gesture(&mut self) {
        self.in_gesture = false;
        self.can_merge = false;
    }

    /// Reverts the last step. Returns the selection to restore, or `None`
    /// when there is nothing to undo.
    pub fn undo(&mut self, doc: &mut Document) -> Option<Selection> {
        let change = self.undo.pop()?;
        doc.apply_patch(&change.before);
        let selection = change.selection_before.clone();
        self.redo.push(change);
        self.can_merge = false;
        Some(selection)
    }

    /// Re-applies the last undone step. Returns false when there is none.
    pub fn redo(&mut self, doc: &mut Document) -> bool {
        let Some(change) = self.redo.pop() else {
            return false;
        };
        doc.apply_patch(&change.after);
        self.undo.push(change);
        self.can_merge = false;
        true
    }

    /// Label of the step [`History::undo`] would revert.
    #[must_use]
    pub fn undo_label(&self) -> Option<&'static str> {
        self.undo.last().map(|c| c.label)
    }

    /// Label of the step [`History::redo`] would re-apply.
    #[must_use]
    pub fn redo_label(&self) -> Option<&'static str> {
        self.redo.last().map(|c| c.label)
    }

    /// Number of undo steps available.
    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Number of redo steps available.
    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
}
