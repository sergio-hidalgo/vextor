//! Shared resources referenced by paints, mirroring SVG `<defs>`.

use serde::{Deserialize, Serialize};

use crate::color::Rgba;
use crate::ids::NodeId;
use crate::math::{Point, Rect, Transform};

/// A gradient or a pattern.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    Gradient(Gradient),
    Pattern(Pattern),
}

/// SVG `linearGradient` / `radialGradient`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gradient {
    pub name: Option<String>,
    pub kind: GradientKind,
    /// ≥ 2, offsets in 0..=1, sorted by offset.
    pub stops: Vec<GradientStop>,
    pub units: GradientUnits,
    pub spread: Spread,
}

/// Geometry of a gradient.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradientKind {
    Linear {
        start: Point,
        end: Point,
    },
    Radial {
        center: Point,
        radius: f64,
        focal: Option<Point>,
    },
}

/// One colour stop.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// 0.0..=1.0.
    pub offset: f64,
    pub color: Rgba,
}

/// SVG `gradientUnits`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradientUnits {
    ObjectBoundingBox,
    UserSpace,
}

/// SVG `spreadMethod`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Spread {
    Pad,
    Reflect,
    Repeat,
}

/// SVG `pattern`: a tile of nodes repeated over the painted area.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    pub name: Option<String>,
    /// One tile, in pattern space.
    pub tile: Rect,
    pub transform: Transform,
    /// Nodes owned by the pattern, not in any layer (bottom → top).
    pub content: Vec<NodeId>,
}

impl Resource {
    /// Pattern content roots; empty for gradients.
    #[must_use]
    pub fn content(&self) -> &[NodeId] {
        match self {
            Self::Gradient(_) => &[],
            Self::Pattern(p) => &p.content,
        }
    }
}
