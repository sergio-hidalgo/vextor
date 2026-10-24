//! File ▸ Export… (spec 12): what can be exported, the files that makes
//! and their names. No window and no disk here; the app asks where and
//! writes (`app/export_flow.rs`).
//!
//! File names come from artboard names, cleaned so they are a plain file
//! name on every system (`security-check` §4); the folder comes from the
//! save dialog only.

use vextor_core::ArtboardId;
use vextor_svg::{Format, Scope};

use crate::session::Session;

/// What File ▸ Export… writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    /// The artboard the code panel and inspector show.
    Artboard,
    Selection,
    /// One file per artboard, into a folder.
    AllArtboards,
}

/// The file type, and for PNG the pixels per document px.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Svg,
    Png(u8),
}

impl Kind {
    /// The choices the dialog offers, in order.
    pub const ALL: [Self; 4] = [Self::Svg, Self::Png(1), Self::Png(2), Self::Png(3)];

    pub fn title(self) -> String {
        match self {
            Self::Svg => "SVG".to_owned(),
            Self::Png(scale) => format!("PNG {scale}×"),
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png(_) => "png",
        }
    }

    /// `stem` with the scale suffix and extension: `icon.svg`,
    /// `icon.png`, `icon@2x.png`.
    pub fn file_name(self, stem: &str) -> String {
        match self {
            Self::Png(scale) if scale > 1 => format!("{stem}@{scale}x.png"),
            _ => format!("{stem}.{}", self.extension()),
        }
    }
}

/// One file to write: its name (no folder) and content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Longest file stem made from an artboard name, in characters.
const MAX_STEM_CHARS: usize = 64;

/// The artboard the code panel and File ▸ Export… use: the one picked in
/// the inspector, else the first.
pub fn current_artboard(s: &Session) -> Option<ArtboardId> {
    s.artboard
        .filter(|id| s.doc.artboards.iter().any(|a| a.id == *id))
        .or_else(|| s.doc.artboards.first().map(|a| a.id))
}

/// Why `what` as `kind` cannot be exported now, if it cannot.
pub fn why_not(s: &Session, what: What, kind: Kind) -> Option<&'static str> {
    match what {
        What::Selection if kind != Kind::Svg => Some("PNG export works on artboards"),
        What::Selection if s.selected().is_empty() => Some("Select objects first"),
        What::Artboard | What::AllArtboards if s.doc.artboards.is_empty() => {
            Some("Add an artboard first")
        }
        _ => None,
    }
}

/// The files `what` as `kind` makes, named after their artboards (or the
/// document, for a selection).
///
/// # Errors
/// A message for the user: nothing to export, or a PNG that could not be
/// made.
pub fn files(s: &Session, what: What, kind: Kind) -> Result<Vec<File>, String> {
    if let Some(why) = why_not(s, what, kind) {
        return Err(why.to_owned());
    }
    let artboards: Vec<ArtboardId> = match what {
        What::Artboard => current_artboard(s).into_iter().collect(),
        What::AllArtboards => s.doc.artboards.iter().map(|a| a.id).collect(),
        What::Selection => {
            let sel = s.selected();
            let svg = svg(s, Scope::Selection(&sel)).ok_or("The selection paints nothing")?;
            let name = kind.file_name(&file_stem(&s.doc.name));
            return Ok(vec![File { name, bytes: svg }]);
        }
    };
    let names: Vec<String> = artboards
        .iter()
        .filter_map(|id| s.doc.artboards.iter().find(|a| a.id == *id))
        .map(|a| file_stem(&a.name))
        .collect();
    let mut out = Vec::with_capacity(artboards.len());
    for (id, stem) in artboards.into_iter().zip(unique(names)) {
        let bytes = match kind {
            Kind::Svg => svg(s, Scope::Artboard(id)).ok_or("The artboard does not exist")?,
            Kind::Png(scale) => vextor_render::artboard_png(&s.doc, &s.fonts, id, f64::from(scale))
                .map_err(|e| format!("The PNG could not be made: {e}"))?,
        };
        out.push(File {
            name: kind.file_name(&stem),
            bytes,
        });
    }
    Ok(out)
}

fn svg(s: &Session, scope: Scope<'_>) -> Option<Vec<u8>> {
    vextor_svg::export(&s.doc, &s.fonts, scope, Format::Svg, &s.doc.export)
        .map(|e| e.code.into_bytes())
}

/// `name` as a file name without extension that is safe on macOS, Windows
/// and Linux: letters, digits, spaces, `-`, `_` and `.` kept, anything
/// else (separators, `:`, control characters…) as `-`; no leading or
/// trailing dots or spaces; Windows device names get a `_` in front; at
/// most [`MAX_STEM_CHARS`] characters; `artboard` when nothing is left.
pub fn file_stem(name: &str) -> String {
    let kept: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .take(MAX_STEM_CHARS)
        .collect();
    let stem = kept.trim_matches(|c: char| c == '.' || c.is_whitespace());
    if stem.is_empty() {
        return "artboard".to_owned();
    }
    let device = stem.split('.').next().unwrap_or(stem).to_ascii_uppercase();
    let reserved = matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (device.len() == 4
            && (device.starts_with("COM") || device.starts_with("LPT"))
            && device.ends_with(|c: char| c.is_ascii_digit() && c != '0'));
    if reserved {
        format!("_{stem}")
    } else {
        stem.to_owned()
    }
}

/// `stems` with repeats (ignoring letter case, as macOS and Windows do)
/// numbered: `icon`, `icon-2`, `icon-3`.
fn unique(stems: Vec<String>) -> Vec<String> {
    let mut taken: Vec<String> = Vec::with_capacity(stems.len());
    let mut out = Vec::with_capacity(stems.len());
    for stem in stems {
        let mut name = stem.clone();
        let mut n = 2;
        while taken.contains(&name.to_lowercase()) {
            name = format!("{stem}-{n}");
            n += 1;
        }
        taken.push(name.to_lowercase());
        out.push(name);
    }
    out
}

#[cfg(test)]
mod tests {
    use vextor_core::{Artboard, Command, Rect};

    use super::*;
    use crate::new_doc::new_document;

    #[test]
    fn hostile_artboard_names_become_plain_file_names() {
        assert_eq!(file_stem("button-primary"), "button-primary");
        assert_eq!(file_stem("../../etc/passwd"), "-..-etc-passwd");
        assert_eq!(file_stem("a/b\\c:d*e?f\"g<h>i|j"), "a-b-c-d-e-f-g-h-i-j");
        assert_eq!(file_stem("  .hidden.  "), "hidden");
        assert_eq!(file_stem("tab\there\u{0}"), "tab-here-");
        assert_eq!(file_stem("Ícono 24"), "Ícono 24");
        assert_eq!(file_stem("..."), "artboard");
        assert_eq!(file_stem("con"), "_con");
        assert_eq!(file_stem("LPT1.icon"), "_LPT1.icon");
        assert_eq!(file_stem("COM0"), "COM0");
        assert_eq!(file_stem(&"x".repeat(100)).len(), MAX_STEM_CHARS);
    }

    #[test]
    fn repeated_names_are_numbered_ignoring_case() {
        let names = ["icon", "Icon", "icon", "card"].map(str::to_owned).to_vec();
        assert_eq!(unique(names), ["icon", "Icon-2", "icon-3", "card"]);
    }

    #[test]
    fn png_files_carry_their_scale() {
        assert_eq!(Kind::Svg.file_name("icon"), "icon.svg");
        assert_eq!(Kind::Png(1).file_name("icon"), "icon.png");
        assert_eq!(Kind::Png(3).file_name("icon"), "icon@3x.png");
    }

    fn two_artboards() -> Session {
        let mut doc = new_document("Pricing", 24.0, 24.0);
        let mut artboards = doc.artboards.clone();
        let id = doc.alloc_artboard_id();
        artboards[0].name = "icon".to_owned();
        artboards.push(Artboard {
            id,
            name: "Icon".to_owned(),
            rect: Rect::new(40.0, 0.0, 24.0, 24.0),
            background: None,
        });
        doc.apply(Command::SetArtboards(artboards)).expect("valid");
        Session::new(doc)
    }

    #[test]
    fn every_artboard_is_one_file_with_its_own_name() {
        let s = two_artboards();
        let svgs = files(&s, What::AllArtboards, Kind::Svg).expect("exports");
        let names: Vec<&str> = svgs.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["icon.svg", "Icon-2.svg"]);
        assert!(svgs[0].bytes.starts_with(b"<svg xmlns="));
        let pngs = files(&s, What::AllArtboards, Kind::Png(2)).expect("exports");
        assert_eq!(pngs[1].name, "Icon-2@2x.png");
        assert!(pngs[1].bytes.starts_with(b"\x89PNG"));
    }

    #[test]
    fn a_selection_needs_objects_and_svg() {
        let s = two_artboards();
        assert_eq!(
            files(&s, What::Selection, Kind::Svg),
            Err("Select objects first".to_owned())
        );
        assert_eq!(
            why_not(&s, What::Selection, Kind::Png(1)),
            Some("PNG export works on artboards")
        );
        assert_eq!(why_not(&s, What::Artboard, Kind::Png(1)), None);
    }

    #[test]
    fn the_current_artboard_is_the_picked_one_else_the_first() {
        let mut s = two_artboards();
        let ids: Vec<ArtboardId> = s.doc.artboards.iter().map(|a| a.id).collect();
        assert_eq!(current_artboard(&s), Some(ids[0]));
        s.artboard = Some(ids[1]);
        assert_eq!(current_artboard(&s), Some(ids[1]));
        let one = files(&s, What::Artboard, Kind::Svg).expect("exports");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].name, "Icon.svg", "alone it needs no number");
    }
}
