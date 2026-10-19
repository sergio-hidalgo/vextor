//! Vextor geometry: hit-testing, snapping, booleans, offset, simplify,
//! outline stroke and text layout over the `vextor-core` model.

mod convert;
mod hit;
mod resize;
mod text;
mod transform;

pub use convert::{bezpath_to_path, geometry_to_path, path_to_bezpath, to_bezpath, to_kurbo};
pub use hit::{bounds, from_affine, pick, selectable, to_affine, touching, union_bounds};
pub use resize::{Resized, polar, resize, stretched};
pub use text::{
    FaceStyle, FontDb, Fonts, LineBox, MissingFont, ResolvedFace, TextLayout, layout, text_box,
    text_outlines,
};
pub use transform::{
    Align, Handle, ROTATE_STEP, align, distribute, flip, retarget, rotate, rotation_angle,
    rotation_degrees, scale,
};
pub use vextor_core::EPS;
