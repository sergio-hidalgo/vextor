//! The open documents (spec 05f): one tab each, which one is active, and
//! the rules for adding, switching and closing them.
//!
//! The clipboard is the app's, not a document's: it always sits in the
//! active session's `EditorState`, moving with the active tab, and is
//! parked here while no document is open.

use std::fs;
use std::path::Path;

use vextor_core::ClipboardPayload;

use crate::recovery::Recovery;
use crate::session::Session;

/// Names an open document for as long as it is open; never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DocId(u64);

/// One open document.
pub struct Tab {
    pub id: DocId,
    pub session: Session,
    /// Its autosave slot; `None` when autosave is off.
    pub recovery: Option<Recovery>,
}

/// Every open document, in tab order, and the active one.
#[derive(Default)]
pub struct Documents {
    tabs: Vec<Tab>,
    /// Index into `tabs`; `None` exactly when `tabs` is empty.
    active: Option<usize>,
    next_id: u64,
    /// The clipboard while no document is open.
    parked: Option<ClipboardPayload>,
}

impl Documents {
    /// True when no document is open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// The open documents, in tab order.
    #[must_use]
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// The open documents, in tab order, to change.
    pub fn tabs_mut(&mut self) -> &mut [Tab] {
        &mut self.tabs
    }

    /// The active document.
    #[must_use]
    pub fn active(&self) -> Option<&Tab> {
        self.active.map(|i| &self.tabs[i])
    }

    /// The active document, to change.
    pub fn active_mut(&mut self) -> Option<&mut Tab> {
        self.active.map(|i| &mut self.tabs[i])
    }

    /// The active document's session.
    #[must_use]
    pub fn session(&self) -> Option<&Session> {
        self.active().map(|t| &t.session)
    }

    /// The active document's session, to change.
    pub fn session_mut(&mut self) -> Option<&mut Session> {
        self.active_mut().map(|t| &mut t.session)
    }

    /// The document `id`, if it is still open.
    #[must_use]
    pub fn get(&self, id: DocId) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    /// The document `id`, if it is still open, to change.
    pub fn get_mut(&mut self, id: DocId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    /// Adds a document as the last tab and makes it active.
    pub fn add(&mut self, session: Session, recovery: Option<Recovery>) -> DocId {
        let id = DocId(self.next_id);
        self.next_id += 1;
        let clipboard = self.take_clipboard();
        self.tabs.push(Tab {
            id,
            session,
            recovery,
        });
        self.active = Some(self.tabs.len() - 1);
        self.give_clipboard(clipboard);
        id
    }

    /// Makes `id` the active document. Returns false when it is not open.
    pub fn activate(&mut self, id: DocId) -> bool {
        let Some(i) = self.index(id) else {
            return false;
        };
        if self.active != Some(i) {
            let clipboard = self.take_clipboard();
            self.active = Some(i);
            self.give_clipboard(clipboard);
        }
        true
    }

    /// Makes the next tab active (`forward`), or the previous one; wraps.
    pub fn cycle(&mut self, forward: bool) {
        let (Some(i), n) = (self.active, self.tabs.len()) else {
            return;
        };
        let next = if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        };
        let id = self.tabs[next].id;
        self.activate(id);
    }

    /// The open document saved to `path`, if any (the same file reached
    /// through another path counts).
    #[must_use]
    pub fn find_path(&self, path: &Path) -> Option<DocId> {
        let wanted = fs::canonicalize(path).ok();
        self.tabs
            .iter()
            .find(|t| {
                t.session.path().is_some_and(|p| {
                    p == path || (wanted.is_some() && fs::canonicalize(p).ok() == wanted)
                })
            })
            .map(|t| t.id)
    }

    /// The first document, in tab order, with unsaved changes that is not
    /// in `skip`.
    #[must_use]
    pub fn first_dirty(&self, skip: &[DocId]) -> Option<DocId> {
        self.tabs
            .iter()
            .find(|t| t.session.is_dirty() && !skip.contains(&t.id))
            .map(|t| t.id)
    }

    /// Closes `id` without asking and deletes its autosave. The tab to its
    /// right becomes active, else the one to its left. Returns false when
    /// it was not open.
    pub fn close(&mut self, id: DocId) -> bool {
        let Some(i) = self.index(id) else {
            return false;
        };
        let mut tab = self.tabs.remove(i);
        if let Some(r) = &mut tab.recovery {
            r.shutdown();
        }
        let was_active = self.active == Some(i);
        self.active = match self.active {
            _ if self.tabs.is_empty() => None,
            Some(a) if a == i => Some(i.min(self.tabs.len() - 1)),
            Some(a) if a > i => Some(a - 1),
            a => a,
        };
        if was_active {
            self.give_clipboard(tab.session.editor.clipboard.take());
        }
        true
    }

    fn index(&self, id: DocId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    fn take_clipboard(&mut self) -> Option<ClipboardPayload> {
        match self.active {
            Some(i) => self.tabs[i].session.editor.clipboard.take(),
            None => self.parked.take(),
        }
    }

    fn give_clipboard(&mut self, clipboard: Option<ClipboardPayload>) {
        let Some(c) = clipboard else {
            return;
        };
        match self.active {
            Some(i) => self.tabs[i].session.editor.clipboard = Some(c),
            None => self.parked = Some(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_doc::new_document;

    fn session(name: &str) -> Session {
        Session::new(new_document(name, 24.0, 24.0))
    }

    fn names(d: &Documents) -> Vec<String> {
        d.tabs()
            .iter()
            .map(|t| t.session.doc.artboards[0].name.clone())
            .collect()
    }

    fn active_name(d: &Documents) -> Option<String> {
        d.session().map(|s| s.doc.artboards[0].name.clone())
    }

    /// Three tabs with artboards a, b, c; c is active.
    fn three() -> (Documents, [DocId; 3]) {
        let mut d = Documents::default();
        let ids = ["a", "b", "c"].map(|n| d.add(session(n), None));
        (d, ids)
    }

    fn dirty(d: &mut Documents, id: DocId) {
        d.activate(id);
        let s = d.session_mut().expect("open");
        let board = s.doc.artboards[0].id;
        assert!(crate::artboard::rename(s, board, "edited"));
    }

    #[test]
    fn adding_makes_the_new_document_active() {
        let (d, [_, _, c]) = three();
        assert_eq!(names(&d), ["a", "b", "c"]);
        assert_eq!(d.active().map(|t| t.id), Some(c));
    }

    #[test]
    fn ids_are_never_reused() {
        let (mut d, [a, b, c]) = three();
        d.close(c);
        let again = d.add(session("d"), None);
        assert!(![a, b, c].contains(&again));
    }

    #[test]
    fn closing_the_active_tab_picks_the_right_then_the_left_neighbour() {
        let (mut d, [a, b, c]) = three();
        d.activate(b);
        assert!(d.close(b));
        assert_eq!(active_name(&d).as_deref(), Some("c"), "right neighbour");
        assert!(d.close(c));
        assert_eq!(active_name(&d).as_deref(), Some("a"), "no right: left");
        assert!(d.close(a));
        assert!(d.is_empty());
        assert_eq!(d.active().map(|t| t.id), None);
        assert!(!d.close(a), "already closed");
    }

    #[test]
    fn closing_a_background_tab_keeps_the_active_one() {
        let (mut d, [a, _, c]) = three();
        d.close(a);
        assert_eq!(d.active().map(|t| t.id), Some(c));
        assert_eq!(names(&d), ["b", "c"]);
    }

    #[test]
    fn cycling_wraps_both_ways() {
        let (mut d, [a, b, c]) = three();
        d.cycle(true);
        assert_eq!(d.active().map(|t| t.id), Some(a));
        d.cycle(false);
        assert_eq!(d.active().map(|t| t.id), Some(c));
        d.cycle(false);
        assert_eq!(d.active().map(|t| t.id), Some(b));
        Documents::default().cycle(true); // nothing open: no panic
    }

    #[test]
    fn first_dirty_follows_tab_order_and_skips() {
        let (mut d, [a, b, c]) = three();
        assert_eq!(d.first_dirty(&[]), None);
        dirty(&mut d, c);
        dirty(&mut d, a);
        assert_eq!(d.first_dirty(&[]), Some(a));
        assert_eq!(d.first_dirty(&[a]), Some(c));
        assert_eq!(d.first_dirty(&[a, c]), None);
        let _ = b;
    }

    #[test]
    fn opening_an_open_path_finds_its_tab() {
        let (mut d, _) = three();
        let path = std::env::temp_dir().join("vextor-05f-not-there.vextor");
        let id = d.add(
            Session::opened(new_document("x", 1.0, 1.0), path.clone()),
            None,
        );
        assert_eq!(d.find_path(&path), Some(id));
        assert_eq!(d.find_path(&path.with_file_name("other.vextor")), None);
    }

    #[test]
    fn undo_and_selection_belong_to_their_tab() {
        let (mut d, [a, b, _]) = three();
        dirty(&mut d, a);
        d.activate(b);
        assert!(
            !d.session_mut().expect("open").undo(),
            "b has nothing to undo"
        );
        d.activate(a);
        assert!(d.session_mut().expect("open").undo(), "a keeps its history");
    }

    #[test]
    fn the_clipboard_follows_the_active_tab_and_outlives_the_last_one() {
        let (mut d, [a, b, c]) = three();
        let payload = ClipboardPayload {
            nodes: Vec::new(),
            resources: Vec::new(),
        };
        d.session_mut().expect("open").editor.clipboard = Some(payload.clone());
        d.activate(a);
        assert_eq!(
            d.session().expect("open").editor.clipboard,
            Some(payload.clone())
        );
        for id in [a, b, c] {
            d.close(id);
        }
        let again = d.add(session("d"), None);
        assert_eq!(
            d.get(again).expect("open").session.editor.clipboard,
            Some(payload)
        );
    }
}
