//! Model → `tiny-skia` conversions. The only place geometry leaves `f64`:
//! the rasteriser works in `f32`.

#![allow(clippy::cast_possible_truncation)] // f64 → f32 at the rasteriser boundary

use kurbo::{BezPath, PathEl};
use vextor_core::{
    EPS, FillRule, Geometry, Gradient, GradientKind, LineCap, LineJoin, Paint, Rect, Rgba, Spread,
    Stroke, Transform,
};

/// A model transform as a `tiny-skia` transform.
#[allow(clippy::many_single_char_names)] // SVG matrix coefficient names
pub(crate) fn transform(t: Transform) -> tiny_skia::Transform {
    let [a, b, c, d, e, f] = t.0;
    tiny_skia::Transform::from_row(a as f32, b as f32, c as f32, d as f32, e as f32, f as f32)
}

/// A model rectangle as a `tiny-skia` one; `None` when it has no area.
pub(crate) fn rect(r: Rect) -> Option<tiny_skia::Rect> {
    tiny_skia::Rect::from_xywh(r.x as f32, r.y as f32, r.width as f32, r.height as f32)
}

/// A Bézier path as a `tiny-skia` path; `None` when it has no segments.
pub(crate) fn path(bez: &BezPath) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    for el in bez.elements() {
        match *el {
            PathEl::MoveTo(p) => pb.move_to(p.x as f32, p.y as f32),
            PathEl::LineTo(p) => pb.line_to(p.x as f32, p.y as f32),
            PathEl::QuadTo(c, p) => pb.quad_to(c.x as f32, c.y as f32, p.x as f32, p.y as f32),
            PathEl::CurveTo(c1, c2, p) => pb.cubic_to(
                c1.x as f32,
                c1.y as f32,
                c2.x as f32,
                c2.y as f32,
                p.x as f32,
                p.y as f32,
            ),
            PathEl::ClosePath => pb.close(),
        }
    }
    pb.finish()
}

/// The fill rule of a geometry: only paths carry one, SVG default otherwise.
pub(crate) fn fill_rule(geometry: &Geometry) -> tiny_skia::FillRule {
    match geometry {
        Geometry::Path(p) if p.fill_rule == FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
        _ => tiny_skia::FillRule::Winding,
    }
}

/// A solid paint, or `None` when it draws nothing (no paint, a fully
/// transparent colour, or a gradient or pattern: see [`gradient`] and the
/// renderer's pattern tiles). `currentColor` shows as black, the CSS
/// default text colour.
pub(crate) fn solid(paint: Paint) -> Option<tiny_skia::Paint<'static>> {
    let color = match paint {
        Paint::Solid(c) => c,
        Paint::CurrentColor => Rgba::BLACK,
        Paint::None | Paint::Gradient(_) | Paint::Pattern(_) => return None,
    };
    if color.a == 0 {
        return None;
    }
    let mut p = tiny_skia::Paint::default();
    p.set_color_rgba8(color.r, color.g, color.b, color.a);
    p.anti_alias = true;
    Some(p)
}

/// A paint drawing `shader`, anti-aliased.
pub(crate) fn shaded(shader: tiny_skia::Shader<'_>) -> tiny_skia::Paint<'_> {
    tiny_skia::Paint {
        shader,
        anti_alias: true,
        ..tiny_skia::Paint::default()
    }
}

/// The shader of a gradient painting an object whose geometry bounds are
/// `bbox` (its own coordinates), SVG semantics: bounding-box units map
/// 0..1 onto `bbox` and paint nothing when it is flat; a gradient without
/// length or radius paints its last stop's colour.
pub(crate) fn gradient(g: &Gradient, bbox: kurbo::Rect) -> Option<tiny_skia::Shader<'static>> {
    let units = transform(vextor_geom::gradient_space(g.units, bbox)?);
    let color = |c: Rgba| tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a);
    let stops: Vec<tiny_skia::GradientStop> = g
        .stops
        .iter()
        .map(|s| tiny_skia::GradientStop::new(s.offset as f32, color(s.color)))
        .collect();
    let last = color(g.stops.last()?.color);
    let mode = match g.spread {
        Spread::Pad => tiny_skia::SpreadMode::Pad,
        Spread::Reflect => tiny_skia::SpreadMode::Reflect,
        Spread::Repeat => tiny_skia::SpreadMode::Repeat,
    };
    let point = |p: vextor_core::Point| tiny_skia::Point::from_xy(p.x as f32, p.y as f32);
    let shader = match g.kind {
        GradientKind::Linear { start, end } => {
            if (end - start).length() <= EPS {
                return Some(tiny_skia::Shader::SolidColor(last));
            }
            tiny_skia::LinearGradient::new(point(start), point(end), stops, mode, units)
        }
        GradientKind::Radial {
            center,
            radius,
            focal,
        } => {
            if radius <= EPS {
                return Some(tiny_skia::Shader::SolidColor(last));
            }
            let focal = focal.unwrap_or(center);
            tiny_skia::RadialGradient::new(
                point(focal),
                0.0,
                point(center),
                radius as f32,
                stops,
                mode,
                units,
            )
        }
    };
    // tiny-skia refuses only degenerate cases, handled above like SVG.
    Some(shader.unwrap_or(tiny_skia::Shader::SolidColor(last)))
}

/// A model stroke as a `tiny-skia` stroke, SVG semantics: an odd dash list
/// repeats once, an invalid or all-zero one draws solid.
pub(crate) fn stroke(s: &Stroke) -> tiny_skia::Stroke {
    let mut dash: Vec<f32> = s.dash.iter().map(|&v| v as f32).collect();
    if dash.len() % 2 == 1 {
        dash.extend_from_within(..);
    }
    tiny_skia::Stroke {
        width: s.width as f32,
        miter_limit: s.miter_limit as f32,
        line_cap: match s.cap {
            LineCap::Butt => tiny_skia::LineCap::Butt,
            LineCap::Round => tiny_skia::LineCap::Round,
            LineCap::Square => tiny_skia::LineCap::Square,
        },
        line_join: match s.join {
            LineJoin::Miter => tiny_skia::LineJoin::Miter,
            LineJoin::Round => tiny_skia::LineJoin::Round,
            LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
        },
        dash: tiny_skia::StrokeDash::new(dash, s.dash_offset as f32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_keeps_svg_coefficient_order() {
        let t = transform(Transform([1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));
        assert_eq!(
            (t.sx, t.ky, t.kx, t.sy, t.tx, t.ty),
            (1.0, 2.0, 3.0, 4.0, 5.0, 6.0)
        );
    }

    #[test]
    fn odd_dash_list_repeats_like_svg() {
        let s = Stroke {
            dash: vec![4.0, 2.0, 1.0],
            ..Stroke::default()
        };
        assert!(stroke(&s).dash.is_some());
    }

    #[test]
    fn zero_dash_list_draws_solid() {
        let s = Stroke {
            dash: vec![0.0, 0.0],
            ..Stroke::default()
        };
        assert!(stroke(&s).dash.is_none());
    }

    #[test]
    fn unsupported_and_transparent_paints_draw_nothing() {
        assert!(solid(Paint::None).is_none());
        assert!(
            solid(Paint::Solid(Rgba {
                a: 0,
                ..Rgba::BLACK
            }))
            .is_none()
        );
        assert!(solid(Paint::CurrentColor).is_some());
    }

    #[test]
    fn empty_path_converts_to_none() {
        assert!(path(&BezPath::new()).is_none());
    }
}
