//! "Save changes?" before a document closes or the app quits (spec 05f),
//! as a small state machine the app drives with the user's answers.

use crate::documents::{DocId, Documents};
use crate::prompts::SaveAnswer;

/// What is waiting on "Save changes?".
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Pending {
    /// Close one document.
    Close(DocId),
    /// Quit: ask about each document with unsaved changes in turn.
    /// `dont_save` are the ones already answered "Don't Save".
    Quit { dont_save: Vec<DocId> },
}

/// What the app does next.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Next {
    /// Show "Save changes?" for this document (make it active first).
    Ask(Pending, DocId),
    /// Nothing more to ask; the app keeps running.
    Done,
    /// Every document was dealt with: quit.
    Quit,
}

impl Pending {
    /// Takes the request as far as it goes without asking: a document with
    /// nothing unsaved closes at once.
    pub(super) fn next(self, docs: &mut Documents) -> Next {
        match self {
            Self::Close(id) => match docs.get(id) {
                Some(t) if t.session.is_dirty() => Next::Ask(self, id),
                _ => {
                    docs.close(id);
                    Next::Done
                }
            },
            Self::Quit { dont_save } => match docs.first_dirty(&dont_save) {
                Some(id) => Next::Ask(Self::Quit { dont_save }, id),
                None => Next::Quit,
            },
        }
    }

    /// Goes on after the user's `answer` about document `id`. `saved` says
    /// whether the app managed to save it (read only for Save); a failed or
    /// cancelled save stops the whole request.
    pub(super) fn answer(
        self,
        docs: &mut Documents,
        id: DocId,
        answer: SaveAnswer,
        saved: bool,
    ) -> Next {
        match answer {
            SaveAnswer::Cancel => return Next::Done,
            SaveAnswer::Save if !saved => return Next::Done,
            SaveAnswer::Save | SaveAnswer::DontSave => {}
        }
        match self {
            Self::Close(_) => {
                docs.close(id);
                Next::Done
            }
            Self::Quit { mut dont_save } => {
                if answer == SaveAnswer::DontSave {
                    dont_save.push(id);
                }
                Self::Quit { dont_save }.next(docs)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_doc::new_document;
    use crate::session::Session;

    fn docs(dirty: &[bool]) -> (Documents, Vec<DocId>) {
        let mut d = Documents::default();
        let ids = dirty
            .iter()
            .map(|&is_dirty| {
                let id = d.add(Session::new(new_document("a", 24.0, 24.0)), None);
                if is_dirty {
                    let s = d.session_mut().expect("open");
                    let board = s.doc.artboards[0].id;
                    crate::artboard::rename(s, board, "edited");
                }
                id
            })
            .collect();
        (d, ids)
    }

    /// What the app does for Save: saving makes the document clean.
    fn save(d: &mut Documents, id: DocId) -> bool {
        d.activate(id);
        let s = d.session_mut().expect("open");
        s.mark_saved(std::env::temp_dir().join("saved.vxtr"));
        true
    }

    #[test]
    fn closing_a_clean_document_does_not_ask() {
        let (mut d, ids) = docs(&[false]);
        assert_eq!(Pending::Close(ids[0]).next(&mut d), Next::Done);
        assert!(d.is_empty());
    }

    #[test]
    fn closing_a_dirty_document_asks_and_cancel_keeps_it() {
        let (mut d, ids) = docs(&[true]);
        let Next::Ask(p, id) = Pending::Close(ids[0]).next(&mut d) else {
            panic!("asks");
        };
        assert_eq!(id, ids[0]);
        assert_eq!(p.answer(&mut d, id, SaveAnswer::Cancel, false), Next::Done);
        assert_eq!(d.tabs().len(), 1);
    }

    #[test]
    fn dont_save_closes_and_a_failed_save_keeps_it() {
        let (mut d, ids) = docs(&[true]);
        let p = Pending::Close(ids[0]);
        assert_eq!(
            p.clone().answer(&mut d, ids[0], SaveAnswer::Save, false),
            Next::Done
        );
        assert_eq!(d.tabs().len(), 1, "save failed or was cancelled");
        assert_eq!(
            p.answer(&mut d, ids[0], SaveAnswer::DontSave, false),
            Next::Done
        );
        assert!(d.is_empty());
    }

    #[test]
    fn quitting_with_nothing_unsaved_quits_at_once() {
        let (mut d, _) = docs(&[false, false]);
        assert_eq!(Pending::Quit { dont_save: vec![] }.next(&mut d), Next::Quit);
    }

    #[test]
    fn quitting_asks_about_each_unsaved_document_in_turn() {
        let (mut d, ids) = docs(&[true, false, true]);
        let Next::Ask(p, first) = Pending::Quit { dont_save: vec![] }.next(&mut d) else {
            panic!("asks");
        };
        assert_eq!(first, ids[0]);
        let saved = save(&mut d, first);
        let Next::Ask(p, second) = p.answer(&mut d, first, SaveAnswer::Save, saved) else {
            panic!("asks again");
        };
        assert_eq!(second, ids[2]);
        assert_eq!(
            p.answer(&mut d, second, SaveAnswer::DontSave, false),
            Next::Quit
        );
    }

    #[test]
    fn cancel_on_the_second_document_keeps_both_and_the_app_running() {
        let (mut d, ids) = docs(&[true, true]);
        let Next::Ask(p, first) = Pending::Quit { dont_save: vec![] }.next(&mut d) else {
            panic!("asks");
        };
        let Next::Ask(p, second) = p.answer(&mut d, first, SaveAnswer::DontSave, false) else {
            panic!("asks again");
        };
        assert_eq!(second, ids[1]);
        assert_eq!(
            p.answer(&mut d, second, SaveAnswer::Cancel, false),
            Next::Done
        );
        assert_eq!(d.tabs().len(), 2);
        assert!(
            d.tabs().iter().all(|t| t.session.is_dirty()),
            "nothing lost"
        );
    }
}
