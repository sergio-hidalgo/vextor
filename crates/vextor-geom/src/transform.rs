//! Transform maths for the selection: the transform box (scale from a
//! handle, rotate, flip), align and distribute, and turning a document-space
//! change into each node's new local transform.
//!
//! Everything works on an axis-aligned box in document (world) space.

use std::f64::consts::PI;

use kurbo::{Affine, Point, Rect, Vec2};
use vextor_core::{Document, EPS, NodeId, Transform};

use crate::hit::{from_affine, to_affine};

/// Smallest scale factor a drag can reach, so a box never collapses to a
/// line (which could not be undone by scaling back).
const MIN_SCALE: f64 = 1e-3;

/// Rotation step while Shift is held, in radians (15°).
pub const ROTATE_STEP: f64 = PI / 12.0;

/// A scale handle on the transform box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl Handle {
    /// All handles, clockwise from the top edge.
    pub const ALL: [Self; 8] = [
        Self::N,
        Self::NE,
        Self::E,
        Self::SE,
        Self::S,
        Self::SW,
        Self::W,
        Self::NW,
    ];

    /// Which side it drags horizontally (-1 left, 0 none, 1 right) and
    /// vertically (-1 top, 0 none, 1 bottom).
    #[must_use]
    pub fn sides(self) -> (i8, i8) {
        match self {
            Self::N => (0, -1),
            Self::NE => (1, -1),
            Self::E => (1, 0),
            Self::SE => (1, 1),
            Self::S => (0, 1),
            Self::SW => (-1, 1),
            Self::W => (-1, 0),
            Self::NW => (-1, -1),
        }
    }

    /// True for the four corners.
    #[must_use]
    pub fn is_corner(self) -> bool {
        let (x, y) = self.sides();
        x != 0 && y != 0
    }

    /// Where the handle sits on `b`.
    #[must_use]
    pub fn position(self, b: Rect) -> Point {
        let (x, y) = self.sides();
        Point::new(along(b.x0, b.x1, x), along(b.y0, b.y1, y))
    }
}

fn along(lo: f64, hi: f64, side: i8) -> f64 {
    match side {
        -1 => lo,
        1 => hi,
        _ => f64::midpoint(lo, hi),
    }
}

/// The scale that follows dragging `handle` of box `b` by `drag`
/// (document space). `keep_ratio` (Shift) scales both axes alike;
/// `from_centre` (Alt) scales about the centre instead of the opposite
/// side. Dragging past the opposite side mirrors.
#[must_use]
pub fn scale(b: Rect, handle: Handle, drag: Vec2, keep_ratio: bool, from_centre: bool) -> Affine {
    let (hx, hy) = handle.sides();
    let grip = handle.position(b);
    let fixed = if from_centre {
        b.center()
    } else {
        Point::new(along(b.x0, b.x1, -hx), along(b.y0, b.y1, -hy))
    };
    let factor = |side: i8, grip: f64, fixed: f64, d: f64| {
        let span = grip - fixed;
        if side == 0 || span.abs() < EPS {
            1.0
        } else {
            clamp_scale((span + d) / span)
        }
    };
    let mut sx = factor(hx, grip.x, fixed.x, drag.x);
    let mut sy = factor(hy, grip.y, fixed.y, drag.y);
    if keep_ratio {
        let s = if handle.is_corner() {
            sx.abs().max(sy.abs())
        } else if hx == 0 {
            sy.abs()
        } else {
            sx.abs()
        };
        sx = s.copysign(sx);
        sy = s.copysign(sy);
    }
    Affine::translate(fixed.to_vec2())
        * Affine::scale_non_uniform(sx, sy)
        * Affine::translate(-fixed.to_vec2())
}

fn clamp_scale(s: f64) -> f64 {
    if s.abs() < MIN_SCALE {
        MIN_SCALE.copysign(if s == 0.0 { 1.0 } else { s })
    } else {
        s
    }
}

/// The angle (radians) the pointer swept around `centre` going from
/// `start` to `now`, rounded to [`ROTATE_STEP`] when `snap` (Shift).
#[must_use]
pub fn rotation_angle(centre: Point, start: Point, now: Point, snap: bool) -> f64 {
    let a = (start - centre).atan2();
    let b = (now - centre).atan2();
    let mut angle = b - a;
    if snap {
        angle = (angle / ROTATE_STEP).round() * ROTATE_STEP;
    }
    angle
}

/// Rotation by `angle` radians about `centre`.
#[must_use]
pub fn rotate(centre: Point, angle: f64) -> Affine {
    Affine::rotate_about(angle, centre)
}

/// Mirror of box `b` about its centre: left ↔ right when `horizontal`,
/// otherwise top ↔ bottom.
#[must_use]
pub fn flip(b: Rect, horizontal: bool) -> Affine {
    let c = b.center().to_vec2();
    let s = if horizontal {
        Affine::scale_non_uniform(-1.0, 1.0)
    } else {
        Affine::scale_non_uniform(1.0, -1.0)
    };
    Affine::translate(c) * s * Affine::translate(-c)
}

/// The rotation of a node on screen, in degrees (−180, 180]: the angle its
/// x axis makes in document space.
#[must_use]
pub fn rotation_degrees(world: Transform) -> f64 {
    let [a, b, ..] = world.0;
    b.atan2(a).to_degrees()
}

/// New local transforms for `ids` after applying `change` (document
/// space) on top of where they are now. Nodes whose parent space cannot be
/// inverted are left out.
#[must_use]
pub fn retarget(doc: &Document, ids: &[NodeId], change: Affine) -> Vec<(NodeId, Transform)> {
    ids.iter()
        .filter_map(|&id| {
            let node = doc.nodes.get(&id)?;
            let parent = to_affine(doc.parent_transform(id));
            if parent.determinant().abs() < EPS {
                return None;
            }
            let local = parent.inverse() * change * parent * to_affine(node.transform);
            let t = from_affine(local);
            t.is_finite().then_some((id, t))
        })
        .collect()
}

/// An edge or centre line to align to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    /// Horizontal centres.
    Centre,
    Right,
    Top,
    /// Vertical centres.
    Middle,
    Bottom,
}

/// How far each box must move to line up with `target` as `how` says.
#[must_use]
pub fn align(boxes: &[Rect], target: Rect, how: Align) -> Vec<Vec2> {
    boxes
        .iter()
        .map(|b| match how {
            Align::Left => Vec2::new(target.x0 - b.x0, 0.0),
            Align::Centre => Vec2::new(target.center().x - b.center().x, 0.0),
            Align::Right => Vec2::new(target.x1 - b.x1, 0.0),
            Align::Top => Vec2::new(0.0, target.y0 - b.y0),
            Align::Middle => Vec2::new(0.0, target.center().y - b.center().y),
            Align::Bottom => Vec2::new(0.0, target.y1 - b.y1),
        })
        .collect()
}

/// How far each box must move so the gaps between neighbours are equal,
/// along x when `horizontal`, otherwise y. The first and last boxes (by
/// position) stay; fewer than three boxes do not move.
#[must_use]
pub fn distribute(boxes: &[Rect], horizontal: bool) -> Vec<Vec2> {
    let mut moves = vec![Vec2::ZERO; boxes.len()];
    if boxes.len() < 3 {
        return moves;
    }
    let lo = |b: &Rect| if horizontal { b.x0 } else { b.y0 };
    let hi = |b: &Rect| if horizontal { b.x1 } else { b.y1 };
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|&a, &b| lo(&boxes[a]).total_cmp(&lo(&boxes[b])));
    let first = lo(&boxes[order[0]]);
    let last = order
        .iter()
        .map(|&i| hi(&boxes[i]))
        .fold(f64::NEG_INFINITY, f64::max);
    let total: f64 = boxes.iter().map(|b| hi(b) - lo(b)).sum();
    #[allow(clippy::cast_precision_loss)] // a handful of boxes
    let gap = (last - first - total) / (boxes.len() - 1) as f64;
    let mut at = first;
    for &i in &order {
        let d = at - lo(&boxes[i]);
        moves[i] = if horizontal {
            Vec2::new(d, 0.0)
        } else {
            Vec2::new(0.0, d)
        };
        at += hi(&boxes[i]) - lo(&boxes[i]) + gap;
    }
    moves
}
