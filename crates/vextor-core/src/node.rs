//! The node tree: layers, groups, shapes, images and text.

use serde::{Deserialize, Serialize};

use crate::ids::NodeId;
use crate::math::{Point, Rect, Transform};
use crate::path::Path;
use crate::style::{Paint, Style};
use crate::text::Text;

/// Longest node name, in characters (spec 10).
pub const MAX_NODE_NAME_CHARS: usize = 64;

/// `typed` as a valid node name (spec 10): control characters become
/// spaces, the ends are trimmed and it is cut to [`MAX_NODE_NAME_CHARS`];
/// `None` when nothing is left.
#[must_use]
pub fn clean_name(typed: &str) -> Option<String> {
    let name: String = typed
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_NODE_NAME_CHARS)
        .collect();
    let name = name.trim_end();
    (!name.is_empty()).then(|| name.to_owned())
}

/// One entry of [`crate::Document::nodes`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
///
/// Fields at their default (no name, visible, unlocked, opaque, identity
/// transform) are left out of saved files and filled back on load
/// (format version 4).
pub struct Node {
    pub id: NodeId,
    /// Becomes the SVG `id` (slugified) when set. Not blank, at most
    /// [`MAX_NODE_NAME_CHARS`], no control characters (spec 10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub visible: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    /// 0.0..=1.0, group opacity.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f64,
    /// Relative to the parent.
    #[serde(default, skip_serializing_if = "is_identity")]
    pub transform: Transform,
    pub kind: NodeKind,
}

fn yes() -> bool {
    true
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's signature
fn is_true(b: &bool) -> bool {
    *b
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's signature
fn is_false(b: &bool) -> bool {
    !*b
}

fn one() -> f64 {
    1.0
}

/// Exactly 1: only an untouched opacity is left out, so nothing rounds.
#[allow(clippy::trivially_copy_pass_by_ref, clippy::float_cmp)] // serde's signature; exact on purpose
fn is_one(v: &f64) -> bool {
    *v == 1.0
}

/// Exactly the identity, bit for bit, for the same reason.
#[allow(clippy::float_cmp)] // exact on purpose
fn is_identity(t: &Transform) -> bool {
    t.0 == Transform::IDENTITY.0
}

/// What a node is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Top level only: listed in [`crate::Document::layers`].
    Layer(Container),
    Group(Container),
    Shape(Shape),
    Image(Image),
    /// Point text (spec 05c).
    Text(Text),
}

/// Children of a layer or group.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Container {
    /// Bottom → top.
    pub children: Vec<NodeId>,
    /// One of `children`, a `Shape`, used as the SVG `clipPath` of the
    /// container instead of being drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<NodeId>,
}

/// Geometry plus paint.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub geometry: Geometry,
    pub style: Style,
}

/// The outline of a shape. Parametric kinds stay editable as such until
/// converted to a path.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Geometry {
    Rect {
        rect: Rect,
        radius: CornerRadii,
    },
    Ellipse {
        center: Point,
        rx: f64,
        ry: f64,
    },
    /// 3 ≤ `sides` ≤ [`crate::MAX_POLYGON_SIDES`].
    Polygon {
        center: Point,
        radius: f64,
        sides: u32,
        corner_radius: f64,
    },
    /// 3 ≤ `points` ≤ [`crate::MAX_STAR_POINTS`].
    Star {
        center: Point,
        outer: f64,
        inner: f64,
        points: u32,
        corner_radius: f64,
    },
    Line {
        from: Point,
        to: Point,
    },
    Path(Path),
}

/// Per-corner radii of a rectangle (CSS `border-radius` order).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CornerRadii {
    pub tl: f64,
    pub tr: f64,
    pub br: f64,
    pub bl: f64,
}

/// A raster image placed in a rectangle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Image {
    pub rect: Rect,
    pub source: ImageSource,
}

/// Where the image pixels come from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageSource {
    Embedded {
        mime: ImageMime,
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
    },
}

/// Supported embedded image formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageMime {
    Png,
    Jpeg,
    Webp,
}

impl Node {
    /// A visible, unlocked, opaque node with identity transform.
    #[must_use]
    pub fn new(id: NodeId, kind: NodeKind) -> Self {
        Self {
            id,
            name: None,
            visible: true,
            locked: false,
            opacity: 1.0,
            transform: Transform::IDENTITY,
            kind,
        }
    }

    /// The children list when this is a layer or group.
    #[must_use]
    pub fn container(&self) -> Option<&Container> {
        match &self.kind {
            NodeKind::Layer(c) | NodeKind::Group(c) => Some(c),
            NodeKind::Shape(_) | NodeKind::Image(_) | NodeKind::Text(_) => None,
        }
    }

    /// Mutable children list when this is a layer or group.
    pub fn container_mut(&mut self) -> Option<&mut Container> {
        match &mut self.kind {
            NodeKind::Layer(c) | NodeKind::Group(c) => Some(c),
            NodeKind::Shape(_) | NodeKind::Image(_) | NodeKind::Text(_) => None,
        }
    }

    /// Every paint of a shape or text; none for other kinds.
    #[must_use]
    pub fn paints(&self) -> Vec<Paint> {
        match &self.kind {
            NodeKind::Shape(s) => s.style.paints().collect(),
            NodeKind::Text(t) => t.paints().collect(),
            NodeKind::Layer(_) | NodeKind::Group(_) | NodeKind::Image(_) => Vec::new(),
        }
    }

    /// Mutable access to every paint, in [`Node::paints`] order.
    pub fn paints_mut(&mut self) -> Vec<&mut Paint> {
        match &mut self.kind {
            NodeKind::Shape(s) => s.style.paints_mut(),
            NodeKind::Text(t) => t.paints_mut(),
            NodeKind::Layer(_) | NodeKind::Group(_) | NodeKind::Image(_) => Vec::new(),
        }
    }

    /// True for [`NodeKind::Layer`].
    #[must_use]
    pub fn is_layer(&self) -> bool {
        matches!(self.kind, NodeKind::Layer(_))
    }
}

/// Serialises image bytes as a standard base64 string.
mod base64_bytes {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    /// While a `.vxtr` file is written, an index into its shared images;
    /// otherwise inline base64.
    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        match crate::format::share_image(bytes) {
            Some(i) => s.serialize_u32(i),
            None => s.serialize_str(&STANDARD.encode(bytes)),
        }
    }

    /// Inline base64 (every version), or an index into the file's shared
    /// `images` (version 4, [`crate::format`]).
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Saved {
        Shared(u32),
        Inline(String),
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        match Saved::deserialize(d)? {
            Saved::Inline(text) => STANDARD.decode(text).map_err(serde::de::Error::custom),
            Saved::Shared(i) => crate::format::shared_image(i)
                .ok_or_else(|| serde::de::Error::custom(format!("no shared image {i}"))),
        }
    }
}
