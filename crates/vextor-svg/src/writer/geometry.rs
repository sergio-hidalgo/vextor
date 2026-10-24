//! Shapes as SVG elements (`<rect>`, `<circle>`, `<ellipse>`, `<line>`,
//! `<path>`) and transforms as the shortest `transform` value.

use vextor_core::{CornerRadii, EPS, FillRule, Geometry, Transform, Vec2};
use vextor_geom::{fitted_radii, to_bezpath};

use crate::element::Element;
use crate::number::Num;
use crate::path_data::path_data;

/// The element for `geometry` moved by `offset`, with its geometry
/// attributes only; `None` when it draws nothing (an empty path).
pub(super) fn element(geometry: &Geometry, offset: Vec2, num: Num) -> Option<Element> {
    let (dx, dy) = (offset.x, offset.y);
    match geometry {
        Geometry::Rect { rect, radius } => {
            let r = fitted_radii(*rect, *radius);
            if !uniform(r) {
                return path(geometry, offset, num);
            }
            let mut el = Element::new("rect");
            position(&mut el, ("x", "y"), rect.x + dx, rect.y + dy, num);
            el.set("width", num.fmt(rect.width));
            el.set("height", num.fmt(rect.height));
            if num.units(r.tl) != 0 {
                el.set("rx", num.fmt(r.tl));
            }
            Some(el)
        }
        Geometry::Ellipse { center, rx, ry } => {
            let circle = num.same(*rx, *ry);
            let mut el = Element::new(if circle { "circle" } else { "ellipse" });
            position(&mut el, ("cx", "cy"), center.x + dx, center.y + dy, num);
            if circle {
                el.set("r", num.fmt(*rx));
            } else {
                el.set("rx", num.fmt(*rx));
                el.set("ry", num.fmt(*ry));
            }
            Some(el)
        }
        Geometry::Line { from, to } => {
            let mut el = Element::new("line");
            position(&mut el, ("x1", "y1"), from.x + dx, from.y + dy, num);
            position(&mut el, ("x2", "y2"), to.x + dx, to.y + dy, num);
            Some(el)
        }
        Geometry::Polygon { .. } | Geometry::Star { .. } | Geometry::Path(_) => {
            path(geometry, offset, num)
        }
    }
}

/// `geometry` as a `<path>`.
fn path(geometry: &Geometry, offset: Vec2, num: Num) -> Option<Element> {
    let mut bez = to_bezpath(geometry);
    bez.apply_affine(kurbo::Affine::translate((offset.x, offset.y)));
    let d = path_data(&bez, num);
    if d.is_empty() {
        return None;
    }
    let mut el = Element::new("path");
    el.set("d", d);
    Some(el)
}

/// A point's two attributes, each left out when 0 (the SVG default).
pub(super) fn position(
    el: &mut Element,
    names: (&'static str, &'static str),
    x: f64,
    y: f64,
    num: Num,
) {
    if num.units(x) != 0 {
        el.set(names.0, num.fmt(x));
    }
    if num.units(y) != 0 {
        el.set(names.1, num.fmt(y));
    }
}

fn uniform(r: CornerRadii) -> bool {
    let same = |a: f64, b: f64| (a - b).abs() < EPS;
    same(r.tl, r.tr) && same(r.tl, r.br) && same(r.tl, r.bl)
}

/// Only paths carry a fill rule; the others fill non-zero.
pub(super) fn fill_rule(geometry: &Geometry) -> FillRule {
    match geometry {
        Geometry::Path(p) => p.fill_rule,
        _ => FillRule::NonZero,
    }
}

/// True when `t` only moves (no rotation, scale or skew).
pub(crate) fn is_translation(t: Transform) -> bool {
    let [xx, xy, yx, yy, ..] = t.0;
    (xx - 1.0).abs() < EPS && xy.abs() < EPS && yx.abs() < EPS && (yy - 1.0).abs() < EPS
}

/// The shortest `transform` value for `t`: `translate`, `rotate` (about
/// the point it turns around), `scale`, else `matrix`; `None` for the
/// identity.
#[allow(clippy::many_single_char_names)] // SVG matrix coefficient names
pub(super) fn transform_attr(t: Transform, num: Num) -> Option<String> {
    let [a, b, c, d, e, f] = t.0;
    let zero = |v: f64| v.abs() < EPS;
    // Matrix coefficients and angles need more decimals than positions.
    let fine = num.at_least(num.decimals() + 3);
    if is_translation(t) {
        if num.units(e) == 0 && num.units(f) == 0 {
            return None;
        }
        return Some(if num.units(f) == 0 {
            format!("translate({})", num.fmt(e))
        } else {
            format!("translate({} {})", num.fmt(e), num.fmt(f))
        });
    }
    let rotation = (a - d).abs() < EPS && (b + c).abs() < EPS && (a * a + b * b - 1.0).abs() < EPS;
    if rotation {
        let angle = fine.fmt(b.atan2(a).to_degrees());
        if zero(e) && zero(f) {
            return Some(format!("rotate({angle})"));
        }
        // The fixed point c of x' = R x + t: (I − R) c = t.
        let det = (1.0 - a) * (1.0 - d) - b * c;
        if det.abs() > EPS {
            let cx = ((1.0 - d) * e + c * f) / det;
            let cy = (b * e + (1.0 - a) * f) / det;
            return Some(format!("rotate({angle} {} {})", num.fmt(cx), num.fmt(cy)));
        }
    }
    if zero(b) && zero(c) && zero(e) && zero(f) {
        return Some(if fine.same(a, d) {
            format!("scale({})", fine.fmt(a))
        } else {
            format!("scale({} {})", fine.fmt(a), fine.fmt(d))
        });
    }
    Some(format!(
        "matrix({} {} {} {} {} {})",
        fine.fmt(a),
        fine.fmt(b),
        fine.fmt(c),
        fine.fmt(d),
        num.fmt(e),
        num.fmt(f)
    ))
}

#[cfg(test)]
mod tests {
    use vextor_core::{Point, Rect};

    use super::*;

    const N: Num = Num::new(2);

    fn attrs(el: &Element) -> String {
        let a: Vec<String> = el.attrs.iter().map(|(n, v)| format!("{n}={v}")).collect();
        format!("{} {}", el.name, a.join(" "))
    }

    #[test]
    fn shapes_use_their_own_elements_and_skip_defaults() {
        let rect = Geometry::Rect {
            rect: Rect::new(0.0, 4.0, 10.0, 6.0),
            radius: CornerRadii {
                tl: 2.0,
                tr: 2.0,
                br: 2.0,
                bl: 2.0,
            },
        };
        let el = element(&rect, Vec2::new(0.0, 0.0), N).expect("draws");
        assert_eq!(attrs(&el), "rect y=4 width=10 height=6 rx=2");
        let circle = Geometry::Ellipse {
            center: Point::new(5.0, 5.0),
            rx: 3.0,
            ry: 3.0,
        };
        let el = element(&circle, Vec2::new(1.0, 0.0), N).expect("draws");
        assert_eq!(attrs(&el), "circle cx=6 cy=5 r=3");
        let line = Geometry::Line {
            from: Point::new(0.0, 0.0),
            to: Point::new(8.0, 0.0),
        };
        let el = element(&line, Vec2::new(0.0, 0.0), N).expect("draws");
        assert_eq!(attrs(&el), "line x2=8");
    }

    #[test]
    fn mixed_corner_radii_become_a_path() {
        let rect = Geometry::Rect {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            radius: CornerRadii {
                tl: 2.0,
                ..CornerRadii::default()
            },
        };
        assert_eq!(
            element(&rect, Vec2::new(0.0, 0.0), N).expect("draws").name,
            "path"
        );
    }

    #[test]
    fn a_radius_too_big_for_the_rect_is_fitted_like_the_canvas() {
        let rect = Geometry::Rect {
            rect: Rect::new(0.0, 0.0, 10.0, 4.0),
            radius: CornerRadii {
                tl: 5.0,
                tr: 5.0,
                br: 5.0,
                bl: 5.0,
            },
        };
        let el = element(&rect, Vec2::new(0.0, 0.0), N).expect("draws");
        assert_eq!(el.get("rx"), Some("2"));
    }

    #[test]
    fn transforms_take_their_shortest_form() {
        assert_eq!(transform_attr(Transform::IDENTITY, N), None);
        assert_eq!(
            transform_attr(Transform::translate(3.0, 0.0), N).as_deref(),
            Some("translate(3)")
        );
        let quarter = Transform([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]);
        assert_eq!(transform_attr(quarter, N).as_deref(), Some("rotate(90)"));
        // A quarter turn about (10, 10).
        let about = Transform::translate(-10.0, -10.0)
            .then(quarter)
            .then(Transform::translate(10.0, 10.0));
        assert_eq!(
            transform_attr(about, N).as_deref(),
            Some("rotate(90 10 10)")
        );
        assert_eq!(
            transform_attr(Transform::scale(-1.0, 1.0), N).as_deref(),
            Some("scale(-1 1)")
        );
        let skew = Transform([1.0, 0.0, 0.5, 1.0, 2.0, 0.0]);
        assert_eq!(
            transform_attr(skew, N).as_deref(),
            Some("matrix(1 0 .5 1 2 0)")
        );
    }
}
