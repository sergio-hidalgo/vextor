//! Shape tool geometry (spec 06): what a drag or the size dialog draws,
//! and the rect corner widgets' radius maths. Pure functions in document
//! (or the shape's own) coordinates, so the tools stay thin.

use std::f64::consts::FRAC_PI_4;

use kurbo::{Point, Vec2};
use vextor_core::{CornerRadii, EPS, Geometry, MAX_POLYGON_SIDES, MAX_STAR_POINTS, Rect};

/// What a shape tool draws, with the options set while drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShapeKind {
    Rect,
    Ellipse,
    Polygon {
        sides: u32,
    },
    /// `inner` is the inner radius as a share of the outer one (0..1).
    Star {
        points: u32,
        inner: f64,
    },
    Line,
}

/// The modifiers held while dragging.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawMods {
    /// Shift: square, circle, or a line at a multiple of 45°.
    pub constrain: bool,
    /// Alt: the press point is the centre.
    pub from_centre: bool,
}

/// The values of the size dialog (a click without a drag).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShapeSize {
    Rect {
        width: f64,
        height: f64,
    },
    Ellipse {
        width: f64,
        height: f64,
    },
    Polygon {
        radius: f64,
        sides: u32,
    },
    Star {
        outer: f64,
        inner: f64,
        points: u32,
    },
    /// `angle` in degrees, clockwise from the +x axis (y points down).
    Line {
        length: f64,
        angle: f64,
    },
}

/// A corner of a rect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

impl Corner {
    /// All four, in CSS `border-radius` order.
    pub const ALL: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomRight,
        Self::BottomLeft,
    ];

    /// The corner's point of `r`, and the unit-per-axis direction pointing
    /// into the rect from it.
    fn of(self, r: Rect) -> (Point, Vec2) {
        let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.width, r.y + r.height);
        match self {
            Self::TopLeft => (Point::new(x0, y0), Vec2::new(1.0, 1.0)),
            Self::TopRight => (Point::new(x1, y0), Vec2::new(-1.0, 1.0)),
            Self::BottomRight => (Point::new(x1, y1), Vec2::new(-1.0, -1.0)),
            Self::BottomLeft => (Point::new(x0, y1), Vec2::new(1.0, -1.0)),
        }
    }

    /// This corner's radius in `radii`.
    #[must_use]
    pub fn radius(self, radii: CornerRadii) -> f64 {
        match self {
            Self::TopLeft => radii.tl,
            Self::TopRight => radii.tr,
            Self::BottomRight => radii.br,
            Self::BottomLeft => radii.bl,
        }
    }

    /// `radii` with this corner set to `value`.
    #[must_use]
    pub fn with(self, radii: CornerRadii, value: f64) -> CornerRadii {
        let mut r = radii;
        match self {
            Self::TopLeft => r.tl = value,
            Self::TopRight => r.tr = value,
            Self::BottomRight => r.br = value,
            Self::BottomLeft => r.bl = value,
        }
        r
    }
}

/// The shape a drag from `start` to `end` draws, or `None` while it has no
/// size yet (a click, or a rect flat along one axis).
#[must_use]
pub fn drawn(kind: ShapeKind, start: Point, end: Point, mods: DrawMods) -> Option<Geometry> {
    let d = end - start;
    match kind {
        ShapeKind::Rect | ShapeKind::Ellipse => {
            let d = if mods.constrain { square(d) } else { d };
            let (x0, x1) = span(start.x, d.x, mods.from_centre);
            let (y0, y1) = span(start.y, d.y, mods.from_centre);
            box_geometry(
                kind,
                x0.min(x1),
                y0.min(y1),
                (x1 - x0).abs(),
                (y1 - y0).abs(),
            )
        }
        ShapeKind::Polygon { .. } | ShapeKind::Star { .. } => {
            // Regular shapes: the drag spans a square, or a radius.
            let (center, radius) = if mods.from_centre {
                (start, d.hypot())
            } else {
                let s = square(d);
                (start + s / 2.0, s.x.abs() / 2.0)
            };
            regular(kind, center, radius)
        }
        ShapeKind::Line => {
            let d = if mods.constrain { snap_45(d) } else { d };
            let from = if mods.from_centre { start - d } else { start };
            line(from, start + d)
        }
    }
}

/// The shape the size dialog places at `at`: its top-left corner (rects,
/// ellipses), its centre (polygons, stars, or with `centred`) or its first
/// end (lines). `None` when a size is zero or not finite.
#[must_use]
pub fn sized(size: ShapeSize, at: Point, centred: bool) -> Option<Geometry> {
    match size {
        ShapeSize::Rect { width, height } | ShapeSize::Ellipse { width, height } => {
            let kind = if matches!(size, ShapeSize::Rect { .. }) {
                ShapeKind::Rect
            } else {
                ShapeKind::Ellipse
            };
            let (x, y) = if centred {
                (at.x - width / 2.0, at.y - height / 2.0)
            } else {
                (at.x, at.y)
            };
            box_geometry(kind, x, y, width, height)
        }
        ShapeSize::Polygon { radius, sides } => regular(ShapeKind::Polygon { sides }, at, radius),
        ShapeSize::Star {
            outer,
            inner,
            points,
        } => {
            if !(outer > EPS && outer.is_finite()) {
                return None;
            }
            let inner = inner / outer;
            regular(ShapeKind::Star { points, inner }, at, outer)
        }
        ShapeSize::Line { length, angle } => {
            let rad = angle.to_radians();
            let d = Vec2::new(rad.cos(), rad.sin()) * length;
            let from = if centred { at - d / 2.0 } else { at };
            line(from, from + d)
        }
    }
}

/// The radius of `corner` after dragging its widget from `from` to `to`
/// (the rect's own coordinates), starting at `start` — the radius drawn
/// when the drag began. Moving along the corner's diagonal by `d` changes
/// the radius by `d / √2`; it stays within 0 and half the shorter side.
#[must_use]
pub fn corner_radius_at(rect: Rect, corner: Corner, start: f64, from: Point, to: Point) -> f64 {
    let (_, inward) = corner.of(rect);
    let r = start + (to - from).dot(inward) / 2.0;
    r.clamp(0.0, max_radius(rect))
}

/// Where `corner`'s widget sits: on the diagonal, as far in as the drawn
/// radius, but at least `min_inset` in (so it never sits on a scale
/// handle) and at most at the centre. In the rect's own coordinates.
#[must_use]
pub fn corner_widget(rect: Rect, radii: CornerRadii, corner: Corner, min_inset: f64) -> Point {
    let (at, inward) = corner.of(rect);
    let drawn = corner.radius(crate::fitted_radii(rect, radii));
    let inset = drawn.max(min_inset).min(max_radius(rect));
    at + inward * inset
}

/// The largest useful corner radius: half the shorter side.
fn max_radius(rect: Rect) -> f64 {
    rect.width.min(rect.height).max(0.0) / 2.0
}

/// `d` grown to a square along its longer axis, keeping both signs.
fn square(d: Vec2) -> Vec2 {
    let s = d.x.abs().max(d.y.abs());
    Vec2::new(s.copysign(d.x), s.copysign(d.y))
}

/// `d` turned to the nearest multiple of 45°, keeping its length.
fn snap_45(d: Vec2) -> Vec2 {
    let len = d.hypot();
    let angle = (d.y.atan2(d.x) / FRAC_PI_4).round() * FRAC_PI_4;
    Vec2::new(angle.cos(), angle.sin()) * len
}

/// The two ends along one axis: from `start` by `d`, or `start ± d`.
fn span(start: f64, d: f64, from_centre: bool) -> (f64, f64) {
    if from_centre {
        (start - d, start + d)
    } else {
        (start, start + d)
    }
}

fn valid(v: f64) -> bool {
    v.is_finite() && v > EPS
}

fn box_geometry(kind: ShapeKind, x: f64, y: f64, w: f64, h: f64) -> Option<Geometry> {
    if !(valid(w) && valid(h) && x.is_finite() && y.is_finite()) {
        return None;
    }
    Some(match kind {
        ShapeKind::Ellipse => Geometry::Ellipse {
            center: vextor_core::Point::new(x + w / 2.0, y + h / 2.0),
            rx: w / 2.0,
            ry: h / 2.0,
        },
        _ => Geometry::Rect {
            rect: Rect::new(x, y, w, h),
            radius: CornerRadii::default(),
        },
    })
}

fn regular(kind: ShapeKind, center: Point, radius: f64) -> Option<Geometry> {
    if !(valid(radius) && center.x.is_finite() && center.y.is_finite()) {
        return None;
    }
    let center = vextor_core::Point::new(center.x, center.y);
    match kind {
        ShapeKind::Polygon { sides } => Some(Geometry::Polygon {
            center,
            radius,
            sides: sides.clamp(3, MAX_POLYGON_SIDES),
            corner_radius: 0.0,
        }),
        ShapeKind::Star { points, inner } => {
            let inner = radius * inner;
            valid(inner).then_some(Geometry::Star {
                center,
                outer: radius,
                inner: inner.min(radius),
                points: points.clamp(3, MAX_STAR_POINTS),
                corner_radius: 0.0,
            })
        }
        _ => None,
    }
}

fn line(from: Point, to: Point) -> Option<Geometry> {
    let ok = [from.x, from.y, to.x, to.y].iter().all(|v| v.is_finite());
    (ok && valid((to - from).hypot())).then(|| Geometry::Line {
        from: vextor_core::Point::new(from.x, from.y),
        to: vextor_core::Point::new(to.x, to.y),
    })
}
