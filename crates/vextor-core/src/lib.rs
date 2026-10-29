//! Vextor document model: documents, layers, shapes, paths, text, paints, the
//! commands that edit them and the undo history.
//!
//! Knows nothing about UI, rendering, file I/O or SVG (see the `rust-core`
//! skill); [`from_bytes`] / [`to_bytes`] define the `.vxtr` bytes the app
//! reads and writes.

mod apply;
mod change;
mod color;
mod command;
mod document;
mod edit;
mod editor;
mod error;
mod format;
mod hash;
mod history;
mod ids;
mod layers;
mod math;
mod node;
mod path;
mod resource;
mod settings;
mod style;
mod text;
mod validate;

pub use change::{Change, Patch};
pub use color::{ColorFormat, Rgba};
pub use command::Command;
pub use document::{Artboard, Document, FORMAT_VERSION, Parent};
pub use edit::Arrange;
pub use editor::{ClipResource, ClipboardPayload, EditorState, Selection, ToolId, View};
pub use error::{ColorError, CoreError};
pub use format::{
    FormatError, MAX_FILE_BYTES, MAX_NODES, MIN_FORMAT_VERSION, from_bytes, to_bytes,
    to_bytes_pretty, to_writer, to_writer_with_preview,
};
pub use hash::{Fnv1a, fnv1a};
pub use history::{DEFAULT_LIMIT, History};
pub use ids::{ArtboardId, NodeId, ResourceId};
pub use math::{EPS, Point, Rect, Transform, Vec2};
pub use node::{
    Container, CornerRadii, Geometry, Image, ImageMime, ImageSource, MAX_NODE_NAME_CHARS, Node,
    NodeKind, Shape, clean_name,
};
pub use path::{FillRule, NodeType, Path, PathNode, PathNodeRef, Side, SubPath};
pub use resource::{
    Gradient, GradientKind, GradientStop, GradientUnits, Pattern, Resource, Spread,
};
pub use settings::{GridSettings, IdMode, SnapSettings, Styling, SvgExportOptions};
pub use style::{
    LineCap, LineJoin, MAX_SWATCH_NAME_CHARS, MAX_SWATCHES, Paint, Stroke, Style, Swatch,
    SwatchKind,
};
pub use text::{
    FONT_WEIGHTS, FontSpec, MAX_FONT_FAMILY_CHARS, MAX_TEXT_CHARS, Text, TextAlign, TextFrame,
};
pub use validate::{MAX_POLYGON_SIDES, MAX_STAR_POINTS, Violation};
