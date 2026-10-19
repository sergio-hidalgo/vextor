//! The last rendered frame, kept until the document or the view changes.

use tiny_skia::Pixmap;
use vextor_core::Document;
use vextor_geom::Fonts;

use crate::error::RenderError;
use crate::render::{Viewport, render_into};

/// Holds the last rendered pixmap and what it was rendered from.
///
/// The caller passes a revision number that changes whenever the document
/// changes; the cache cannot see edits itself.
#[derive(Default)]
pub struct RenderCache {
    key: Option<(u64, Viewport)>,
    pixmap: Option<Pixmap>,
}

/// A frame from [`RenderCache::frame`].
pub struct CachedFrame<'a> {
    pub pixmap: &'a Pixmap,
    /// True when the pixmap was rendered by this call (the caller must
    /// upload it again); false when it is the cached one.
    pub fresh: bool,
}

impl RenderCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The frame for `doc` (text laid out with `fonts`) at `revision` seen
    /// through `viewport`, rendered
    /// only when either differs from the cached one.
    ///
    /// # Errors
    /// [`RenderError`] when the viewport is empty, too large or invalid; the
    /// cache is then emptied.
    pub fn frame(
        &mut self,
        doc: &Document,
        fonts: &Fonts,
        revision: u64,
        viewport: &Viewport,
    ) -> Result<CachedFrame<'_>, RenderError> {
        let key = (revision, *viewport);
        let fresh = self.key != Some(key) || self.pixmap.is_none();
        if fresh {
            self.key = None;
            let result = if let Some(pixmap) = self.pixmap.as_mut() {
                render_into(doc, fonts, viewport, pixmap)
            } else {
                crate::render(doc, fonts, viewport).map(|p| self.pixmap = Some(p))
            };
            if let Err(e) = result {
                self.pixmap = None;
                return Err(e);
            }
            self.key = Some(key);
        }
        match &self.pixmap {
            Some(pixmap) => Ok(CachedFrame { pixmap, fresh }),
            // Unreachable: a successful render above always stores a pixmap.
            None => Err(RenderError::Empty {
                width: viewport.width,
                height: viewport.height,
            }),
        }
    }

    /// Forgets the cached frame, so the next call renders.
    pub fn invalidate(&mut self) {
        self.key = None;
    }
}

#[cfg(test)]
mod tests {
    use vextor_core::{Vec2, View};
    use vextor_geom::Fonts;

    use super::*;

    fn vp(pan_x: f64) -> Viewport {
        Viewport {
            view: View {
                zoom: 1.0,
                pan: Vec2::new(pan_x, 0.0),
            },
            width: 16,
            height: 16,
            pixels_per_point: 1.0,
        }
    }

    #[test]
    fn same_revision_and_view_reuse_the_frame() {
        let doc = Document::new("t");
        let mut cache = RenderCache::new();
        assert!(
            cache
                .frame(&doc, &Fonts::default(), 1, &vp(0.0))
                .expect("valid")
                .fresh
        );
        assert!(
            !cache
                .frame(&doc, &Fonts::default(), 1, &vp(0.0))
                .expect("valid")
                .fresh
        );
    }

    #[test]
    fn new_revision_renders_again() {
        let doc = Document::new("t");
        let mut cache = RenderCache::new();
        cache
            .frame(&doc, &Fonts::default(), 1, &vp(0.0))
            .expect("valid");
        assert!(
            cache
                .frame(&doc, &Fonts::default(), 2, &vp(0.0))
                .expect("valid")
                .fresh
        );
    }

    #[test]
    fn new_view_renders_again() {
        let doc = Document::new("t");
        let mut cache = RenderCache::new();
        cache
            .frame(&doc, &Fonts::default(), 1, &vp(0.0))
            .expect("valid");
        assert!(
            cache
                .frame(&doc, &Fonts::default(), 1, &vp(5.0))
                .expect("valid")
                .fresh
        );
    }

    #[test]
    fn invalidate_forces_a_render() {
        let doc = Document::new("t");
        let mut cache = RenderCache::new();
        cache
            .frame(&doc, &Fonts::default(), 1, &vp(0.0))
            .expect("valid");
        cache.invalidate();
        assert!(
            cache
                .frame(&doc, &Fonts::default(), 1, &vp(0.0))
                .expect("valid")
                .fresh
        );
    }

    #[test]
    fn failed_render_empties_the_cache() {
        let doc = Document::new("t");
        let mut cache = RenderCache::new();
        cache
            .frame(&doc, &Fonts::default(), 1, &vp(0.0))
            .expect("valid");
        let mut bad = vp(0.0);
        bad.width = 0;
        assert!(cache.frame(&doc, &Fonts::default(), 1, &bad).is_err());
        assert!(
            cache
                .frame(&doc, &Fonts::default(), 1, &vp(0.0))
                .expect("valid")
                .fresh
        );
    }
}
