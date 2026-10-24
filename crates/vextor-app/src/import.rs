//! SVG and image import (spec 13): what a file or the clipboard holds,
//! where it goes and how it lands in a document. The parsing itself is
//! `vextor_svg::import_svg` / `import_image`; the window runs it off the UI
//! thread (`app/import_flow.rs`).

use std::path::{Path, PathBuf};

use vextor_core::{Point, Transform};
use vextor_geom::Fonts;
use vextor_svg::{ImportError, Imported};

use crate::edit_ops;
use crate::files;
use crate::session::Session;

/// Extensions File ▸ Import… offers and a drop accepts as an import.
pub const SVG_EXTENSIONS: [&str; 1] = ["svg"];
pub const IMAGE_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// What a dropped or opened file is, by its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Document,
    Svg,
    Image,
    Other,
}

impl FileKind {
    #[must_use]
    pub fn of(path: &Path) -> Self {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .unwrap_or_default();
        if files::has_document_extension(path) {
            Self::Document
        } else if SVG_EXTENSIONS.contains(&ext.as_str()) {
            Self::Svg
        } else if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            Self::Image
        } else {
            Self::Other
        }
    }
}

/// Reads and imports the SVG or image at `path` (any thread).
///
/// # Errors
/// A one-line reason naming the file.
pub fn read(path: &Path, kind: FileKind, fonts: &Fonts) -> Result<Imported, String> {
    let name = files::display_name(path);
    let bytes = files::read_limited(path).map_err(|e| format!("Couldn't import “{name}”: {e}"))?;
    parse(&bytes, kind, fonts).map_err(|e| format!("Couldn't import “{name}”: {e}"))
}

/// Imports `bytes` as `kind` (an image is recognised by its content too).
///
/// # Errors
/// Why the content cannot be imported.
pub fn parse(bytes: &[u8], kind: FileKind, fonts: &Fonts) -> Result<Imported, ImportError> {
    match kind {
        FileKind::Image => vextor_svg::import_image(bytes),
        FileKind::Svg | FileKind::Document | FileKind::Other => {
            vextor_svg::import_svg(bytes, fonts)
        }
    }
}

/// A bitmap pasted from the clipboard (straight RGBA) as one embedded PNG
/// image (any thread).
///
/// # Errors
/// A one-line reason.
pub fn pasted_image(width: u32, height: u32, rgba: Vec<u8>) -> Result<Imported, String> {
    let png = vextor_render::rgba_png(width, height, rgba)
        .ok_or_else(|| format!("Couldn't paste the image: {}", ImportError::ImageTooLarge))?;
    vextor_svg::import_image(&png).map_err(|e| format!("Couldn't paste the image: {e}"))
}

/// What ⌘V does with the system clipboard's `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasteChoice {
    /// The internal clipboard: it is what vextor itself copied last, or
    /// nothing on the system clipboard can be imported.
    Internal,
    /// SVG copied from somewhere else: import it.
    Svg,
    /// SVG or image files copied in the file manager (the text is only
    /// their names): import them.
    Files(Vec<PathBuf>),
}

/// Picks what ⌘V pastes: the internal clipboard keeps everything vextor
/// knows (names, ids, live shapes), so it wins whenever the system text is
/// the SVG vextor put there itself. `files` are the files on the system
/// clipboard, read only when the text is neither (`files` is called once
/// at most).
#[must_use]
pub fn paste_choice(
    text: &str,
    last_copied: Option<&str>,
    internal: bool,
    files: impl FnOnce() -> Vec<PathBuf>,
) -> PasteChoice {
    if internal && last_copied == Some(text) {
        return PasteChoice::Internal;
    }
    if vextor_svg::looks_like_svg(text) {
        return PasteChoice::Svg;
    }
    let importable: Vec<PathBuf> = files()
        .into_iter()
        .filter(|p| matches!(FileKind::of(p), FileKind::Svg | FileKind::Image))
        .collect();
    if importable.is_empty() {
        PasteChoice::Internal
    } else {
        PasteChoice::Files(importable)
    }
}

/// Places `imported` in the active document with its centre at `at` (in
/// document space), as one undo step named `label`, and selects it. Several
/// objects come in as one group named `name`. Returns the message to show:
/// the summary when something was left out.
///
/// # Errors
/// A message for the user when there is nowhere to put it.
pub fn place(
    s: &mut Session,
    imported: Imported,
    at: Point,
    name: Option<&str>,
    label: &'static str,
) -> Result<Option<String>, String> {
    let imported = vextor_svg::grouped(imported, name);
    let (parent, index) =
        edit_ops::paste_target(s).ok_or("Show or unlock a layer to import into.")?;
    let (w, h) = imported.size;
    let offset = Transform::translate(at.x - w / 2.0, at.y - h / 2.0);
    let (cmd, roots) = s
        .doc
        .paste(&imported.payload, parent, index, offset, label)
        .map_err(|e| format!("Couldn't import: {e}"))?;
    s.apply(cmd).map_err(|e| format!("Couldn't import: {e}"))?;
    s.select(roots);
    Ok((!imported.report.is_clean()).then(|| imported.report.summary()))
}

/// A new untitled document holding `imported` (File ▸ Open of an SVG, or a
/// drop with no document open), named after the file.
///
/// # Errors
/// Why it cannot be built.
pub fn document(imported: &Imported, path: &Path) -> Result<Session, String> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(vextor_core::clean_name)
        .unwrap_or_else(|| "Imported".to_owned());
    let doc = vextor_svg::into_document(imported, &name)
        .map_err(|e| format!("Couldn't open “{}”: {e}", files::display_name(path)))?;
    Ok(Session::new(doc))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use vextor_core::{Document, NodeKind};

    use super::*;

    const ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
        <rect width="10" height="10" fill="#f00"/><circle cx="18" cy="18" r="4"/>
        <rect width="4" height="4" filter="url(#f)"/><filter id="f"><feFlood/></filter></svg>"##;

    fn session() -> Session {
        Session::new(Document::new("Test"))
    }

    #[test]
    fn files_are_told_apart_by_extension() {
        let kind = |p: &str| FileKind::of(&PathBuf::from(p));
        assert_eq!(kind("a.vxtr"), FileKind::Document);
        assert_eq!(kind("icon.SVG"), FileKind::Svg);
        assert_eq!(kind("photo.JPeG"), FileKind::Image);
        assert_eq!(kind("x.webp"), FileKind::Image);
        assert_eq!(kind("notes.txt"), FileKind::Other);
        assert_eq!(kind("no_extension"), FileKind::Other);
    }

    #[test]
    fn paste_prefers_what_vextor_copied_itself() {
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
        let none = Vec::new;
        assert_eq!(
            paste_choice(svg, Some(svg), true, none),
            PasteChoice::Internal
        );
        assert_eq!(
            paste_choice(svg, Some("other"), true, none),
            PasteChoice::Svg
        );
        // Nothing of ours to paste: the SVG is imported even if we wrote it.
        assert_eq!(paste_choice(svg, Some(svg), false, none), PasteChoice::Svg);
        assert_eq!(
            paste_choice("hello", None, true, none),
            PasteChoice::Internal
        );
    }

    #[test]
    fn files_copied_in_the_file_manager_are_imported() {
        let copied = || {
            ["/a/photo.JPG", "/a/notes.txt", "/a/icon.svg", "/a/doc.vxtr"]
                .map(PathBuf::from)
                .to_vec()
        };
        assert_eq!(
            paste_choice("photo.JPG", None, false, copied),
            PasteChoice::Files(vec![
                PathBuf::from("/a/photo.JPG"),
                PathBuf::from("/a/icon.svg")
            ])
        );
        let text_only = || vec![PathBuf::from("/a/notes.txt")];
        assert_eq!(
            paste_choice("notes.txt", None, true, text_only),
            PasteChoice::Internal
        );
        // Our own copy never looks at the files.
        let ours = "<svg/>";
        let unread = || -> Vec<PathBuf> { panic!("files read") };
        assert_eq!(
            paste_choice(ours, Some(ours), true, unread),
            PasteChoice::Internal
        );
    }

    #[test]
    fn an_import_lands_as_one_selected_group_centred_and_undoable() {
        let mut s = session();
        let before = s.doc.clone();
        let imported = parse(ICON.as_bytes(), FileKind::Svg, &Fonts::default()).expect("svg");
        let message = place(
            &mut s,
            imported,
            Point::new(100.0, 50.0),
            Some("icon"),
            "Import",
        )
        .expect("placed");
        assert_eq!(
            message.as_deref(),
            Some("Imported 3 objects. Left out: 1 filter.")
        );
        let selected = s.selected();
        assert_eq!(selected.len(), 1);
        let group = &s.doc.nodes[&selected[0]];
        assert_eq!(group.name.as_deref(), Some("icon"));
        assert!(matches!(group.kind, NodeKind::Group(_)));
        // The 24 × 24 viewport is centred on the point.
        let corner = group.transform.apply(Point::new(0.0, 0.0));
        assert_eq!((corner.x, corner.y), (88.0, 38.0));
        assert!(s.undo());
        assert_eq!(s.doc, before);
    }

    #[test]
    fn a_clean_import_shows_no_message() {
        let mut s = session();
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><rect width="4" height="4"/></svg>"#;
        let imported = parse(svg.as_bytes(), FileKind::Svg, &Fonts::default()).expect("svg");
        let message =
            place(&mut s, imported, Point::new(0.0, 0.0), None, "Paste SVG").expect("placed");
        assert_eq!(message, None);
        let one = &s.doc.nodes[&s.selected()[0]];
        assert!(
            matches!(one.kind, NodeKind::Shape(_)),
            "one object is not grouped"
        );
    }

    #[test]
    fn an_svg_opens_as_an_untitled_document_named_after_the_file() {
        let imported = parse(ICON.as_bytes(), FileKind::Svg, &Fonts::default()).expect("svg");
        let s = document(&imported, Path::new("/tmp/My Icon.svg")).expect("document");
        assert_eq!(s.doc.name, "My Icon");
        assert_eq!(s.doc.artboards[0].rect.width, 24.0);
        assert!(s.path().is_none() && !s.is_dirty());
    }

    #[test]
    fn a_pasted_bitmap_lands_as_an_image_at_its_pixel_size() {
        let mut s = session();
        let imported = pasted_image(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).expect("png");
        place(
            &mut s,
            imported,
            Point::new(10.0, 10.0),
            None,
            "Paste image",
        )
        .expect("placed");
        let node = &s.doc.nodes[&s.selected()[0]];
        let NodeKind::Image(image) = &node.kind else {
            panic!("an image")
        };
        assert_eq!((image.rect.width, image.rect.height), (2.0, 1.0));
        let vextor_core::ImageSource::Embedded { mime, .. } = &image.source;
        assert_eq!(*mime, vextor_core::ImageMime::Png);
        assert!(
            pasted_image(1, 1, vec![0; 3]).is_err(),
            "pixel count must match"
        );
    }

    #[test]
    fn a_locked_document_says_where_to_import() {
        let mut s = session();
        let layer = s.doc.layers[0];
        s.doc.nodes.get_mut(&layer).expect("layer").locked = true;
        let imported = parse(ICON.as_bytes(), FileKind::Svg, &Fonts::default()).expect("svg");
        let err = place(&mut s, imported, Point::new(0.0, 0.0), None, "Import").unwrap_err();
        assert_eq!(err, "Show or unlock a layer to import into.");
    }
}
