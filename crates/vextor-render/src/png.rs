//! PNG export of an artboard (spec 12): the canvas render of its area at
//! 1×, 2× or 3×, transparent where the artboard has no background.

use vextor_core::{ArtboardId, Document, Vec2, View};
use vextor_geom::Fonts;

use crate::error::RenderError;
use crate::render::{MAX_PIXMAP_SIDE, Viewport, render};

/// Artboard `id` of `doc` as PNG bytes, `scale` pixels per document px.
///
/// # Errors
/// [`RenderError::NoArtboard`] when it does not exist; the viewport errors
/// when the image would be empty or over [`MAX_PIXMAP_SIDE`] pixels a side;
/// [`RenderError::Encode`] when PNG encoding fails.
pub fn artboard_png(
    doc: &Document,
    fonts: &Fonts,
    id: ArtboardId,
    scale: f64,
) -> Result<Vec<u8>, RenderError> {
    let artboard = doc
        .artboards
        .iter()
        .find(|a| a.id == id)
        .ok_or(RenderError::NoArtboard)?;
    let r = artboard.rect;
    let side = |len: f64| -> Result<u32, RenderError> {
        let px = (len * scale).ceil();
        if !px.is_finite() || px < 1.0 {
            return Err(RenderError::InvalidView);
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // checked range
        let px = px.min(f64::from(MAX_PIXMAP_SIDE) + 1.0) as u32;
        Ok(px)
    };
    let viewport = Viewport {
        view: View {
            zoom: scale,
            pan: Vec2::new(-r.x * scale, -r.y * scale),
        },
        width: side(r.width)?,
        height: side(r.height)?,
        pixels_per_point: 1.0,
    };
    let pixmap = render(doc, fonts, &viewport)?;
    pixmap
        .encode_png()
        .map_err(|e| RenderError::Encode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn a_2x_png_has_twice_the_artboard_size() {
        let doc = fixtures::basic_shapes();
        let id = doc.artboards[0].id;
        let png = artboard_png(&doc, &Fonts::default(), id, 2.0).expect("renders");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let decoded = tiny_skia::Pixmap::decode_png(&png).expect("decodes");
        let (w, h) = fixtures::BASIC_SHAPES_SIZE;
        assert_eq!(
            (f64::from(decoded.width()), f64::from(decoded.height())),
            (w * 2.0, h * 2.0)
        );
    }

    #[test]
    fn a_missing_or_huge_artboard_is_an_error() {
        let doc = fixtures::basic_shapes();
        let missing = artboard_png(&doc, &Fonts::default(), ArtboardId(9_999), 1.0);
        assert_eq!(missing, Err(RenderError::NoArtboard));
        let id = doc.artboards[0].id;
        let huge = artboard_png(&doc, &Fonts::default(), id, 1_000.0);
        assert!(
            matches!(huge, Err(RenderError::TooLarge { .. })),
            "{huge:?}"
        );
    }
}
