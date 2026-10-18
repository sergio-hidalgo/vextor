//! Fill and stroke of a shape.

use serde::{Deserialize, Serialize};

use crate::color::Rgba;
use crate::ids::ResourceId;

/// How a shape is filled and stroked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub fill: Paint,
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

/// A named colour kept with the document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Swatch {
    pub name: Option<String>,
    pub color: Rgba,
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
}
