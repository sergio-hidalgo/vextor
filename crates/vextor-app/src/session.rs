//! One open document with its history and editor state.

use std::path::PathBuf;
use std::sync::Arc;

use vextor_core::{
    Command, CoreError, Document, EditorState, Geometry, GridSettings, History, NodeId, NodeKind,
    Path, PathNode, PathNodeRef, Rgba, Selection, Shape, Style,
};
use vextor_geom::Fonts;

use crate::files;
use crate::swatch_library::Swatches;

/// The document being edited, its undo history and what the user is
/// working with. Every edit goes through [`Session::apply`].
pub struct Session {
    pub doc: Document,
    history: History,
    pub editor: EditorState,
    /// Fonts and text layouts, shared by every session of the app; empty
    /// until the system fonts have loaded ([`Session::set_fonts`]).
    pub fonts: Arc<Fonts>,
    /// Changes whenever `doc` changes; the canvas re-renders on change.
    revision: u64,
    /// The canvas should fit the artboards on its next frame.
    pub needs_fit: bool,
    /// The artboard last picked, shown by Inspector ▸ Artboard (spec 08;
    /// see `artboard::current`).
    pub artboard: Option<vextor_core::ArtboardId>,
    /// The eyedropper (spec 09): the colour editor waiting for a pick on
    /// the canvas, by its key; and the colour picked for it, until that
    /// editor takes it. Not saved, not undone.
    pub eyedropper: Option<u64>,
    pub picked: Option<(u64, Rgba)>,
    /// The app's swatch library, shared by every session (spec 09; the
    /// app loads and saves it). Empty and unsaved in tests.
    pub swatches: Swatches,
    /// The file the document was opened from or last saved to.
    path: Option<PathBuf>,
    /// `revision` when the document last matched its file (or was
    /// created); `None` when it never did, e.g. after recovery.
    saved_revision: Option<u64>,
}

/// What the Edit menu shows: the label of the step undo/redo would act on,
/// or `None` when the item is disabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditMenu {
    pub undo: Option<&'static str>,
    pub redo: Option<&'static str>,
}

impl Session {
    /// A session for a freshly created document: empty history, no file,
    /// nothing unsaved, fit on first show.
    #[must_use]
    pub fn new(doc: Document) -> Self {
        Self {
            doc,
            history: History::new(),
            editor: EditorState::default(),
            fonts: Arc::default(),
            revision: 0,
            needs_fit: true,
            artboard: None,
            eyedropper: None,
            picked: None,
            swatches: Swatches::default(),
            path: None,
            saved_revision: Some(0),
        }
    }

    /// A session for a document opened from `path`, nothing unsaved.
    #[must_use]
    pub fn opened(doc: Document, path: PathBuf) -> Self {
        Self {
            path: Some(path),
            ..Self::new(doc)
        }
    }

    /// A session for a document recovered from an autosave: it has unsaved
    /// changes until saved. `path` is the file it came from, if any.
    #[must_use]
    pub fn recovered(doc: Document, path: Option<PathBuf>) -> Self {
        Self {
            path,
            saved_revision: None,
            ..Self::new(doc)
        }
    }

    /// The file this document is saved to, if any.
    #[must_use]
    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }

    /// True when the document changed since it was created, opened or
    /// last saved. Undoing back to the saved state still counts.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.saved_revision != Some(self.revision)
    }

    /// Records that the document, as it is now, was saved to `path`.
    pub fn mark_saved(&mut self, path: PathBuf) {
        self.path = Some(path);
        self.saved_revision = Some(self.revision);
    }

    /// The document's file was renamed to `path`; whether it has unsaved
    /// changes stays as it was.
    pub fn moved_to(&mut self, path: PathBuf) {
        self.path = Some(path);
    }

    /// What the title bar and dialogs call the document: the file name,
    /// or the document name before the first save.
    #[must_use]
    pub fn display_name(&self) -> String {
        self.path
            .as_deref()
            .map_or_else(|| self.doc.name.clone(), files::display_name)
    }

    /// The window title, with `•` while there are unsaved changes.
    #[must_use]
    pub fn title(&self) -> String {
        let dirty = if self.is_dirty() { " •" } else { "" };
        format!("{}{dirty} — Vextor", self.display_name())
    }

    /// Changes whenever the document changes.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Switches to `fonts` (the system fonts finished loading). Text lays
    /// out again, so the revision moves on; a clean document stays clean.
    pub fn set_fonts(&mut self, fonts: Arc<Fonts>) {
        let clean = !self.is_dirty();
        self.fonts = fonts;
        self.revision += 1;
        if clean {
            self.saved_revision = Some(self.revision);
        }
    }

    /// Applies and records `cmd`.
    ///
    /// # Errors
    /// The command's [`CoreError`]; nothing changes.
    /// A style given to a shape becomes the default style of new shapes.
    pub fn apply(&mut self, cmd: Command) -> Result<(), CoreError> {
        let style = shape_style(&self.doc, &cmd).cloned();
        self.history
            .apply(&mut self.doc, cmd, &self.editor.selection)?;
        self.revision += 1;
        if let Some(style) = style {
            self.editor.default_style = style;
        }
        Ok(())
    }

    /// Sets the grid and snap settings. A view setting (`ui-editor`): not
    /// an undo step, but saved with the document, so it counts as a
    /// change. Returns false when `grid` is invalid or the same.
    pub fn set_grid(&mut self, grid: GridSettings) -> bool {
        if grid == self.doc.grid || self.doc.apply(Command::SetGrid(grid)).is_err() {
            return false;
        }
        self.revision += 1;
        true
    }

    /// Reverts the last step and restores the selection it had. Returns
    /// false when there was nothing to undo.
    pub fn undo(&mut self) -> bool {
        let Some(selection) = self.history.undo(&mut self.doc) else {
            return false;
        };
        self.editor.selection = selection;
        self.revision += 1;
        self.prune_selection();
        true
    }

    /// Re-applies the last undone step. Returns false when there was none.
    pub fn redo(&mut self) -> bool {
        let done = self.history.redo(&mut self.doc);
        if done {
            self.revision += 1;
            self.prune_selection();
        }
        done
    }

    /// Starts a pointer gesture: the changes until [`Session::end_gesture`]
    /// that have the same label and targets become one undo step.
    pub fn begin_gesture(&mut self) {
        self.history.begin_gesture();
    }

    /// Ends the gesture started by [`Session::begin_gesture`].
    pub fn end_gesture(&mut self) {
        self.history.end_gesture();
    }

    /// The selected objects, bottom → top. Selected path nodes or a
    /// segment stand for their shape (spec 07).
    #[must_use]
    pub fn selected(&self) -> Vec<NodeId> {
        match &self.editor.selection {
            Selection::Objects(ids) => self.doc.sorted_by_paint_order(ids),
            Selection::PathNodes(refs) => refs.first().map(|r| vec![r.shape]).unwrap_or_default(),
            Selection::Segment { shape, .. } => vec![*shape],
            Selection::None => Vec::new(),
        }
    }

    /// Selects `ids` (nothing when empty).
    pub fn select(&mut self, ids: Vec<NodeId>) {
        self.editor.selection = if ids.is_empty() {
            Selection::None
        } else {
            Selection::Objects(ids)
        };
    }

    /// Drops selected objects, path nodes and segments that no longer
    /// exist (after redo, say). Path nodes all gone leave their shape
    /// selected, if it is still there.
    fn prune_selection(&mut self) {
        match &self.editor.selection {
            Selection::Objects(ids) => {
                let kept: Vec<NodeId> = ids
                    .iter()
                    .copied()
                    .filter(|id| self.doc.nodes.contains_key(id))
                    .collect();
                self.select(kept);
            }
            Selection::PathNodes(refs) => {
                let shape = refs.first().map(|r| r.shape);
                let kept: Vec<PathNodeRef> = refs
                    .iter()
                    .copied()
                    .filter(|r| self.path_node(*r).is_some())
                    .collect();
                if kept.is_empty() {
                    self.select(
                        shape
                            .filter(|id| self.path_of(*id).is_some())
                            .into_iter()
                            .collect(),
                    );
                } else {
                    self.editor.selection = Selection::PathNodes(kept);
                }
            }
            &Selection::Segment {
                shape,
                subpath,
                index,
            } => {
                let exists = self
                    .path_of(shape)
                    .and_then(|p| p.subpaths.get(subpath))
                    .is_some_and(|sp| index < sp.segment_count());
                if !exists {
                    self.select(self.path_of(shape).map(|_| shape).into_iter().collect());
                }
            }
            Selection::None => {}
        }
    }

    /// The path of shape `id`, when it is a path.
    #[must_use]
    pub fn path_of(&self, id: NodeId) -> Option<&Path> {
        match &self.doc.nodes.get(&id)?.kind {
            NodeKind::Shape(Shape {
                geometry: Geometry::Path(p),
                ..
            }) => Some(p),
            _ => None,
        }
    }

    /// The path node `r` points at, if it exists.
    #[must_use]
    pub fn path_node(&self, r: PathNodeRef) -> Option<&PathNode> {
        self.path_of(r.shape)?
            .subpaths
            .get(r.subpath)?
            .nodes
            .get(r.index)
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

/// The style `cmd` gives to at least one shape, if it does (`SetStyle`,
/// also inside a batch; the last one wins).
fn shape_style<'a>(doc: &Document, cmd: &'a Command) -> Option<&'a Style> {
    match cmd {
        Command::SetStyle { ids, style } => ids
            .iter()
            .any(|id| matches!(doc.nodes.get(id).map(|n| &n.kind), Some(NodeKind::Shape(_))))
            .then_some(style),
        Command::Batch { commands, .. } => commands.iter().rev().find_map(|c| shape_style(doc, c)),
        _ => None,
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
    fn styling_a_shape_sets_the_default_style_but_styling_text_does_not() {
        use vextor_core::{CornerRadii, Geometry, Node, Paint, Rect, Rgba, Shape, Text};
        let mut doc = Document::new("t");
        let layer = doc.layers[0];
        let (shape, text) = (doc.alloc_node_id(), doc.alloc_node_id());
        let rect = Shape {
            geometry: Geometry::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: CornerRadii::default(),
            },
            style: Style::default(),
        };
        doc.apply(Command::Insert {
            parent: Some(layer),
            index: 0,
            nodes: vec![
                Node::new(shape, NodeKind::Shape(rect)),
                Node::new(text, NodeKind::Text(Text::new("a", "Inter"))),
            ],
        })
        .expect("inserts");
        let mut s = Session::new(doc);
        let blue = Style {
            fill: Paint::Solid(Rgba::rgb(0, 0, 255)),
            stroke: None,
        };
        let red = Style {
            fill: Paint::Solid(Rgba::rgb(255, 0, 0)),
            stroke: None,
        };
        s.apply(Command::SetStyle {
            ids: vec![text],
            style: red,
        })
        .expect("styles");
        assert_eq!(s.editor.default_style, Style::default(), "text only");
        s.apply(Command::Batch {
            label: "Restyle",
            commands: vec![Command::SetStyle {
                ids: vec![shape],
                style: blue.clone(),
            }],
        })
        .expect("styles");
        assert_eq!(s.editor.default_style, blue);
        s.undo();
        assert_eq!(
            s.editor.default_style, blue,
            "undo keeps the last used style"
        );
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
    fn new_fonts_redraw_without_marking_the_document_changed() {
        let mut s = Session::new(Document::new("t"));
        let before = s.revision();
        s.set_fonts(Arc::default());
        assert!(s.revision() > before, "text lays out again");
        assert!(!s.is_dirty());

        s.apply(nudge(&s, 10.0)).expect("valid command");
        s.set_fonts(Arc::default());
        assert!(s.is_dirty(), "unsaved work stays unsaved");
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
    fn edits_mark_the_document_dirty_until_saved() {
        let mut s = Session::new(Document::new("icon"));
        assert!(!s.is_dirty());
        assert_eq!(s.title(), "icon — Vextor");
        s.apply(nudge(&s, 10.0)).expect("valid command");
        assert!(s.is_dirty());
        assert_eq!(s.title(), "icon • — Vextor");
        s.mark_saved(PathBuf::from("/tmp/button.vextor"));
        assert!(!s.is_dirty());
        assert_eq!(s.title(), "button.vextor — Vextor");
        assert_eq!(s.path(), Some(std::path::Path::new("/tmp/button.vextor")));
    }

    #[test]
    fn undo_after_a_save_is_an_unsaved_change() {
        let mut s = Session::new(Document::new("t"));
        s.apply(nudge(&s, 10.0)).expect("valid command");
        s.mark_saved(PathBuf::from("/tmp/t.vextor"));
        s.undo();
        assert!(s.is_dirty());
    }

    #[test]
    fn failed_command_keeps_the_document_clean() {
        let mut s = Session::opened(Document::new("t"), PathBuf::from("/tmp/t.vextor"));
        let bad = Command::SetTransform {
            ids: vec![s.doc.layers[0]],
            transform: Transform([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]),
        };
        assert!(s.apply(bad).is_err());
        assert!(!s.is_dirty());
    }

    #[test]
    fn a_recovered_document_is_dirty_and_keeps_its_file() {
        let s = Session::recovered(Document::new("t"), Some(PathBuf::from("/tmp/a.vextor")));
        assert!(s.is_dirty());
        assert_eq!(s.title(), "a.vextor • — Vextor");
        let s = Session::recovered(Document::new("t"), None);
        assert_eq!(s.path(), None);
        assert!(s.is_dirty());
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
