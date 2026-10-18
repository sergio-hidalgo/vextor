//! Serialisable geometry primitives. `vextor-geom` converts them to `kurbo`.

use std::ops::{Add, Neg, Sub};

use serde::{Deserialize, Serialize};

/// Tolerance for floating-point comparisons, in document units (CSS px).
/// The one place this value lives; `vextor_geom::EPS` re-exports it.
pub const EPS: f64 = 1e-6;

/// A position in document space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A displacement (e.g. a path handle relative to its anchor).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

/// An axis-aligned rectangle: top-left corner plus size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// 2×3 affine matrix `[a b c d e f]`, SVG order: maps `(x, y)` to
/// `(a·x + c·y + e, b·x + d·y + f)`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform(pub [f64; 6]);

impl Point {
    /// Creates a point.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// True when both coordinates are finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// This point moved by `v`.
    #[must_use]
    pub fn offset(self, v: Vec2) -> Self {
        Self::new(self.x + v.x, self.y + v.y)
    }
}

impl Vec2 {
    /// Creates a vector.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// True when both components are finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Euclidean length.
    #[must_use]
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    /// This vector multiplied by `k`.
    #[must_use]
    pub fn scale(self, k: f64) -> Self {
        Self::new(self.x * k, self.y * k)
    }

    /// 2D cross product (z of the 3D cross).
    #[must_use]
    pub fn cross(self, other: Self) -> f64 {
        self.x * other.y - self.y * other.x
    }

    /// Dot product.
    #[must_use]
    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// Unit vector in the same direction, or `None` when shorter than [`EPS`].
    #[must_use]
    pub fn normalized(self) -> Option<Self> {
        let len = self.length();
        (len > EPS).then(|| self.scale(1.0 / len))
    }
}

impl Rect {
    /// Creates a rectangle.
    #[must_use]
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// True when every field is finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite())
    }
}

impl Transform {
    /// The identity transform.
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// A pure translation.
    #[must_use]
    pub const fn translate(x: f64, y: f64) -> Self {
        Self([1.0, 0.0, 0.0, 1.0, x, y])
    }

    /// A pure scale about the origin.
    #[must_use]
    pub const fn scale(sx: f64, sy: f64) -> Self {
        Self([sx, 0.0, 0.0, sy, 0.0, 0.0])
    }

    /// True when every coefficient is finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.0.iter().all(|v| v.is_finite())
    }

    /// The transform that applies `self` first, then `outer`
    /// (matrix product `outer · self`).
    #[must_use]
    #[allow(clippy::many_single_char_names)] // SVG matrix coefficient names
    pub fn then(self, outer: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [ma, mb, mc, md, me, mf] = outer.0;
        Self([
            ma * a + mc * b,
            mb * a + md * b,
            ma * c + mc * d,
            mb * c + md * d,
            ma * e + mc * f + me,
            mb * e + md * f + mf,
        ])
    }

    /// Maps a point through this transform.
    #[must_use]
    #[allow(clippy::many_single_char_names)] // SVG matrix coefficient names
    pub fn apply(self, p: Point) -> Point {
        let [a, b, c, d, e, f] = self.0;
        Point::new(a * p.x + c * p.y + e, b * p.x + d * p.y + f)
    }
}

impl Sub for Point {
    type Output = Vec2;

    /// The vector from `rhs` to `self`.
    fn sub(self, rhs: Self) -> Vec2 {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Add for Vec2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Neg for Vec2 {
    type Output = Self;

    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn then_applies_inner_first() {
        let t = Transform::scale(2.0, 2.0).then(Transform::translate(10.0, 0.0));
        assert_eq!(t.apply(Point::new(1.0, 1.0)), Point::new(12.0, 2.0));
        let u = Transform::translate(10.0, 0.0).then(Transform::scale(2.0, 2.0));
        assert_eq!(u.apply(Point::new(1.0, 1.0)), Point::new(22.0, 2.0));
    }

    #[test]
    fn identity_is_neutral() {
        let t = Transform([1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(t.then(Transform::IDENTITY), t);
        assert_eq!(Transform::IDENTITY.then(t), t);
    }

    #[test]
    fn short_vectors_do_not_normalize() {
        assert_eq!(Vec2::new(0.0, 0.0).normalized(), None);
        let unit = Vec2::new(3.0, 4.0).normalized().expect("long enough");
        assert!((unit.x - 0.6).abs() < EPS && (unit.y - 0.8).abs() < EPS);
    }
}
