//! Image to vector (spec 13b): an image's pixels as a few filled paths, one
//! per colour, stacked large → small.
//!
//! Colours (Otsu threshold or k-means) → speckles merged into their
//! largest neighbour → one outline per region edge on the pixel grid →
//! corners kept, the rest fitted with smooth cubics → `simplify`. Outlines
//! only takes the 6-colour areas and draws the edges between them, each
//! once (`edges`). Written
//! from public papers (Otsu 1979; marching squares; Potrace's corner idea),
//! not from another product.

mod colors;
mod edges;
mod fit;
mod outline;
mod regions;

use std::fmt;

use kurbo::{BezPath, PathEl};
use vextor_core::{FillRule, Rgba};

/// Most separate shapes (subpaths) a trace may give.
pub const MAX_TRACE_SHAPES: usize = 2_000;
/// Most path nodes a trace may give, all colours together.
pub const MAX_TRACE_NODES: usize = 20_000;

/// What a trace gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TraceMode {
    /// Black where the image is dark (Otsu threshold), white left empty.
    BlackAndWhite,
    /// The image's 3 main colours.
    Colors3,
    /// The image's 6 main colours.
    Colors6,
    /// The edges between the image's 6 main colours, each once, as lines
    /// (open where three areas meet): one path, for the caller to stroke.
    Outlines,
}

/// Straight (not premultiplied) RGBA pixels, row by row.
#[derive(Clone, Copy, Debug)]
pub struct Pixels<'a> {
    pub width: usize,
    pub height: usize,
    pub rgba: &'a [u8],
}

/// One piece of a trace: one area of one colour with its holes (or, for
/// outlines, lines that meet), its outline in pixel units (0,0 at the
/// image's top-left corner, one unit per pixel). Pieces come bottom first,
/// a colour's pieces together.
#[derive(Clone, Debug, PartialEq)]
pub struct TracedColor {
    pub color: Rgba,
    pub path: BezPath,
    /// `EvenOdd` when the outline has holes, else `NonZero`.
    pub fill_rule: FillRule,
}

/// Why an image cannot be traced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceError {
    /// More than [`MAX_TRACE_SHAPES`] shapes or [`MAX_TRACE_NODES`] nodes.
    TooDetailed,
    /// The pixel buffer does not match the size.
    BadPixels,
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooDetailed => "Too detailed for vector shapes: try fewer colours",
            Self::BadPixels => "The image's pixels do not match its size",
        })
    }
}

impl std::error::Error for TraceError {}

/// `pixels` traced as `mode`: one entry per piece, colours bottom
/// (largest) to top; for outlines, black pieces of lines. Empty when
/// nothing is opaque (or, in black and white, dark; for outlines, when
/// the image is one colour). The same pixels and mode always give the
/// same result.
///
/// # Errors
/// [`TraceError::TooDetailed`] past the limits; [`TraceError::BadPixels`]
/// when `rgba` is not `width × height × 4` bytes.
pub fn trace(pixels: Pixels<'_>, mode: TraceMode) -> Result<Vec<TracedColor>, TraceError> {
    let count = pixels
        .width
        .checked_mul(pixels.height)
        .ok_or(TraceError::BadPixels)?;
    if count.checked_mul(4) != Some(pixels.rgba.len()) {
        return Err(TraceError::BadPixels);
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    let (mut labels, palette) = colors::classify(pixels, mode);
    regions::merge_speckles(&mut labels, pixels.width, pixels.height);
    if mode == TraceMode::Outlines {
        return outlines(&labels, pixels.width, pixels.height);
    }
    let order = regions::stacking(&labels, palette.len());

    let mut found = Vec::with_capacity(order.len());
    let mut shapes = 0;
    for rank in 0..order.len() {
        let above = regions::stacked_mask(&labels, &order, rank, pixels.width, pixels.height);
        let loops = outline::loops(&above, pixels.width, pixels.height);
        shapes += loops.len();
        if shapes > MAX_TRACE_SHAPES {
            return Err(TraceError::TooDetailed);
        }
        let pieces = outline::pieces(&above, pixels.width, pixels.height, &loops);
        found.push((order[rank], loops, pieces));
    }

    let mut out = Vec::new();
    let mut nodes = 0;
    for (index, loops, pieces) in found {
        for piece in pieces {
            let mut path = BezPath::new();
            for &i in &piece {
                path.extend(fit::fitted(&loops[i]));
            }
            nodes += node_count(&path);
            if nodes > MAX_TRACE_NODES {
                return Err(TraceError::TooDetailed);
            }
            out.push(TracedColor {
                color: palette[index],
                path,
                fill_rule: if piece.len() > 1 {
                    FillRule::EvenOdd
                } else {
                    FillRule::NonZero
                },
            });
        }
    }
    Ok(out)
}

/// The edges between the colour areas of `labels`, as black pieces (lines
/// that meet).
fn outlines(labels: &[u8], width: usize, height: usize) -> Result<Vec<TracedColor>, TraceError> {
    let lines = edges::lines(labels, width, height);
    if lines.len() > MAX_TRACE_SHAPES {
        return Err(TraceError::TooDetailed);
    }
    let mut out = Vec::new();
    let mut nodes = 0;
    for piece in edges::pieces(&lines) {
        let mut path = BezPath::new();
        for &i in &piece {
            path.extend(fit::fitted(&lines[i]));
        }
        nodes += node_count(&path);
        if nodes > MAX_TRACE_NODES {
            return Err(TraceError::TooDetailed);
        }
        out.push(TracedColor {
            color: Rgba::BLACK,
            path,
            fill_rule: FillRule::NonZero,
        });
    }
    Ok(out)
}

/// Nodes `path` becomes in the model: one per element but the closes.
fn node_count(path: &BezPath) -> usize {
    path.elements()
        .iter()
        .filter(|el| !matches!(el, PathEl::ClosePath))
        .count()
}
