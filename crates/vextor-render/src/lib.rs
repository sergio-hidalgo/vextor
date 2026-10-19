//! Vextor canvas rasteriser: draws a `vextor-core` document into a pixmap.
//!
//! Solid fills and strokes, transforms, group opacity, clips and text
//! (glyph outlines over an optional frame, laid out by `vextor-geom`) are
//! drawn. Gradients and patterns (spec 09) and images draw nothing yet.

mod cache;
mod error;
pub mod fixtures;
mod paint;
mod render;

pub use cache::{CachedFrame, RenderCache};
pub use error::RenderError;
pub use render::{MAX_PIXMAP_SIDE, Viewport, render, render_into};
pub use tiny_skia::Pixmap;
