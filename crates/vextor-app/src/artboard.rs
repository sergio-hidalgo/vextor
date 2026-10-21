//! Renaming an artboard and setting its background (spec 05e), and
//! resizing it from Inspector ▸ Artboard (spec 06 review): the rules
//! shared by the canvas label and the inspector. Several artboards
//! (spec 08): adding, deleting, reordering, and which one is current.
//! Moving one, with the objects on it (spec 08b): `moving`.

use vextor_core::{Artboard, ArtboardId, Command, Rgba};

use crate::session::Session;

mod moving;

pub use moving::{ArtboardDrag, move_to};

/// Longest artboard name, in characters (it becomes a file name, spec 12).
pub const MAX_NAME_CHARS: usize = 64;
/// The colour a background starts at when it is turned on.
pub const DEFAULT_BACKGROUND: Rgba = Rgba::rgb(0xFF, 0xFF, 0xFF);

/// The artboard Inspector ▸ Artboard shows: the one last picked (its
/// label clicked, or chosen in the inspector's list), else the first.
#[must_use]
pub fn current(s: &Session) -> Option<&Artboard> {
    s.artboard
        .and_then(|id| s.doc.artboards.iter().find(|a| a.id == id))
        .or_else(|| s.doc.artboards.first())
}

/// Makes artboard `id` the current one.
pub fn pick(s: &mut Session, id: ArtboardId) {
    if s.doc.artboards.iter().any(|a| a.id == id) {
        s.artboard = Some(id);
    }
}

/// Space between a new artboard and the ones before it, in px.
pub const ADD_GAP: f64 = 40.0;

/// Adds a transparent `width` × `height` artboard named `name` to the
/// right of the others, top-aligned with them, as one undo step; it
/// becomes current and the view fits all artboards. Returns false when
/// the size was refused.
pub fn add(s: &mut Session, name: &str, width: f64, height: f64) -> bool {
    let (x, y) = s
        .doc
        .artboards
        .iter()
        .map(|a| (a.rect.x + a.rect.width + ADD_GAP, a.rect.y))
        .reduce(|(x, y), (ax, ay)| (x.max(ax), y.min(ay)))
        .unwrap_or((0.0, 0.0));
    let id = s.doc.alloc_artboard_id();
    let mut artboards = s.doc.artboards.clone();
    artboards.push(Artboard {
        id,
        name: clean_name(name).unwrap_or_else(|| "artboard".to_owned()),
        rect: vextor_core::Rect::new(x, y, width, height),
        background: None,
    });
    let done = s
        .apply(Command::Batch {
            label: "Add artboard",
            commands: vec![Command::SetArtboards(artboards)],
        })
        .is_ok();
    if done {
        s.artboard = Some(id);
        s.needs_fit = true;
    }
    done
}

/// Deletes artboard `id` as one undo step; the objects on it stay where
/// they are. The last artboard cannot be deleted.
///
/// # Errors
/// A message for the user when it was refused.
pub fn delete(s: &mut Session, id: ArtboardId) -> Result<(), String> {
    if s.doc.artboards.len() <= 1 {
        return Err("A document keeps at least one artboard".to_owned());
    }
    let artboards: Vec<Artboard> = s
        .doc
        .artboards
        .iter()
        .filter(|a| a.id != id)
        .cloned()
        .collect();
    if artboards.len() == s.doc.artboards.len() {
        return Ok(());
    }
    s.apply(Command::Batch {
        label: "Delete artboard",
        commands: vec![Command::SetArtboards(artboards)],
    })
    .map_err(|e| format!("Delete artboard failed: {e}"))?;
    if s.artboard == Some(id) {
        s.artboard = None;
    }
    Ok(())
}

/// Moves artboard `id` one place earlier (`up`) or later in the order
/// (the order of export, spec 12), as one undo step. Returns false at
/// either end.
pub fn reorder(s: &mut Session, id: ArtboardId, up: bool) -> bool {
    let mut artboards = s.doc.artboards.clone();
    let Some(i) = artboards.iter().position(|a| a.id == id) else {
        return false;
    };
    let j = if up {
        let Some(j) = i.checked_sub(1) else {
            return false;
        };
        j
    } else if i + 1 < artboards.len() {
        i + 1
    } else {
        return false;
    };
    artboards.swap(i, j);
    s.apply(Command::Batch {
        label: "Reorder artboards",
        commands: vec![Command::SetArtboards(artboards)],
    })
    .is_ok()
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

/// Smallest artboard side, in px.
pub const MIN_SIDE: f64 = 1.0;

/// Resizes artboard `id` to `width` × `height` (each clamped to
/// [`MIN_SIDE`]..=[`crate::new_doc::MAX_SIDE`]) about its centre, as one
/// undo step; inside a gesture, consecutive changes merge into one. The
/// objects on it stay where they are. Returns false when nothing changed.
pub fn resize(s: &mut Session, id: ArtboardId, width: f64, height: f64) -> bool {
    if !(width.is_finite() && height.is_finite()) {
        return false;
    }
    let side = |v: f64| v.clamp(MIN_SIDE, crate::new_doc::MAX_SIDE);
    let (w, h) = (side(width), side(height));
    edit(s, id, "Resize artboard", |a| {
        let r = a.rect;
        let (cx, cy) = (r.x + r.width / 2.0, r.y + r.height / 2.0);
        a.rect = vextor_core::Rect::new(cx - w / 2.0, cy - h / 2.0, w, h);
    })
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

    fn names(s: &Session) -> Vec<&str> {
        s.doc.artboards.iter().map(|a| a.name.as_str()).collect()
    }

    #[test]
    fn add_puts_the_artboard_to_the_right_and_makes_it_current() {
        let (mut s, first) = session();
        assert!(add(&mut s, "icon-24", 24.0, 24.0));
        assert_eq!(names(&s), ["card", "icon-24"]);
        let r = s.doc.artboards[1].rect;
        assert!((r.x - (320.0 + ADD_GAP)).abs() < 1e-9 && r.y.abs() < 1e-9);
        assert_eq!(current(&s).map(|a| a.name.as_str()), Some("icon-24"));
        assert_eq!(s.edit_menu().undo, Some("Add artboard"));
        pick(&mut s, first);
        assert_eq!(current(&s).map(|a| a.id), Some(first));
    }

    #[test]
    fn add_delete_reorder_round_trip_through_undo() {
        let (mut s, _) = session();
        let start = s.doc.clone();
        add(&mut s, "a", 10.0, 10.0);
        add(&mut s, "b", 10.0, 10.0);
        let b = s.doc.artboards[2].id;
        assert!(reorder(&mut s, b, true));
        assert_eq!(names(&s), ["card", "b", "a"]);
        let first = s.doc.artboards[0].id;
        assert!(!reorder(&mut s, first, true), "already first");
        assert!(delete(&mut s, b).is_ok());
        assert_eq!(names(&s), ["card", "a"]);
        assert_eq!(current(&s).map(|a| a.name.as_str()), Some("card"));
        let after = s.doc.clone();
        while s.undo() {}
        assert_eq!(s.doc, start);
        while s.redo() {}
        assert_eq!(s.doc, after);
    }

    #[test]
    fn the_last_artboard_cannot_be_deleted() {
        let (mut s, id) = session();
        assert!(delete(&mut s, id).is_err());
        assert_eq!(s.doc.artboards.len(), 1);
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

    #[test]
    fn resizing_keeps_the_centre_and_the_objects() {
        let (mut s, id) = session();
        let before = s.doc.artboards[0].rect;
        let nodes = s.doc.nodes.clone();
        assert!(resize(
            &mut s,
            id,
            before.width + 40.0,
            before.height - 20.0
        ));
        let r = s.doc.artboards[0].rect;
        assert_eq!(
            (r.width, r.height),
            (before.width + 40.0, before.height - 20.0)
        );
        assert_eq!(
            (r.x, r.y),
            (before.x - 20.0, before.y + 10.0),
            "grows from the centre"
        );
        assert_eq!(s.doc.nodes, nodes, "objects stay where they are");
        assert_eq!(s.edit_menu().undo, Some("Resize artboard"));
        s.undo();
        assert_eq!(s.doc.artboards[0].rect, before);
    }

    #[test]
    fn resizing_clamps_and_refuses_nonsense() {
        let (mut s, id) = session();
        assert!(resize(&mut s, id, 0.0, 99_999.0));
        let r = s.doc.artboards[0].rect;
        assert_eq!((r.width, r.height), (MIN_SIDE, crate::new_doc::MAX_SIDE));
        assert!(!resize(&mut s, id, f64::NAN, 10.0));
        assert!(!resize(&mut s, id, r.width, r.height), "no change, no step");
    }
}
