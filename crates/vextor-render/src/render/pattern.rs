//! Pattern tiles (spec 09): a pattern is drawn once per scale per render
//! into a tile pixmap, then repeated as a shader. Nesting and a per-render
//! tile budget bound the work on hostile files.

use std::cell::RefCell;
use std::rc::Rc;

use tiny_skia::Pixmap;
use vextor_core::{EPS, Pattern, Resource, ResourceId, Transform};

use super::{Scene, Tile};

/// Patterns inside pattern tiles deeper than this are not drawn (a
/// pattern may paint with itself).
const MAX_PATTERN_NESTING: usize = 4;
/// Largest side of a drawn pattern tile, in pixels; a tile shown larger
/// is drawn at this size and scaled up.
const MAX_TILE_SIDE: f64 = 1024.0;
/// Most pattern tiles one render draws, nesting included; patterns past it
/// paint nothing. A document needs a handful (one per pattern and scale);
/// the cap keeps a hostile file from drawing tiles inside tiles for each
/// of thousands of differently scaled objects.
pub(super) const MAX_TILES: usize = 256;

impl Scene<'_> {
    /// Pattern `id` drawn into one tile at the resolution `world` shows it
    /// at, from this render's tiles when already drawn. The tile's content
    /// is placed from its top-left corner and clipped to it (SVG).
    pub(super) fn pattern_tile(&self, id: ResourceId, world: Transform) -> Option<Tile> {
        if self.nesting >= MAX_PATTERN_NESTING {
            return None;
        }
        let Some(Resource::Pattern(pattern)) = self.doc.resources.get(&id) else {
            return None;
        };
        // Lengths of the mapped x and y axes.
        let [xx, xy, yx, yy, ..] = pattern.transform.then(world).0;
        let (sx, sy) = (xx.hypot(xy), yx.hypot(yy));
        let key = (id, sx.to_bits(), sy.to_bits());
        if let Some(tile) = self.tiles.borrow().get(&key) {
            return tile.clone();
        }
        let left = self.budget.get();
        if left == 0 {
            return None;
        }
        self.budget.set(left - 1);
        let tile = self.draw_tile(pattern, sx, sy).map(Rc::new);
        self.tiles.borrow_mut().insert(key, tile.clone());
        tile
    }

    /// Draws pattern `p`'s tile scaled by `sx`, `sy` (at most
    /// [`MAX_TILE_SIDE`] pixels a side, at least one).
    fn draw_tile(&self, p: &Pattern, sx: f64, sy: f64) -> Option<(Pixmap, Transform)> {
        let side = |len: f64, scale: f64| -> Option<(u32, f64)> {
            let px = (len * scale).round().clamp(1.0, MAX_TILE_SIDE);
            let ok = len > EPS && px.is_finite();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 1..=MAX_TILE_SIDE
            ok.then(|| (px as u32, px / len))
        };
        let (width, kx) = side(p.tile.width, sx)?;
        let (height, ky) = side(p.tile.height, sy)?;
        let mut pixmap = Pixmap::new(width, height)?;
        let inner = Scene {
            doc: self.doc,
            fonts: self.fonts,
            nesting: self.nesting + 1,
            tiles: RefCell::default(),
            budget: Rc::clone(&self.budget),
        };
        for &root in &p.content {
            inner.draw_node(root, Transform::scale(kx, ky), &mut pixmap, 0);
        }
        let ts = Transform::scale(1.0 / kx, 1.0 / ky)
            .then(Transform::translate(p.tile.x, p.tile.y))
            .then(p.transform);
        Some((pixmap, ts))
    }
}
