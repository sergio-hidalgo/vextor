//! The macOS bundle's icons (spec 13c): the app icon and the `.vxtr`
//! document icon, rendered from their SVGs into `.iconset` folders that
//! `iconutil` turns into `.icns`. Test code only: `scripts/bundle-macos.sh`
//! runs
//!
//! ```sh
//! VEXTOR_ICONSET_DIR=target/bundle cargo test -p vextor-app -- --ignored write_iconsets
//! ```

use std::path::Path;

use resvg::{tiny_skia, usvg};

const APP_SVG: &str = include_str!("../assets/icon.svg");
const DOCUMENT_SVG: &str = include_str!("../assets/document-icon.svg");
const INFO_PLIST: &str = include_str!("../macos/Info.plist");

/// Every image an `.iconset` holds: file name and side in px.
const ICONSET: [(&str, u32); 10] = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
];

/// The rounded corners `icon.svg` gives its tile (for the window icon).
const APP_TILE_CORNERS: &str = r#" rx="14.87" ry="14.87""#;

/// The bundle's app icon: `icon.svg` full bleed, its amber tile square
/// to the edges with no border. macOS 26 rounds a full-bleed square icon
/// to its own shape; an icon with its own corners and a transparent
/// border it shrinks onto a grey tile instead (owner review).
fn app_bundle_svg() -> String {
    APP_SVG.replace(APP_TILE_CORNERS, "")
}

/// `svg` rendered centred on a `side` px square, `inset` × `side` left
/// empty on every side.
fn render(svg: &str, side: u32, inset: f32) -> tiny_skia::Pixmap {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).expect("icon SVG parses");
    let mut pixmap = tiny_skia::Pixmap::new(side, side).expect("non-empty size");
    #[allow(clippy::cast_precision_loss)] // icon sides are small
    let side = side as f32;
    let room = side * (1.0 - 2.0 * inset);
    let svg_size = tree.size();
    let scale = (room / svg_size.width()).min(room / svg_size.height());
    let dx = (side - svg_size.width() * scale) / 2.0;
    let dy = (side - svg_size.height() * scale) / 2.0;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale).post_translate(dx, dy),
        &mut pixmap.as_mut(),
    );
    pixmap
}

/// Writes `svg` as `dir`, an `.iconset` folder with every [`ICONSET`] size.
fn write_iconset(svg: &str, inset: f32, dir: &Path) {
    std::fs::create_dir_all(dir).expect("iconset folder is writable");
    for (name, side) in ICONSET {
        render(svg, side, inset)
            .save_png(dir.join(name))
            .expect("icon PNG is writable");
    }
}

/// `AppIcon.iconset` and `Document.iconset` in `$VEXTOR_ICONSET_DIR`, for
/// the bundle script.
#[test]
#[ignore = "writes files, run by scripts/bundle-macos.sh"]
fn write_iconsets() {
    let dir = std::env::var_os("VEXTOR_ICONSET_DIR").expect("VEXTOR_ICONSET_DIR is set");
    let dir = Path::new(&dir);
    write_iconset(&app_bundle_svg(), 0.0, &dir.join("AppIcon.iconset"));
    write_iconset(DOCUMENT_SVG, 0.0, &dir.join("Document.iconset"));
}

/// The straight colour at (`x`, `y`) of `p`.
fn at(p: &tiny_skia::Pixmap, x: u32, y: u32) -> [u8; 4] {
    let c = p.pixel(x, y).expect("inside").demultiply();
    [c.red(), c.green(), c.blue(), c.alpha()]
}

fn near(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 12)
}

const AMBER: [u8; 4] = [0xdc, 0xa5, 0x00, 255];

#[test]
fn the_document_icon_is_a_page_with_a_fold_an_open_frame_and_the_logo() {
    let p = render(DOCUMENT_SVG, 1024, 0.0);
    assert_eq!(at(&p, 20, 20)[3], 0, "transparent around the page");
    assert_eq!(at(&p, 800, 100)[3], 0, "the folded-away corner is empty");
    assert!(near(at(&p, 700, 200), [0xe3, 0xe3, 0xe3, 255]), "the fold");
    assert!(near(at(&p, 330, 300), [0xf6, 0xf6, 0xf6, 255]), "the page");
    assert!(near(at(&p, 264, 500), AMBER), "the frame's left side");
    assert!(near(at(&p, 500, 852), AMBER), "the frame's bottom");
    assert!(near(at(&p, 760, 600), AMBER), "the frame's right side");
    assert!(
        near(at(&p, 645, 164), [0xf6, 0xf6, 0xf6, 255]),
        "open before the fold"
    );
    assert!(near(at(&p, 405, 516), AMBER), "the logo's O");
    assert!(
        near(at(&p, 512, 516), [0xf6, 0xf6, 0xf6, 255]),
        "inside the O"
    );
}

#[test]
fn the_bundle_app_icon_fills_its_square_to_the_corners() {
    assert!(
        APP_SVG.contains(APP_TILE_CORNERS),
        "icon.svg's tile corners changed"
    );
    let p = render(&app_bundle_svg(), 1024, 0.0);
    for (x, y) in [(0, 0), (1023, 0), (0, 1023), (1023, 1023), (512, 2)] {
        assert_eq!(at(&p, x, y)[3], 255, "opaque at ({x}, {y})");
    }
    assert!(
        near(at(&p, 60, 960), AMBER),
        "the amber tile reaches the corner"
    );
}

#[test]
fn an_iconset_has_every_apple_size() {
    let dir = std::env::temp_dir().join(format!("vextor-iconset-{}", std::process::id()));
    write_iconset(DOCUMENT_SVG, 0.0, &dir);
    for (name, side) in ICONSET {
        let bytes = std::fs::read(dir.join(name)).expect("written");
        let icon = eframe::icon_data::from_png_bytes(&bytes).expect("decodes");
        assert_eq!((icon.width, icon.height), (side, side), "{name}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn info_plist_declares_vxtr_with_the_icons_the_script_makes() {
    let has = |s: &str| INFO_PLIST.contains(s);
    let id = "<string>io.github.sergio-hidalgo.vextor.document</string>";
    assert_eq!(INFO_PLIST.matches(id).count(), 2, "exported and used");
    assert!(has("<string>vxtr</string>"), "the extension");
    assert!(has(
        "<key>CFBundleIconFile</key>\n\t<string>AppIcon</string>"
    ));
    assert!(has(
        "<key>CFBundleTypeIconFile</key>\n\t\t\t<string>Document</string>"
    ));
    assert!(has(
        "<key>UTTypeIconFile</key>\n\t\t\t<string>Document</string>"
    ));
    assert!(has(
        "<key>CFBundleExecutable</key>\n\t<string>vextor</string>"
    ));
    assert!(
        has("<string>__VERSION__</string>"),
        "version filled in by the script"
    );
}
