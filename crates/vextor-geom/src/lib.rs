//! Vextor geometry: hit-testing, snapping, booleans, offset, simplify and
//! outline stroke over the `vextor-core` model.

mod convert;

pub use convert::{bezpath_to_path, geometry_to_path, path_to_bezpath, to_bezpath, to_kurbo};
pub use vextor_core::EPS;
