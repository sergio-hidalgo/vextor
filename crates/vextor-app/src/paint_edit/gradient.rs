//! Gradient stops, dash presets and pattern tile transforms (spec 09).

use vextor_core::{
    EPS, Gradient, GradientKind, GradientStop, GradientUnits, LineCap, Point, Rgba, Spread, Stroke,
    Transform,
};

/// Linear or radial.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientShape {
    Linear,
    Radial,
}

/// Most values a custom dash may have.
const MAX_DASH_VALUES: usize = 16;

/// A new gradient of `shape` stretched over the object (bounding-box
/// units): `like`'s stops and spread when given, else `color` fading to
/// white. Linear runs left to right; radial fills the box from its centre.
#[must_use]
pub fn new_gradient(shape: GradientShape, color: Rgba, like: Option<&Gradient>) -> Gradient {
    let kind = match shape {
        GradientShape::Linear => GradientKind::Linear {
            start: Point::new(0.0, 0.5),
            end: Point::new(1.0, 0.5),
        },
        GradientShape::Radial => GradientKind::Radial {
            center: Point::new(0.5, 0.5),
            radius: 0.5,
            focal: None,
        },
    };
    let (stops, spread) = like.map_or_else(
        || {
            let stops = vec![
                GradientStop { offset: 0.0, color },
                GradientStop {
                    offset: 1.0,
                    color: Rgba::rgb(255, 255, 255),
                },
            ];
            (stops, Spread::Pad)
        },
        |g| (g.stops.clone(), g.spread),
    );
    Gradient {
        name: None,
        kind,
        stops,
        units: GradientUnits::ObjectBoundingBox,
        spread,
    }
}

/// The colour of `g` at `offset`, blending its two neighbouring stops.
fn color_at(g: &Gradient, offset: f64) -> Rgba {
    let after = g.stops.iter().position(|s| s.offset >= offset);
    let (a, b) = match after {
        Some(0) => return g.stops[0].color,
        Some(i) => (g.stops[i - 1], g.stops[i]),
        None => return g.stops.last().map_or(Rgba::BLACK, |s| s.color),
    };
    let span = b.offset - a.offset;
    let t = if span > EPS {
        (offset - a.offset) / span
    } else {
        0.0
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=255
    let mix = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round() as u8;
    Rgba {
        r: mix(a.color.r, b.color.r),
        g: mix(a.color.g, b.color.g),
        b: mix(a.color.b, b.color.b),
        a: mix(a.color.a, b.color.a),
    }
}

/// `g` with a new stop at `offset` (clamped to 0..=1) in the colour the
/// gradient has there, and the new stop's index.
#[must_use]
pub fn add_stop(g: &Gradient, offset: f64) -> (Gradient, usize) {
    let offset = offset.clamp(0.0, 1.0);
    let color = color_at(g, offset);
    let mut out = g.clone();
    let index = out.stops.partition_point(|s| s.offset <= offset);
    out.stops.insert(index, GradientStop { offset, color });
    (out, index)
}

/// `g` with stop `index` moved to `offset` (clamped to 0..=1), stops kept
/// sorted, and the stop's new index. `None` for a missing stop.
#[must_use]
pub fn move_stop(g: &Gradient, index: usize, offset: f64) -> Option<(Gradient, usize)> {
    let mut out = g.clone();
    let mut stop = out.stops.get(index).copied()?;
    stop.offset = offset.clamp(0.0, 1.0);
    out.stops.remove(index);
    let at = out.stops.partition_point(|s| s.offset <= stop.offset);
    out.stops.insert(at, stop);
    Some((out, at))
}

/// `g` without stop `index`; `None` when that would leave fewer than two
/// or the stop is missing.
#[must_use]
pub fn remove_stop(g: &Gradient, index: usize) -> Option<Gradient> {
    if g.stops.len() <= 2 || index >= g.stops.len() {
        return None;
    }
    let mut out = g.clone();
    out.stops.remove(index);
    Some(out)
}

/// Solid, dashed, dotted or anything else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DashPreset {
    Solid,
    Dashed,
    Dotted,
    Custom,
}

impl DashPreset {
    /// The three presets, as listed in the inspector.
    pub const PRESETS: [Self; 3] = [Self::Solid, Self::Dashed, Self::Dotted];

    /// Its name in the inspector.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Dashed => "Dashed",
            Self::Dotted => "Dotted",
            Self::Custom => "Custom",
        }
    }
}

/// The dash list (and cap) of a preset for a stroke `width` wide: dashes
/// three widths long with two-width gaps; round dots two widths apart.
#[must_use]
pub fn preset_dash(preset: DashPreset, width: f64) -> (Vec<f64>, Option<LineCap>) {
    let w = width.max(1.0);
    match preset {
        DashPreset::Solid | DashPreset::Custom => (Vec::new(), None),
        DashPreset::Dashed => (vec![3.0 * w, 2.0 * w], None),
        DashPreset::Dotted => (vec![0.0, 2.0 * w], Some(LineCap::Round)),
    }
}

/// Which preset `s`'s dash list is.
#[must_use]
pub fn dash_preset(s: &Stroke) -> DashPreset {
    if s.dash.iter().all(|v| v.abs() <= EPS) {
        return DashPreset::Solid;
    }
    let same = |want: &[f64]| {
        want.len() == s.dash.len() && want.iter().zip(&s.dash).all(|(a, b)| (a - b).abs() <= EPS)
    };
    for preset in [DashPreset::Dashed, DashPreset::Dotted] {
        let (want, cap) = preset_dash(preset, s.width);
        if same(&want) && cap.is_none_or(|c| c == s.cap) {
            return preset;
        }
    }
    DashPreset::Custom
}

/// Dash and gap lengths typed as numbers separated by spaces or commas
/// ("6 3", "4, 2, 1, 2"): at most 16, each finite and ≥ 0.
///
/// # Errors
/// A sentence for the inline error under the field.
pub fn parse_dash(text: &str) -> Result<Vec<f64>, &'static str> {
    let values = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|v| !v.is_empty())
        .map(str::parse::<f64>)
        .collect::<Result<Vec<f64>, _>>()
        .map_err(|_| "Type lengths as numbers, like 6 3")?;
    if values.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err("Lengths cannot be negative");
    }
    if values.len() > MAX_DASH_VALUES {
        return Err("Use at most 16 lengths");
    }
    Ok(values)
}

/// A dash list as typed in the field: "6 3".
#[must_use]
pub fn format_dash(dash: &[f64]) -> String {
    dash.iter()
        .map(|v| format!("{}", (v * 100.0).round() / 100.0))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A pattern tile transform: scaled by `scale` (1 = 100 %), turned by
/// `degrees`, then moved by `offset`.
#[must_use]
pub fn tile_transform(scale: f64, degrees: f64, offset: (f64, f64)) -> Transform {
    let (sin, cos) = degrees.to_radians().sin_cos();
    Transform([
        scale * cos,
        scale * sin,
        -scale * sin,
        scale * cos,
        offset.0,
        offset.1,
    ])
}

/// Scale, rotation in degrees and offset of a tile transform made by
/// [`tile_transform`] (any other is read as its uniform part).
#[must_use]
pub fn tile_parts(t: Transform) -> (f64, f64, (f64, f64)) {
    let [xx, xy, .., dx, dy] = t.0;
    let scale = xx.hypot(xy);
    let degrees = xy.atan2(xx).to_degrees();
    (scale, degrees, (dx, dy))
}

/// A point of a gradient dragged on the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientPoint {
    Start,
    End,
    Center,
    /// On the circle, right of the centre: sets the radius.
    Radius,
    /// Where a radial gradient starts; at the centre until moved.
    Focal,
}

/// The draggable points of `g`, in its own coordinates. A radial
/// gradient's focal point sits on its centre until moved.
#[must_use]
pub fn gradient_points(g: &Gradient) -> Vec<(GradientPoint, Point)> {
    match g.kind {
        GradientKind::Linear { start, end } => {
            vec![(GradientPoint::Start, start), (GradientPoint::End, end)]
        }
        GradientKind::Radial {
            center,
            radius,
            focal,
        } => vec![
            (GradientPoint::Center, center),
            (
                GradientPoint::Radius,
                Point::new(center.x + radius, center.y),
            ),
            (GradientPoint::Focal, focal.unwrap_or(center)),
        ],
    }
}

/// `g` with `point` moved to `to` (its own coordinates). Moving the centre
/// takes the focal point along; the radius is the distance from the
/// centre. `None` when `g` has no such point or `to` is not finite.
#[must_use]
pub fn move_point(g: &Gradient, point: GradientPoint, to: Point) -> Option<Gradient> {
    if !to.is_finite() {
        return None;
    }
    let mut out = g.clone();
    match (&mut out.kind, point) {
        (GradientKind::Linear { start, .. }, GradientPoint::Start) => *start = to,
        (GradientKind::Linear { end, .. }, GradientPoint::End) => *end = to,
        (GradientKind::Radial { center, focal, .. }, GradientPoint::Center) => {
            let delta = to - *center;
            *center = to;
            if let Some(f) = focal {
                *f = f.offset(delta);
            }
        }
        (GradientKind::Radial { center, radius, .. }, GradientPoint::Radius) => {
            *radius = (to - *center).length();
        }
        (GradientKind::Radial { focal, .. }, GradientPoint::Focal) => *focal = Some(to),
        _ => return None,
    }
    Some(out)
}
