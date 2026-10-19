//! Resizing as a vector editor does: the stretch goes into the shapes'
//! geometry, not their transform, so strokes keep their width and corner
//! radii their size. (A stretched transform scales the stroke too, unevenly
//! when the stretch is uneven — like a resized image.)
//!
//! Each shape's new local transform is split (polar decomposition) into a
//! rigid part — rotation or mirror, plus position — kept as its transform,
//! and a stretch applied to its points. Groups pass the resize down to the
//! shapes inside; images keep a stretched transform. Text keeps its rigid
//! transform and scales its font size and letter spacing by the stretch's
//! mean scale, so it keeps its proportions (frame padding, corner radii and
//! stroke widths stay as they are).

use kurbo::{Affine, Vec2};
use vextor_core::{Document, Geometry, NodeId, NodeKind, Path, Point, Rect, Text, Transform};

use crate::convert::geometry_to_path;
use crate::hit::{from_affine, to_affine};

/// Deepest nesting followed, matching the renderer's guard.
const MAX_DEPTH: usize = 64;

/// The edits that resize objects: new transforms (shapes, images, text),
/// new geometries (shapes whose stretch changed) and rescaled text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resized {
    pub transforms: Vec<(NodeId, Transform)>,
    pub geometries: Vec<(NodeId, Geometry)>,
    /// Text objects with their new size and letter spacing.
    pub texts: Vec<(NodeId, Text)>,
}

/// The edits that apply `change` (document space, typically a scale from
/// the transform box) to `ids` and everything inside them, with every
/// shape's stretch baked into its geometry.
#[must_use]
pub fn resize(doc: &Document, ids: &[NodeId], change: Affine) -> Resized {
    let mut out = Resized::default();
    for &id in ids {
        let parent = to_affine(doc.parent_transform(id));
        if parent.determinant().abs() < vextor_core::EPS {
            continue;
        }
        walk(doc, id, parent, change, 0, &mut out);
    }
    out
}

fn walk(
    doc: &Document,
    id: NodeId,
    parent: Affine,
    change: Affine,
    depth: usize,
    out: &mut Resized,
) {
    let Some(node) = doc.nodes.get(&id) else {
        return;
    };
    if depth > MAX_DEPTH {
        return;
    }
    let local = to_affine(node.transform);
    match &node.kind {
        NodeKind::Layer(c) | NodeKind::Group(c) => {
            let world = parent * local;
            for &child in &c.children {
                walk(doc, child, world, change, depth + 1, out);
            }
        }
        NodeKind::Text(text) => {
            let new_local = parent.inverse() * change * parent * local;
            let (rigid, stretch) = polar(new_local);
            let t = from_affine(rigid);
            let k = stretch.determinant().abs().sqrt();
            if !t.is_finite() || !k.is_finite() || k < vextor_core::EPS {
                return;
            }
            out.transforms.push((id, t));
            if (k - 1.0).abs() > 1e-9 {
                let mut scaled = text.clone();
                scaled.size *= k;
                scaled.letter_spacing *= k;
                out.texts.push((id, scaled));
            }
        }
        NodeKind::Image(_) => {
            let t = from_affine(parent.inverse() * change * parent * local);
            if t.is_finite() {
                out.transforms.push((id, t));
            }
        }
        NodeKind::Shape(s) => {
            let new_local = parent.inverse() * change * parent * local;
            let (rigid, stretch) = polar(new_local);
            let t = from_affine(rigid);
            if !t.is_finite() {
                return;
            }
            out.transforms.push((id, t));
            if !is_identity(stretch) {
                out.geometries.push((id, stretched(&s.geometry, stretch)));
            }
        }
    }
}

/// Splits `m` into `rigid · stretch`: `rigid` is a rotation (or a mirror,
/// when `m` mirrors) plus `m`'s translation; `stretch` is symmetric and
/// positive, with no translation.
#[must_use]
#[allow(clippy::many_single_char_names)] // SVG matrix coefficient names
pub fn polar(m: Affine) -> (Affine, Affine) {
    let [a, b, c, d, e, f] = m.as_coeffs();
    // Linear part as columns (a, b) and (c, d). For a rotation R(θ) ·
    // symmetric, θ = atan2(b − c, a + d); for a mirror F(θ) · symmetric,
    // θ = atan2(b + c, a − d) with F(θ) = [[cos, sin], [sin, −cos]].
    let mirrored = a * d - b * c < 0.0;
    let rot = if mirrored {
        let t = (b + c).atan2(a - d);
        Affine::new([t.cos(), t.sin(), t.sin(), -t.cos(), 0.0, 0.0])
    } else {
        let t = (b - c).atan2(a + d);
        Affine::new([t.cos(), t.sin(), -t.sin(), t.cos(), 0.0, 0.0])
    };
    // R is orthogonal, so R⁻¹ = Rᵀ, which for these two forms is inverse().
    let stretch = rot.inverse() * Affine::new([a, b, c, d, 0.0, 0.0]);
    let [ra, rb, rc, rd, ..] = rot.as_coeffs();
    (Affine::new([ra, rb, rc, rd, e, f]), stretch)
}

fn is_identity(stretch: Affine) -> bool {
    let [xx, yx, xy, yy, ..] = stretch.as_coeffs();
    (xx - 1.0).abs() < 1e-9 && yx.abs() < 1e-9 && xy.abs() < 1e-9 && (yy - 1.0).abs() < 1e-9
}

/// `geometry` with every point mapped through `stretch` (linear,
/// symmetric). Rects and
/// ellipses stay what they are when `s` is axis-aligned, and polygons and
/// stars when it is uniform; otherwise they become paths.
#[must_use]
pub fn stretched(geometry: &Geometry, stretch: Affine) -> Geometry {
    let [sx, shear_y, shear_x, sy, ..] = stretch.as_coeffs();
    let aligned = shear_y.abs() < 1e-9 && shear_x.abs() < 1e-9;
    let uniform = aligned && (sx - sy).abs() < 1e-9;
    let pt = |p: Point| {
        let q = stretch * kurbo::Point::new(p.x, p.y);
        Point::new(q.x, q.y)
    };
    match geometry {
        Geometry::Rect { rect, radius } if aligned => Geometry::Rect {
            rect: Rect::new(rect.x * sx, rect.y * sy, rect.width * sx, rect.height * sy),
            radius: *radius,
        },
        Geometry::Ellipse { center, rx, ry } if aligned => Geometry::Ellipse {
            center: pt(*center),
            rx: rx * sx,
            ry: ry * sy,
        },
        Geometry::Polygon {
            center,
            radius,
            sides,
            corner_radius,
        } if uniform => Geometry::Polygon {
            center: pt(*center),
            radius: radius * sx,
            sides: *sides,
            corner_radius: *corner_radius,
        },
        Geometry::Star {
            center,
            outer,
            inner,
            points,
            corner_radius,
        } if uniform => Geometry::Star {
            center: pt(*center),
            outer: outer * sx,
            inner: inner * sx,
            points: *points,
            corner_radius: *corner_radius,
        },
        Geometry::Line { from, to } => Geometry::Line {
            from: pt(*from),
            to: pt(*to),
        },
        Geometry::Path(p) => Geometry::Path(stretched_path(p, stretch)),
        other => Geometry::Path(stretched_path(&geometry_to_path(other), stretch)),
    }
}

/// Anchors and handles mapped through `s`. A linear map keeps opposite
/// handles opposite (and equal ones equal), so node types stay valid.
fn stretched_path(p: &Path, s: Affine) -> Path {
    let vec = |v: vextor_core::Vec2| {
        let w = s * Vec2::new(v.x, v.y).to_point();
        vextor_core::Vec2::new(w.x, w.y)
    };
    let mut out = p.clone();
    for node in out.subpaths.iter_mut().flat_map(|sp| sp.nodes.iter_mut()) {
        let a = s * kurbo::Point::new(node.anchor.x, node.anchor.y);
        node.anchor = Point::new(a.x, a.y);
        node.handle_in = node.handle_in.map(vec);
        node.handle_out = node.handle_out.map(vec);
    }
    out
}
