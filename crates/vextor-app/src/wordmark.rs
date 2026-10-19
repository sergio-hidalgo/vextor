//! The "vextor" wordmark shown above the new-document picker.
//!
//! The source is `assets/wordmark.svg`: "vext" and "r" in the Poppins
//! typeface, semi-bold (SIL OFL 1.1), as outlines, with the app logo as the
//! "o". `assets/wordmark.png` is its render at [`SCALE`] pixels per point so
//! it stays sharp on high-DPI screens. After changing the SVG, regenerate the PNG:
//!
//! ```sh
//! cargo test -p vextor-app -- --ignored regenerate_wordmark_png
//! ```

use eframe::egui;

const PNG: &[u8] = include_bytes!("../assets/wordmark.png");

/// Pixels per point in the bundled PNG.
const SCALE: f32 = 3.0;

/// The wordmark as a texture, or `None` if the bundled PNG cannot be
/// decoded (the picker then shows without it).
pub fn texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let png = eframe::icon_data::from_png_bytes(PNG).ok()?;
    let size = [png.width as usize, png.height as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, &png.rgba);
    Some(ctx.load_texture("wordmark", image, egui::TextureOptions::LINEAR))
}

/// On-screen size of `texture`, in points.
pub fn size(texture: &egui::TextureHandle) -> egui::Vec2 {
    texture.size_vec2() / SCALE
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use resvg::{tiny_skia, usvg};

    use super::*;

    const SVG: &str = include_str!("../assets/wordmark.svg");

    /// On-screen height of the wordmark, in points.
    const HEIGHT: f32 = 52.0;

    /// Renders the SVG [`SCALE`] × [`HEIGHT`] px tall, keeping its aspect.
    fn render_svg() -> tiny_skia::Pixmap {
        let tree = usvg::Tree::from_str(SVG, &usvg::Options::default()).expect("SVG parses");
        let svg = tree.size();
        let scale = HEIGHT * SCALE / svg.height();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small, > 0
        let (w, h) = (
            (svg.width() * scale).ceil() as u32,
            (svg.height() * scale).round() as u32,
        );
        let mut pixmap = tiny_skia::Pixmap::new(w, h).expect("non-empty size");
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        pixmap
    }

    #[test]
    fn bundled_wordmark_decodes_at_the_display_scale() {
        let png = eframe::icon_data::from_png_bytes(PNG).expect("wordmark.png decodes");
        #[allow(clippy::cast_precision_loss)]
        let height = png.height as f32;
        assert!((height - HEIGHT * SCALE).abs() < 1.0);
        assert!(png.width > png.height * 2, "a wide wordmark");
        assert!(
            png.rgba.chunks(4).any(|px| px[3] == 255),
            "has opaque pixels"
        );
    }

    #[test]
    fn bundled_png_matches_the_svg() {
        let png = eframe::icon_data::from_png_bytes(PNG).expect("wordmark.png decodes");
        let fresh = render_svg();
        assert_eq!((png.width, png.height), (fresh.width(), fresh.height()));
        // PNG holds straight alpha, tiny-skia premultiplied; compare the
        // demultiplied render, allowing anti-aliasing noise.
        let fresh: Vec<u8> = fresh
            .pixels()
            .iter()
            .flat_map(|p| {
                let c = p.demultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            })
            .collect();
        let differing = png
            .rgba
            .chunks(4)
            .zip(fresh.chunks(4))
            .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > 8))
            .count();
        assert!(
            differing * 1000 < png.rgba.len() / 4,
            "{differing} pixels differ: regenerate wordmark.png (see module docs)"
        );
    }

    #[test]
    #[ignore = "writes assets/wordmark.png; run after changing assets/wordmark.svg"]
    fn regenerate_wordmark_png() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wordmark.png");
        render_svg()
            .save_png(&path)
            .expect("wordmark.png is writable");
    }
}
