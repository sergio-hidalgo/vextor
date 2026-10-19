//! Renaming an artboard and setting its background (spec 05e): the rules
//! shared by the canvas label and Inspector ▸ Artboard.

use vextor_core::{Artboard, ArtboardId, Command, Rgba};

use crate::session::Session;

/// Longest artboard name, in characters (it becomes a file name, spec 12).
pub const MAX_NAME_CHARS: usize = 64;
/// The colour a background starts at when it is turned on.
pub const DEFAULT_BACKGROUND: Rgba = Rgba::rgb(0xFF, 0xFF, 0xFF);

/// The artboard Inspector ▸ Artboard shows. A document has one artboard
/// until spec 08 adds more.
#[must_use]
pub fn current(s: &Session) -> Option<&Artboard> {
    s.doc.artboards.first()
}

/// `input` as an artboard name: control characters (tabs, line breaks)
/// removed, trimmed and cut to [`MAX_NAME_CHARS`]; `None` when nothing is
/// left.
#[must_use]
pub fn clean_name(input: &str) -> Option<String> {
    let name: String = input
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_NAME_CHARS)
        .collect();
    let name = name.trim_end();
    (!name.is_empty()).then(|| name.to_owned())
}

/// Renames artboard `id` to `input` (see [`clean_name`]) as one undo step.
/// Returns false when the name was refused or did not change.
pub fn rename(s: &mut Session, id: ArtboardId, input: &str) -> bool {
    let Some(name) = clean_name(input) else {
        return false;
    };
    edit(s, id, "Rename artboard", |a| a.name = name)
}

/// Sets artboard `id`'s background (`None` = transparent) as one undo
/// step; inside a gesture, consecutive changes merge into one. Returns
/// false when nothing changed.
pub fn set_background(s: &mut Session, id: ArtboardId, background: Option<Rgba>) -> bool {
    edit(s, id, "Artboard background", |a| a.background = background)
}

fn edit(
    s: &mut Session,
    id: ArtboardId,
    label: &'static str,
    change: impl FnOnce(&mut Artboard),
) -> bool {
    let mut artboards = s.doc.artboards.clone();
    let Some(a) = artboards.iter_mut().find(|a| a.id == id) else {
        return false;
    };
    let before = a.clone();
    change(a);
    if *a == before {
        return false;
    }
    s.apply(Command::Batch {
        label,
        commands: vec![Command::SetArtboards(artboards)],
    })
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_doc::new_document;

    fn session() -> (Session, ArtboardId) {
        let s = Session::new(new_document("card", 320.0, 200.0));
        let id = s.doc.artboards[0].id;
        (s, id)
    }

    fn name(s: &Session) -> &str {
        &s.doc.artboards[0].name
    }

    fn background(s: &Session) -> Option<Rgba> {
        s.doc.artboards[0].background
    }

    #[test]
    fn rename_commits_trimmed_text_as_one_undo_step() {
        let (mut s, id) = session();
        assert!(rename(&mut s, id, "  pricing-card \n"));
        assert_eq!(name(&s), "pricing-card");
        assert_eq!(s.edit_menu().undo, Some("Rename artboard"));
        assert!(s.undo());
        assert_eq!(name(&s), "card");
        assert!(!s.undo());
    }

    #[test]
    fn empty_or_whitespace_name_is_refused() {
        let (mut s, id) = session();
        assert!(!rename(&mut s, id, ""));
        assert!(!rename(&mut s, id, "  \t "));
        assert_eq!(name(&s), "card");
        assert_eq!(s.edit_menu().undo, None);
    }

    #[test]
    fn long_names_are_cut_to_64_characters() {
        let (mut s, id) = session();
        assert!(rename(&mut s, id, &"ñ".repeat(70)));
        assert_eq!(name(&s).chars().count(), MAX_NAME_CHARS);
        // Cutting never leaves a trailing space.
        let spaced = format!("{} b", "a".repeat(63));
        assert_eq!(clean_name(&spaced).as_deref(), Some(&*"a".repeat(63)));
    }

    #[test]
    fn control_characters_are_removed() {
        assert_eq!(
            clean_name("pricing\tcard\u{7}").as_deref(),
            Some("pricingcard")
        );
        assert_eq!(clean_name("\n\r"), None);
    }

    #[test]
    fn same_name_adds_no_undo_step() {
        let (mut s, id) = session();
        assert!(!rename(&mut s, id, " card "));
        assert_eq!(s.edit_menu().undo, None);
    }

    #[test]
    fn unknown_artboard_changes_nothing() {
        let (mut s, _) = session();
        assert!(!rename(&mut s, ArtboardId(9_999), "x"));
        assert!(!set_background(&mut s, ArtboardId(9_999), None));
    }

    #[test]
    fn background_on_change_off_are_undo_steps() {
        let (mut s, id) = session();
        let blue = Rgba::rgb(30, 144, 255);
        assert!(set_background(&mut s, id, Some(DEFAULT_BACKGROUND)));
        assert_eq!(background(&s), Some(DEFAULT_BACKGROUND));
        assert!(set_background(&mut s, id, Some(blue)));
        assert_eq!(background(&s), Some(blue));
        assert!(set_background(&mut s, id, None));
        assert_eq!(background(&s), None);
        assert_eq!(s.edit_menu().undo, Some("Artboard background"));

        assert!(s.undo());
        assert_eq!(background(&s), Some(blue));
        assert!(s.undo());
        assert_eq!(background(&s), Some(DEFAULT_BACKGROUND));
        assert!(s.undo());
        assert_eq!(background(&s), None);
        assert!(!s.undo());
    }

    #[test]
    fn colour_picker_drag_is_one_undo_step() {
        let (mut s, id) = session();
        s.begin_gesture();
        for g in [10, 20, 30] {
            assert!(set_background(&mut s, id, Some(Rgba::rgb(0, g, 0))));
        }
        s.end_gesture();
        assert!(s.undo());
        assert_eq!(background(&s), None);
        assert!(!s.undo());
    }

    #[test]
    fn name_and_background_survive_save_and_open() {
        let (mut s, id) = session();
        rename(&mut s, id, "pricing-card");
        set_background(&mut s, id, Some(Rgba::rgb(1, 2, 3)));
        let bytes = vextor_core::to_bytes(&s.doc).expect("saves");
        let back = vextor_core::from_bytes(&bytes).expect("opens");
        assert_eq!(back.artboards, s.doc.artboards);
    }
}
