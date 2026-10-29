//! The card shown alone while Vextor starts (spec 14e).
//!
//! The source is `assets/launch-card.svg`, the owner's artwork with its
//! lettering as outlines. `assets/launch-card.png` is its render at 2
//! pixels per point; the rounded corners are transparent. After
//! changing the SVG, regenerate the PNG:
//!
//! ```sh
//! cargo test -p vextor-app -- --ignored regenerate_launch_card_png
//! ```

use std::time::{Duration, Instant};

use eframe::egui;

use crate::launch::{self, Phase};

const PNG: &[u8] = include_bytes!("../assets/launch-card.png");

/// The card's size on screen, in points: the SVG's view box, rounded.
pub const SIZE: egui::Vec2 = egui::vec2(477.0, 318.0);

/// The editor window's size once the card is gone, in points.
pub const EDITOR_SIZE: egui::Vec2 = egui::vec2(1280.0, 800.0);

/// The editor window's smallest size, in points.
pub const EDITOR_MIN_SIZE: egui::Vec2 = egui::vec2(800.0, 500.0);

/// The window at launch: the card alone, centred, no title bar, with
/// transparent corners. [`LaunchCard::show`] turns it into the editor.
pub fn viewport(title: &str) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(SIZE)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
}

/// Longest wait, after the switch, for the window to report the editor's
/// size; a window manager may refuse the resize, and the editor shows
/// anyway.
const GROW_WAIT: Duration = Duration::from_millis(250);

/// The editor may draw once the window has grown past the card (so a
/// launch file is fitted to the editor's canvas, not the card's), or after
/// [`GROW_WAIT`] since the switch.
fn editor_may_draw(window: egui::Vec2, since_switch: Duration) -> bool {
    (window.x > SIZE.x && window.y > SIZE.y) || since_switch >= GROW_WAIT
}

/// The card while it is up.
pub struct LaunchCard {
    started: Instant,
    texture: Option<egui::TextureHandle>,
    /// A card frame was drawn: the app is built and its window up.
    ready: bool,
    /// When the window was asked to become the editor.
    switched: Option<Instant>,
}

impl LaunchCard {
    /// The card for a launch that began at `started`.
    pub fn new(ctx: &egui::Context, started: Instant) -> Self {
        Self {
            started,
            texture: texture(ctx),
            ready: false,
            switched: None,
        }
    }

    /// Paints the card and returns `true` while it stays up. When its time
    /// is up, turns the window into the editor and keeps it plain until
    /// the window has grown; then returns `false` and the editor draws.
    pub fn show(&mut self, ui: &egui::Ui) -> bool {
        let ctx = ui.ctx();
        if let Some(at) = self.switched {
            if editor_may_draw(ui.max_rect().size(), at.elapsed()) {
                return false;
            }
            plain(ui);
            return true;
        }
        let elapsed = self.started.elapsed();
        if launch::phase(elapsed, self.ready) == Phase::Editor {
            to_editor(ctx);
            self.switched = Some(Instant::now());
            plain(ui);
            return true;
        }
        if let Some(texture) = &self.texture {
            let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
            ui.painter()
                .image(texture.id(), ui.max_rect(), uv, egui::Color32::WHITE);
        }
        self.ready = true;
        ctx.request_repaint_after(launch::CARD_MIN.saturating_sub(elapsed));
        true
    }
}

/// The editor's background while the window grows, and another frame soon.
fn plain(ui: &egui::Ui) {
    ui.painter()
        .rect_filled(ui.max_rect(), 0.0, crate::app::BACKGROUND);
    ui.ctx().request_repaint();
}

/// Gives the window its title bar and the editor's size, centred where
/// the card was (or on the screen).
fn to_editor(ctx: &egui::Context) {
    let (card, monitor) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().monitor_size));
    let size = monitor.map_or(EDITOR_SIZE, |m| EDITOR_SIZE.min(m));
    let centre = card
        .map(|r| r.center())
        .or_else(|| monitor.map(|m| (m / 2.0).to_pos2()));
    for cmd in [
        egui::ViewportCommand::Transparent(false),
        egui::ViewportCommand::Decorations(true),
        egui::ViewportCommand::Resizable(true),
        egui::ViewportCommand::MinInnerSize(EDITOR_MIN_SIZE),
        egui::ViewportCommand::InnerSize(size),
    ] {
        ctx.send_viewport_cmd(cmd);
    }
    if let Some(centre) = centre {
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(centre - size / 2.0));
    }
}

/// The card as a texture, or `None` if the bundled PNG cannot be decoded
/// (the window then stays empty until the editor shows).
fn texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let png = eframe::icon_data::from_png_bytes(PNG).ok()?;
    let size = [png.width as usize, png.height as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, &png.rgba);
    Some(ctx.load_texture("launch-card", image, egui::TextureOptions::LINEAR))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use resvg::{tiny_skia, usvg};

    use super::*;

    const SVG: &str = include_str!("../assets/launch-card.svg");

    /// Pixels per point in the bundled PNG.
    const SCALE: f32 = 2.0;

    /// Renders the SVG at [`SCALE`] pixels per point.
    fn render_svg() -> tiny_skia::Pixmap {
        let tree = usvg::Tree::from_str(SVG, &usvg::Options::default()).expect("SVG parses");
        let svg = tree.size();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small, > 0
        let (w, h) = (
            (svg.width() * SCALE).ceil() as u32,
            (svg.height() * SCALE).ceil() as u32,
        );
        let mut pixmap = tiny_skia::Pixmap::new(w, h).expect("non-empty size");
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(SCALE, SCALE),
            &mut pixmap.as_mut(),
        );
        pixmap
    }

    fn decoded() -> egui::IconData {
        eframe::icon_data::from_png_bytes(PNG).expect("launch-card.png decodes")
    }

    #[test]
    fn bundled_card_is_twice_the_card_size() {
        let png = decoded();
        assert_eq!((png.width, png.height), (954, 637));
        #[allow(clippy::cast_precision_loss)]
        let points = egui::vec2(png.width as f32, png.height as f32) / SCALE;
        assert!((points - SIZE).length() < 1.0, "{points:?}");
    }

    #[test]
    fn card_corners_are_transparent_and_its_middle_opaque() {
        let png = decoded();
        let alpha = |x: u32, y: u32| png.rgba[((y * png.width + x) * 4 + 3) as usize];
        let (r, b) = (png.width - 1, png.height - 1);
        for (x, y) in [(0, 0), (r, 0), (0, b), (r, b)] {
            assert_eq!(alpha(x, y), 0, "corner ({x}, {y})");
        }
        assert_eq!(alpha(png.width / 2, png.height / 2), 255);
    }

    #[test]
    fn bundled_png_matches_the_svg() {
        let png = decoded();
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
            "{differing} pixels differ: regenerate launch-card.png (see module docs)"
        );
    }

    /// One frame with the window `size` points big; whether the card is
    /// still up.
    fn frame(ctx: &egui::Context, card: &mut LaunchCard, size: egui::Vec2) -> bool {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        let mut up = false;
        ctx.run_ui(input, |ui| up = card.show(ui))
            .textures_delta
            .clear();
        up
    }

    #[test]
    fn the_editor_waits_for_the_window_to_grow_past_the_card() {
        let ctx = egui::Context::default();
        let long_ago = Instant::now()
            .checked_sub(launch::CARD_MIN * 2)
            .expect("clock is past boot + 4 s");
        let mut card = LaunchCard::new(&ctx, long_ago);
        assert!(frame(&ctx, &mut card, SIZE), "first frame: the card");
        assert!(
            frame(&ctx, &mut card, SIZE),
            "switch frame: still card-sized"
        );
        assert!(frame(&ctx, &mut card, SIZE), "window not grown yet");
        assert!(!frame(&ctx, &mut card, EDITOR_SIZE), "grown: the editor");
    }

    #[test]
    fn the_editor_draws_after_a_short_wait_even_if_the_window_does_not_grow() {
        assert!(!editor_may_draw(SIZE, Duration::ZERO));
        assert!(!editor_may_draw(SIZE, GROW_WAIT / 2));
        assert!(editor_may_draw(SIZE, GROW_WAIT));
        assert!(editor_may_draw(EDITOR_MIN_SIZE, Duration::ZERO));
    }

    #[test]
    #[ignore = "writes assets/launch-card.png; run after changing assets/launch-card.svg"]
    fn regenerate_launch_card_png() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/launch-card.png");
        render_svg()
            .save_png(&path)
            .expect("launch-card.png is writable");
    }
}
