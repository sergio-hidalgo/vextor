//! Paints and strokes.
//!
//! `usvg` resolves every gradient to user space, folding bounding-box
//! units and `gradientTransform` into one transform. The model has no
//! gradient transform, so the gradient is brought back: bounding-box units
//! when the transform is exactly the object's box (what the writer
//! emits), else user space with the transform applied to its points —
//! exact for linear gradients and for radial ones under rotation and
//! uniform scale, approximated otherwise.

use std::sync::Arc;

use vextor_core::{
    EPS, Gradient, GradientKind, GradientStop, GradientUnits, LineCap, LineJoin, Paint, Point,
    Resource, Rgba, Spread, Stroke, Transform,
};

use super::nodes::{Mapper, convert};
use super::parse::CURRENT_COLOR;

impl Mapper<'_> {
    /// `paint` with `opacity` (fill or stroke opacity), on an object whose
    /// box in its own space is `bbox`.
    pub(super) fn paint(
        &mut self,
        paint: &usvg::Paint,
        opacity: usvg::Opacity,
        bbox: kurbo::Rect,
    ) -> Paint {
        let opacity = f64::from(opacity.get());
        match paint {
            usvg::Paint::Color(c) if [c.red, c.green, c.blue] == current() => {
                // `currentColor` carries no alpha in the model.
                if opacity < 1.0 - EPS {
                    self.report.approximated += 1;
                }
                Paint::CurrentColor
            }
            usvg::Paint::Color(c) => Paint::Solid(Rgba {
                r: c.red,
                g: c.green,
                b: c.blue,
                a: alpha(opacity),
            }),
            usvg::Paint::LinearGradient(g) => {
                let kind = GradientKind::Linear {
                    start: pt(g.x1(), g.y1()),
                    end: pt(g.x2(), g.y2()),
                };
                self.gradient(g, kind, 0.0, opacity, bbox)
            }
            usvg::Paint::RadialGradient(g) => {
                let kind = GradientKind::Radial {
                    center: pt(g.cx(), g.cy()),
                    radius: f64::from(g.r().get()),
                    focal: Some(pt(g.fx(), g.fy())),
                };
                self.gradient(g, kind, f64::from(g.fr().get()), opacity, bbox)
            }
            usvg::Paint::Pattern(p) => {
                if opacity < 1.0 - EPS {
                    self.report.approximated += 1;
                }
                self.pattern_paint(p)
            }
        }
    }

    fn pattern_paint(&mut self, p: &Arc<usvg::Pattern>) -> Paint {
        self.pattern(p).map_or(Paint::None, Paint::Pattern)
    }

    fn gradient(
        &mut self,
        g: &usvg::BaseGradient,
        kind: GradientKind,
        focal_radius: f64,
        opacity: f64,
        bbox: kurbo::Rect,
    ) -> Paint {
        let t = convert(g.transform());
        let (units, kind, exact) = if is_box(t, bbox) {
            (GradientUnits::ObjectBoundingBox, kind, true)
        } else {
            let (kind, exact) = transformed(kind, t);
            (GradientUnits::UserSpace, kind, exact)
        };
        if !exact || focal_radius > EPS {
            self.report.approximated += 1;
        }
        let kind = match kind {
            GradientKind::Radial {
                center,
                radius,
                focal,
            } => GradientKind::Radial {
                center,
                radius,
                focal: focal.filter(|f| (f.x - center.x).hypot(f.y - center.y) > EPS),
            },
            linear @ GradientKind::Linear { .. } => linear,
        };
        let mut stops: Vec<GradientStop> = g
            .stops()
            .iter()
            .map(|s| GradientStop {
                offset: f64::from(s.offset().get()).clamp(0.0, 1.0),
                color: {
                    // A stop in `currentColor` has no CSS colour to take
                    // here: black, as a page without `color` draws it.
                    let c = s.color();
                    let [red, green, blue] = if [c.red, c.green, c.blue] == current() {
                        [0, 0, 0]
                    } else {
                        [c.red, c.green, c.blue]
                    };
                    Rgba {
                        r: red,
                        g: green,
                        b: blue,
                        a: alpha(f64::from(s.opacity().get()) * opacity),
                    }
                },
            })
            .collect();
        // usvg keeps offsets ascending; a lone stop is drawn as two.
        if stops.len() == 1 {
            stops.push(stops[0]);
        }
        let Some(first) = stops.first().copied() else {
            return Paint::None;
        };
        if !stops_valid(&stops) {
            return Paint::Solid(first.color);
        }
        let gradient = Gradient {
            name: self.name(g.id()),
            kind,
            stops,
            units,
            spread: match g.spread_method() {
                usvg::SpreadMethod::Pad => Spread::Pad,
                usvg::SpreadMethod::Reflect => Spread::Reflect,
                usvg::SpreadMethod::Repeat => Spread::Repeat,
            },
        };
        // usvg copies a gradient for each object it paints in bounding-box
        // units; back in those units the copies are one gradient again.
        let unnamed = Gradient {
            name: None,
            ..gradient.clone()
        };
        if let Some(&(_, id)) = self.gradients.iter().find(|(g, _)| *g == unnamed) {
            self.name_resource(id, gradient.name);
            return Paint::Gradient(id);
        }
        let id = self.resource_id();
        self.gradients.push((unnamed, id));
        self.add_resource(id, Resource::Gradient(gradient), Vec::new());
        Paint::Gradient(id)
    }

    pub(super) fn stroke(&mut self, s: &usvg::Stroke, bbox: kurbo::Rect) -> Option<Stroke> {
        let paint = self.paint(s.paint(), s.opacity(), bbox);
        let width = f64::from(s.width().get());
        if paint == Paint::None || !width.is_finite() {
            return None;
        }
        Some(Stroke {
            paint,
            width,
            cap: match s.linecap() {
                usvg::LineCap::Butt => LineCap::Butt,
                usvg::LineCap::Round => LineCap::Round,
                usvg::LineCap::Square => LineCap::Square,
            },
            join: match s.linejoin() {
                usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => LineJoin::Miter,
                usvg::LineJoin::Round => LineJoin::Round,
                usvg::LineJoin::Bevel => LineJoin::Bevel,
            },
            miter_limit: f64::from(s.miterlimit().get()).max(1.0),
            dash: s
                .dasharray()
                .map(|d| d.iter().map(|&v| f64::from(v).max(0.0)).collect())
                .unwrap_or_default(),
            dash_offset: f64::from(s.dashoffset()),
        })
    }
}

/// The sentinel `currentColor` resolves to during import.
fn current() -> [u8; 3] {
    [CURRENT_COLOR.r, CURRENT_COLOR.g, CURRENT_COLOR.b]
}

fn pt(x: f32, y: f32) -> Point {
    Point::new(f64::from(x), f64::from(y))
}

/// 0..1 as an 8-bit alpha.
fn alpha(opacity: f64) -> u8 {
    // Clamped to 0..=255 first, so the cast cannot truncate.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let a = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    a
}

fn stops_valid(stops: &[GradientStop]) -> bool {
    stops.len() >= 2 && stops.windows(2).all(|w| w[0].offset <= w[1].offset)
}

/// True when `t` stretches 0..1 over `bbox` (bounding-box units).
#[allow(clippy::many_single_char_names)] // SVG matrix coefficient names
fn is_box(t: Transform, bbox: kurbo::Rect) -> bool {
    let [a, b, c, d, e, f] = t.0;
    let near = |v: f64, want: f64| (v - want).abs() <= 1e-4 * want.abs().max(1.0);
    bbox.width() > EPS
        && bbox.height() > EPS
        && near(a, bbox.width())
        && near(d, bbox.height())
        && near(b, 0.0)
        && near(c, 0.0)
        && near(e, bbox.x0)
        && near(f, bbox.y0)
}

/// `kind` with `t` applied, in user space; false when that is not exact.
#[allow(clippy::many_single_char_names)] // SVG matrix coefficient names
fn transformed(kind: GradientKind, t: Transform) -> (GradientKind, bool) {
    let [a, b, c, d, ..] = t.0;
    let identity = (a - 1.0).abs() < EPS && b.abs() < EPS && c.abs() < EPS && (d - 1.0).abs() < EPS;
    let [.., e, f] = t.0;
    if identity && e.abs() < EPS && f.abs() < EPS {
        return (kind, true);
    }
    match kind {
        GradientKind::Linear { start, end } => {
            // The gradient's value is linear in the point, so one user-space
            // line has the same isolines: through T(start), along the
            // direction the value grows fastest (inverse transpose of T).
            let p1 = t.apply(start);
            let (dx, dy) = (end.x - start.x, end.y - start.y);
            let det = a * d - b * c;
            if det.abs() < EPS {
                return (
                    GradientKind::Linear {
                        start: p1,
                        end: t.apply(end),
                    },
                    false,
                );
            }
            let n = ((d * dx - b * dy) / det, (a * dy - c * dx) / det);
            let nn = n.0 * n.0 + n.1 * n.1;
            if nn < EPS * EPS {
                return (GradientKind::Linear { start: p1, end: p1 }, false);
            }
            let k = (dx * dx + dy * dy) / nn;
            let end = Point::new(p1.x + n.0 * k, p1.y + n.1 * k);
            (GradientKind::Linear { start: p1, end }, true)
        }
        GradientKind::Radial {
            center,
            radius,
            focal,
        } => {
            // Rotation and uniform scale (mirrored or not) keep circles.
            let scale = (a * d - b * c).abs().sqrt();
            // usvg works in f32: compare relative to the scale.
            let tol = 1e-5 * scale.max(1.0);
            let similar = ((a - d).abs() < tol && (b + c).abs() < tol)
                || ((a + d).abs() < tol && (b - c).abs() < tol);
            let kind = GradientKind::Radial {
                center: t.apply(center),
                radius: radius * scale,
                focal: focal.map(|p| t.apply(p)),
            };
            (kind, similar)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The value (0..1 before spreading) a linear gradient has at `p`.
    fn value(start: Point, end: Point, p: Point) -> f64 {
        let (vx, vy) = (end.x - start.x, end.y - start.y);
        ((p.x - start.x) * vx + (p.y - start.y) * vy) / (vx * vx + vy * vy)
    }

    #[test]
    fn a_skewed_linear_gradient_keeps_its_values_in_user_space() {
        let t = Transform([2.0, 0.5, 0.7, 3.0, 10.0, -4.0]);
        let (start, end) = (Point::new(0.1, 0.2), Point::new(0.9, 0.4));
        let (kind, exact) = transformed(GradientKind::Linear { start, end }, t);
        assert!(exact);
        let GradientKind::Linear { start: s2, end: e2 } = kind else {
            panic!("linear")
        };
        for p in [
            Point::new(0.3, 0.7),
            Point::new(-1.0, 2.0),
            Point::new(5.0, 5.0),
        ] {
            let want = value(start, end, p);
            let got = value(s2, e2, t.apply(p));
            assert!((want - got).abs() < 1e-9, "{want} vs {got}");
        }
    }

    #[test]
    fn a_stretched_radial_gradient_is_approximated() {
        let kind = GradientKind::Radial {
            center: Point::new(0.5, 0.5),
            radius: 0.5,
            focal: None,
        };
        let (_, exact) = transformed(kind.clone(), Transform([1.0, 0.0, 0.0, 1.0, 3.0, 4.0]));
        assert!(exact);
        let (_, exact) = transformed(kind, Transform([20.0, 0.0, 0.0, 10.0, 0.0, 0.0]));
        assert!(!exact);
    }

    #[test]
    fn the_object_box_is_recognised_as_bounding_box_units() {
        let bbox = kurbo::Rect::new(10.0, 20.0, 50.0, 30.0);
        assert!(is_box(Transform([40.0, 0.0, 0.0, 10.0, 10.0, 20.0]), bbox));
        assert!(!is_box(Transform::IDENTITY, bbox));
    }
}
