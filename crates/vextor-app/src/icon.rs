//! The app icon shown in the Dock (macOS) or the window/taskbar while
//! Vextor runs.
//!
//! The source is `assets/icon.svg`; `assets/icon.png` is its 512 px render
//! on a black rounded tile sized like other macOS icons, because the OS
//! takes pixels, not vectors. After changing the SVG, regenerate the PNG:
//!
//! ```sh
//! cargo test -p vextor-app -- --ignored regenerate_icon_png
//! ```

use eframe::egui::IconData;

const PNG: &[u8] = include_bytes!("../assets/icon.png");

/// The app icon, or `None` if the bundled PNG cannot be decoded (the OS
/// default icon is then used).
pub fn app_icon() -> Option<IconData> {
    eframe::icon_data::from_png_bytes(PNG).ok()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use resvg::{tiny_skia, usvg};

    use super::*;

    const SVG: &str = include_str!("../assets/icon.svg");

    /// Width and height of the icon render, in px.
    const SIZE: u32 = 512;

    /// Transparent border around the tile, per side (Apple's grid: 100 of
    /// 1024).
    const TILE_INSET: f32 = 50.0;

    /// Corner radius of the tile (Apple's grid: ~185 of 1024).
    const TILE_RADIUS: f32 = 92.0;

    /// Space between the tile's edge and the logo, per side, both
    /// horizontally and vertically.
    const LOGO_PADDING: f32 = 66.0;

    /// Renders the black rounded tile, then the logo centred inside it
    /// with [`LOGO_PADDING`].
    fn render_svg() -> tiny_skia::Pixmap {
        #[allow(clippy::cast_precision_loss)]
        let side = SIZE as f32;
        let tile = side - 2.0 * TILE_INSET;
        let background = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{side}" height="{side}"><rect x="{TILE_INSET}" y="{TILE_INSET}" width="{tile}" height="{tile}" rx="{TILE_RADIUS}" fill="#000"/></svg>"##
        );
        let options = usvg::Options::default();
        let background = usvg::Tree::from_str(&background, &options).expect("tile SVG parses");
        let logo = usvg::Tree::from_str(SVG, &options).expect("icon SVG parses");

        let mut pixmap = tiny_skia::Pixmap::new(SIZE, SIZE).expect("non-empty size");
        resvg::render(
            &background,
            tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        let svg = logo.size();
        let inner = tile - 2.0 * LOGO_PADDING;
        let scale = (inner / svg.width()).min(inner / svg.height());
        let dx = (side - svg.width() * scale) / 2.0;
        let dy = (side - svg.height() * scale) / 2.0;
        resvg::render(
            &logo,
            tiny_skia::Transform::from_scale(scale, scale).post_translate(dx, dy),
            &mut pixmap.as_mut(),
        );
        pixmap
    }

    #[test]
    fn bundled_icon_decodes_to_a_square_with_artwork() {
        let icon = app_icon().expect("icon.png decodes");
        assert_eq!((icon.width, icon.height), (SIZE, SIZE));
        assert!(
            icon.rgba.chunks(4).any(|px| px[3] == 255),
            "has opaque pixels"
        );
        assert_eq!(icon.rgba[3], 0, "corner is transparent (margin)");
        // Inside the tile, in the padding left of the logo: opaque black.
        let at = |x: usize, y: usize| &icon.rgba[(y * SIZE as usize + x) * 4..][..4];
        assert_eq!(at(70, 256), [0, 0, 0, 255], "black tile");
        // Just inside the tile's rounded corner: still transparent.
        assert_eq!(at(52, 52)[3], 0, "rounded corner");
    }

    #[test]
    fn bundled_png_matches_the_svg() {
        let icon = app_icon().expect("icon.png decodes");
        let fresh = render_svg();
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
        let differing = icon
            .rgba
            .chunks(4)
            .zip(fresh.chunks(4))
            .filter(|(a, b)| a.iter().zip(*b).any(|(x, y)| x.abs_diff(*y) > 8))
            .count();
        assert!(
            differing * 1000 < (SIZE * SIZE) as usize,
            "{differing} pixels differ: regenerate icon.png (see module docs)"
        );
    }

    #[test]
    #[ignore = "writes assets/icon.png; run after changing assets/icon.svg"]
    fn regenerate_icon_png() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icon.png");
        render_svg().save_png(&path).expect("icon.png is writable");
    }
}
