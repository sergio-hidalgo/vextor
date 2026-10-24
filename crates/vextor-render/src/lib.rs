//! Vextor canvas rasteriser: draws a `vextor-core` document into a pixmap.
//!
//! Fills and strokes (solid, linear and radial gradients, patterns; dashes),
//! transforms, opacity, clips and text (glyph outlines over an optional
//! frame, laid out by `vextor-geom`) are drawn. Images draw nothing yet.
//! [`artboard_png`] exports an artboard as a PNG.

mod cache;
mod error;
pub mod fixtures;
mod paint;
mod png;
mod render;

pub use cache::{CachedFrame, RenderCache};
pub use error::RenderError;
pub use png::artboard_png;
pub use render::{MAX_PIXMAP_SIDE, Viewport, render, render_into};
pub use tiny_skia::Pixmap;
