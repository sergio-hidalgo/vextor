//! Editing the characters of one text object (spec 05c): keys in, content,
//! caret and selection out. Pure, so it is tested without a window; the
//! canvas feeds it keys and applies the content as `SetText`.

use std::ops::Range;

use kurbo::Point;
use vextor_core::MAX_TEXT_CHARS;
use vextor_geom::TextLayout;

/// One input while editing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditKey {
    /// Typed or pasted characters.
    Insert(String),
    /// `Enter`: a new line.
    NewLine,
    /// `⌫`: the selection, or the character before the caret.
    Backspace,
    /// `⌦`: the selection, or the character after the caret.
    Delete,
    /// Arrows. `word`: `⌥` (left/right by word); `line`: `⌘` (to the
    /// start/end of the line); `extend`: `⇧` (grow the selection).
    Left {
        word: bool,
        line: bool,
        extend: bool,
    },
    Right {
        word: bool,
        line: bool,
        extend: bool,
    },
    Up {
        extend: bool,
    },
    Down {
        extend: bool,
    },
    /// `⌘A`.
    SelectAll,
    /// `⌘C`.
    Copy,
    /// `⌘X`.
    Cut,
    /// `Esc`.
    Finish,
}

/// What the caller does after a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Keep editing; `changed` when the content changed.
    Continue { changed: bool },
    /// Put this plain text on the system clipboard, keep editing.
    Clipboard { text: String, changed: bool },
    /// Editing is over.
    Finish,
}

/// The characters being edited, the caret and the selection. Indexes are
/// byte offsets into `content`, always on character boundaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEditor {
    content: String,
    /// Where typing goes.
    caret: usize,
    /// The other end of the selection; equals `caret` when nothing is
    /// selected.
    anchor: usize,
    /// x the caret keeps while moving up and down through lines.
    goal_x: Option<i64>,
}

impl TextEditor {
    /// Starts editing `content` with everything selected, so typing
    /// replaces it (a new "Text" placeholder, or a label being retyped).
    #[must_use]
    pub fn new(content: &str) -> Self {
        Self {
            content: content.to_owned(),
            caret: content.len(),
            anchor: 0,
            goal_x: None,
        }
    }

    /// The characters as edited so far.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Byte index of the caret.
    #[must_use]
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// The selected byte range (empty when nothing is selected).
    #[must_use]
    pub fn selection(&self) -> Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }

    /// Puts the caret at byte `index` (clamped to a character boundary),
    /// growing the selection with `extend` (a click or a drag on canvas).
    pub fn place(&mut self, index: usize, extend: bool) {
        self.caret = floor_boundary(&self.content, index);
        if !extend {
            self.anchor = self.caret;
        }
        self.goal_x = None;
    }

    /// Applies one key. `layout` is the text's current layout, used to move
    /// up and down between lines (without it those keys go to the start or
    /// end).
    pub fn key(&mut self, key: EditKey, layout: Option<&TextLayout>) -> Outcome {
        let vertical = matches!(key, EditKey::Up { .. } | EditKey::Down { .. });
        if !vertical {
            self.goal_x = None;
        }
        let changed = match key {
            EditKey::Insert(s) => self.insert(&s),
            EditKey::NewLine => self.insert("\n"),
            EditKey::Backspace => self.erase(prev_boundary, true),
            EditKey::Delete => self.erase(next_boundary, false),
            EditKey::Left { word, line, extend } => {
                let to = if line {
                    line_start(&self.content, self.caret)
                } else if word {
                    prev_word(&self.content, self.caret)
                } else if !extend && !self.selection().is_empty() {
                    self.selection().start
                } else {
                    prev_boundary(&self.content, self.caret)
                };
                self.move_to(to, extend);
                false
            }
            EditKey::Right { word, line, extend } => {
                let to = if line {
                    line_end(&self.content, self.caret)
                } else if word {
                    next_word(&self.content, self.caret)
                } else if !extend && !self.selection().is_empty() {
                    self.selection().end
                } else {
                    next_boundary(&self.content, self.caret)
                };
                self.move_to(to, extend);
                false
            }
            EditKey::Up { extend } => {
                let to = self.vertical(layout, -1);
                self.move_to(to, extend);
                false
            }
            EditKey::Down { extend } => {
                let to = self.vertical(layout, 1);
                self.move_to(to, extend);
                false
            }
            EditKey::SelectAll => {
                self.anchor = 0;
                self.caret = self.content.len();
                false
            }
            EditKey::Copy => {
                let text = self.content[self.selection()].to_owned();
                return if text.is_empty() {
                    Outcome::Continue { changed: false }
                } else {
                    Outcome::Clipboard {
                        text,
                        changed: false,
                    }
                };
            }
            EditKey::Cut => {
                let text = self.content[self.selection()].to_owned();
                if text.is_empty() {
                    return Outcome::Continue { changed: false };
                }
                self.replace_selection("");
                return Outcome::Clipboard {
                    text,
                    changed: true,
                };
            }
            EditKey::Finish => return Outcome::Finish,
        };
        Outcome::Continue { changed }
    }

    /// Types `s` over the selection, keeping the content within
    /// [`MAX_TEXT_CHARS`]; `\r\n` and `\r` become `\n`, other control
    /// characters (tabs included) are dropped.
    fn insert(&mut self, s: &str) -> bool {
        let clean: String = s
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .chars()
            .filter(|&c| c == '\n' || !c.is_control())
            .collect();
        let kept = self.content.chars().count() - self.content[self.selection()].chars().count();
        let room = MAX_TEXT_CHARS.saturating_sub(kept);
        let clean: String = clean.chars().take(room).collect();
        if clean.is_empty() && self.selection().is_empty() {
            return false;
        }
        self.replace_selection(&clean);
        true
    }

    /// Removes the selection, or the character on the `before` side.
    fn erase(&mut self, step: impl Fn(&str, usize) -> usize, before: bool) -> bool {
        if self.selection().is_empty() {
            let other = step(&self.content, self.caret);
            if other == self.caret {
                return false;
            }
            self.anchor = other;
            if !before {
                std::mem::swap(&mut self.anchor, &mut self.caret);
            }
        }
        self.replace_selection("");
        true
    }

    fn replace_selection(&mut self, with: &str) {
        let r = self.selection();
        self.content.replace_range(r.clone(), with);
        self.caret = r.start + with.len();
        self.anchor = self.caret;
    }

    fn move_to(&mut self, to: usize, extend: bool) {
        self.caret = to;
        if !extend {
            self.anchor = to;
        }
    }

    /// The index one line up (`dir` −1) or down (+1) at the remembered x.
    fn vertical(&mut self, layout: Option<&TextLayout>, dir: isize) -> usize {
        let Some(layout) = layout.filter(|l| !l.lines.is_empty()) else {
            return if dir < 0 { 0 } else { self.content.len() };
        };
        let (line, x) = layout.caret(self.caret);
        // Kept in hundredths of a px so the editor stays `Eq`.
        #[allow(clippy::cast_possible_truncation)]
        let goal = *self.goal_x.get_or_insert((x * 100.0).round() as i64);
        #[allow(clippy::cast_precision_loss)]
        let goal = goal as f64 / 100.0;
        let target = line
            .checked_add_signed(dir)
            .filter(|&l| l < layout.lines.len());
        match target {
            Some(l) => layout.index_at(Point::new(goal, layout.lines[l].rect.center().y)),
            None if dir < 0 => 0,
            None => self.content.len(),
        }
    }
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn prev_boundary(s: &str, i: usize) -> usize {
    s[..i].char_indices().next_back().map_or(0, |(j, _)| j)
}

fn next_boundary(s: &str, i: usize) -> usize {
    s[i..].chars().next().map_or(i, |c| i + c.len_utf8())
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

/// Start of the word before `i` (skipping spaces and punctuation first).
fn prev_word(s: &str, i: usize) -> usize {
    let mut chars = s[..i].char_indices().rev().peekable();
    while chars.next_if(|(_, c)| !is_word(*c)).is_some() {}
    let mut start = chars.peek().map_or(0, |(j, _)| *j);
    while let Some((j, _)) = chars.next_if(|(_, c)| is_word(*c)) {
        start = j;
    }
    start
}

/// End of the word after `i` (skipping spaces and punctuation first).
fn next_word(s: &str, i: usize) -> usize {
    let mut chars = s[i..].char_indices().peekable();
    while chars.next_if(|(_, c)| !is_word(*c)).is_some() {}
    while chars.next_if(|(_, c)| is_word(*c)).is_some() {}
    chars.peek().map_or(s.len(), |(j, _)| i + j)
}

fn line_start(s: &str, i: usize) -> usize {
    s[..i].rfind('\n').map_or(0, |j| j + 1)
}

fn line_end(s: &str, i: usize) -> usize {
    s[i..].find('\n').map_or(s.len(), |j| i + j)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vextor_core::Text;
    use vextor_geom::{FontDb, layout};

    use super::*;

    const REGULAR: &[u8] =
        include_bytes!("../../vextor-geom/tests/fixtures/fonts/NotoSans-Regular.ttf");

    fn plain() -> EditKey {
        EditKey::Left {
            word: false,
            line: false,
            extend: false,
        }
    }

    fn right(word: bool, line: bool, extend: bool) -> EditKey {
        EditKey::Right { word, line, extend }
    }

    fn left(word: bool, line: bool, extend: bool) -> EditKey {
        EditKey::Left { word, line, extend }
    }

    /// An editor on `content` with the caret at byte `at`, nothing selected.
    fn at(content: &str, at: usize) -> TextEditor {
        let mut e = TextEditor::new(content);
        e.place(at, false);
        e
    }

    fn typed(e: &mut TextEditor, s: &str) {
        for c in s.chars() {
            e.key(EditKey::Insert(c.to_string()), None);
        }
    }

    #[test]
    fn typing_replaces_the_placeholder() {
        let mut e = TextEditor::new("Text");
        assert_eq!(e.selection(), 0..4);
        typed(&mut e, "Buy now");
        assert_eq!(e.content(), "Buy now");
        assert_eq!(e.caret(), 7);
        assert!(e.selection().is_empty());
    }

    #[test]
    fn keys_report_whether_the_content_changed() {
        let mut e = at("ab", 2);
        assert_eq!(
            e.key(EditKey::Insert("c".into()), None),
            Outcome::Continue { changed: true }
        );
        assert_eq!(e.key(plain(), None), Outcome::Continue { changed: false });
        assert_eq!(e.key(EditKey::Finish, None), Outcome::Finish);
    }

    #[test]
    fn enter_inserts_a_line_break() {
        let mut e = at("Buynow", 3);
        e.key(EditKey::NewLine, None);
        assert_eq!(e.content(), "Buy\nnow");
        assert_eq!(e.caret(), 4);
    }

    #[test]
    fn backspace_and_delete_remove_one_character_or_the_selection() {
        let mut e = at("añb", 3); // after "ñ" (2 bytes)
        e.key(EditKey::Backspace, None);
        assert_eq!((e.content(), e.caret()), ("ab", 1));
        e.key(EditKey::Delete, None);
        assert_eq!((e.content(), e.caret()), ("a", 1));
        assert_eq!(
            e.key(EditKey::Delete, None),
            Outcome::Continue { changed: false }
        );
        e.place(0, false);
        assert_eq!(
            e.key(EditKey::Backspace, None),
            Outcome::Continue { changed: false }
        );
        let mut e = TextEditor::new("all of it");
        e.key(EditKey::Backspace, None);
        assert_eq!(e.content(), "");
    }

    #[test]
    fn arrows_move_by_character_and_collapse_a_selection() {
        let mut e = at("añb", 0);
        e.key(right(false, false, false), None);
        e.key(right(false, false, false), None);
        assert_eq!(e.caret(), 3, "steps over the 2-byte ñ");
        e.key(right(false, false, true), None);
        assert_eq!(e.selection(), 3..4);
        e.key(plain(), None);
        assert_eq!(
            (e.caret(), e.selection()),
            (3, 3..3),
            "collapses to the start"
        );
        e.key(right(false, false, false), None);
        e.key(right(false, false, false), None);
        assert_eq!(e.caret(), 4, "stops at the end");
    }

    #[test]
    fn option_arrows_move_by_word() {
        let s = "Buy now, save 20%";
        let mut e = at(s, 0);
        e.key(right(true, false, false), None);
        assert_eq!(e.caret(), 3);
        e.key(right(true, false, false), None);
        assert_eq!(e.caret(), 7, "end of 'now', before the comma");
        e.key(right(true, false, false), None);
        assert_eq!(e.caret(), 13);
        e.key(left(true, false, false), None);
        assert_eq!(e.caret(), 9, "start of 'save'");
        e.key(left(true, false, true), None);
        assert_eq!(e.selection(), 4..9);
        e.place(s.len(), false);
        e.key(right(true, false, false), None);
        assert_eq!(e.caret(), s.len());
    }

    #[test]
    fn command_arrows_go_to_the_line_ends() {
        let mut e = at("one\ntwo", 5);
        e.key(left(false, true, false), None);
        assert_eq!(e.caret(), 4);
        e.key(right(false, true, true), None);
        assert_eq!(e.selection(), 4..7);
    }

    #[test]
    fn select_all_then_type() {
        let mut e = at("old label", 3);
        e.key(EditKey::SelectAll, None);
        assert_eq!(e.selection(), 0..9);
        typed(&mut e, "new");
        assert_eq!(e.content(), "new");
    }

    #[test]
    fn copy_cut_and_paste_plain_text() {
        let mut e = at("Buy now", 0);
        e.key(right(true, false, true), None);
        assert_eq!(
            e.key(EditKey::Copy, None),
            Outcome::Clipboard {
                text: "Buy".into(),
                changed: false
            }
        );
        assert_eq!(
            e.key(EditKey::Cut, None),
            Outcome::Clipboard {
                text: "Buy".into(),
                changed: true
            }
        );
        assert_eq!(e.content(), " now");
        e.key(right(false, true, false), None);
        e.key(EditKey::Insert(" Sell\r\nit\tnow".into()), None);
        assert_eq!(e.content(), " now Sell\nitnow");
        assert_eq!(
            e.key(EditKey::Copy, None),
            Outcome::Continue { changed: false },
            "nothing selected, nothing copied"
        );
    }

    #[test]
    fn content_never_exceeds_the_limit() {
        let mut e = at(&"a".repeat(MAX_TEXT_CHARS - 2), MAX_TEXT_CHARS - 2);
        e.key(EditKey::Insert("ñññññ".into()), None);
        assert_eq!(e.content().chars().count(), MAX_TEXT_CHARS);
        assert_eq!(
            e.key(EditKey::Insert("x".into()), None),
            Outcome::Continue { changed: false }
        );
        // Replacing a selection frees its room first.
        e.key(left(false, false, true), None);
        e.key(EditKey::Insert("xy".into()), None);
        assert_eq!(e.content().chars().count(), MAX_TEXT_CHARS);
        assert!(e.content().ends_with('x'));
    }

    #[test]
    fn place_clamps_to_a_character_boundary() {
        let mut e = TextEditor::new("ñ");
        e.place(1, false);
        assert_eq!(e.caret(), 0);
        e.place(99, false);
        assert_eq!(e.caret(), 2);
    }

    #[test]
    fn up_and_down_keep_the_column() {
        let db = Arc::new(FontDb::from_data([REGULAR.to_vec()]));
        let content = "HHHH\nH\nHHHH";
        let l = layout(
            &Text {
                size: 100.0,
                ..Text::new(content, "Noto Sans")
            },
            &db,
        );
        let mut e = at(content, 3); // after "HHH"
        e.key(EditKey::Down { extend: false }, Some(&l));
        assert_eq!(e.caret(), 6, "end of the short line");
        e.key(EditKey::Down { extend: false }, Some(&l));
        assert_eq!(e.caret(), 10, "back to the remembered column");
        e.key(EditKey::Down { extend: false }, Some(&l));
        assert_eq!(e.caret(), content.len(), "past the last line: the end");
        e.key(EditKey::Up { extend: true }, Some(&l));
        assert_eq!(e.selection(), 6..content.len());
        e.place(2, false);
        e.key(EditKey::Up { extend: false }, Some(&l));
        assert_eq!(e.caret(), 0, "above the first line: the start");
        e.key(EditKey::Down { extend: false }, None);
        assert_eq!(e.caret(), content.len(), "without a layout: the end");
    }
}
