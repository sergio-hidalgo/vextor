//! Errors of the rasteriser.

use thiserror::Error;

/// Why a document could not be rasterised.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum RenderError {
    /// The requested pixmap has no pixels.
    #[error("viewport is empty ({width}×{height} px)")]
    Empty { width: u32, height: u32 },
    /// The requested pixmap is wider or taller than [`crate::MAX_PIXMAP_SIDE`].
    #[error("viewport {width}×{height} px is too large")]
    TooLarge { width: u32, height: u32 },
    /// Zoom or pixel density is not a finite number > 0, or pan is not finite.
    #[error("invalid view: zoom and pixel density must be > 0, pan finite")]
    InvalidView,
    /// The artboard to export does not exist.
    #[error("the artboard does not exist")]
    NoArtboard,
    /// The PNG encoder failed.
    #[error("the PNG could not be written: {0}")]
    Encode(String),
}
