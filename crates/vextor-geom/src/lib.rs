//! Vextor geometry: hit-testing, snapping, booleans, offset, simplify,
//! outline stroke and text layout over the `vextor-core` model.

mod convert;
mod draw;
mod hit;
mod path_edit;
mod resize;
mod snap;
mod text;
mod transform;

pub use convert::{
    bezpath_to_path, fitted_radii, geometry_to_path, path_to_bezpath, to_bezpath, to_kurbo,
};
pub use draw::{
    Corner, DrawMods, ShapeKind, ShapeSize, corner_radius_at, corner_widget, drawn, sized,
};
pub use hit::{bounds, from_affine, pick, selectable, to_affine, touching, union_bounds};
pub use path_edit::{
    NodeCorner, SegmentHit, bend, constrain_angle, constrain_vector, from_point, from_vec,
    handle_or_none, hit_node, hit_segment, is_straight, join_open, node_corner, open_endpoints,
    segment, smooth_handles, split_segment, transform_subpath,
};
pub use resize::{Resized, polar, resize, stretched};
pub use snap::{BoxSnap, Guide, Snap, SnapKind, SnapTargets, minor_spacing, pixel_snap, tolerance};
pub use text::{
    FaceStyle, FontDb, Fonts, GlyphSpan, LineBox, MissingFont, ResolvedFace, TextLayout, layout,
    text_box, text_outlines,
};
pub use transform::{
    Align, Handle, ROTATE_STEP, align, distribute, flip, retarget, rotate, rotation_angle,
    rotation_degrees, scale,
};
pub use vextor_core::EPS;
