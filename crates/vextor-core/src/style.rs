//! Fill and stroke of a shape.

use serde::{Deserialize, Serialize};

use crate::color::Rgba;
use crate::ids::ResourceId;

/// How a shape is filled and stroked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub fill: Paint,
    /// Left out of saved files when `None` (format version 4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
}

/// What a fill or a stroke is painted with.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Paint {
    None,
    Solid(Rgba),
    /// Inherits the CSS `color` of the page (icons).
    CurrentColor,
    /// Refers to a [`crate::Resource::Gradient`].
    Gradient(ResourceId),
    /// Refers to a [`crate::Resource::Pattern`].
    Pattern(ResourceId),
}

/// An outline drawn along the shape, SVG semantics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub paint: Paint,
    pub width: f64,
    pub cap: LineCap,
    pub join: LineJoin,
    /// ≥ 1, SVG default 4.
    pub miter_limit: f64,
    /// Dash and gap lengths; empty = solid line.
    pub dash: Vec<f64>,
    pub dash_offset: f64,
}

/// SVG `stroke-linecap`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

/// SVG `stroke-linejoin`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

/// Most swatches a document keeps.
pub const MAX_SWATCHES: usize = 256;
/// Longest swatch name, in characters.
pub const MAX_SWATCH_NAME_CHARS: usize = 64;

/// A named colour kept with the document, in the fill or the stroke list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Swatch {
    pub name: Option<String>,
    pub color: Rgba,
    /// Which list it is in. Files without it load as fill swatches.
    #[serde(default)]
    pub kind: SwatchKind,
}

/// The swatch lists: fills (and text backgrounds), strokes (and text
/// borders). Each colour editor shows and adds to its own list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwatchKind {
    #[default]
    Fill,
    Stroke,
}

impl Default for Style {
    /// Black fill, no stroke (the SVG default).
    fn default() -> Self {
        Self {
            fill: Paint::Solid(Rgba::BLACK),
            stroke: None,
        }
    }
}

impl Default for Stroke {
    /// 1 px black, butt caps, miter joins, solid: the SVG defaults.
    fn default() -> Self {
        Self {
            paint: Paint::Solid(Rgba::BLACK),
            width: 1.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: 4.0,
            dash: Vec::new(),
            dash_offset: 0.0,
        }
    }
}

impl Style {
    /// The fill and stroke paints, in that order.
    pub fn paints(&self) -> impl Iterator<Item = Paint> + '_ {
        std::iter::once(self.fill).chain(self.stroke.as_ref().map(|s| s.paint))
    }

    /// Mutable fill and stroke paints, in that order.
    pub fn paints_mut(&mut self) -> Vec<&mut Paint> {
        let mut out = vec![&mut self.fill];
        if let Some(s) = &mut self.stroke {
            out.push(&mut s.paint);
        }
        out
    }
}
