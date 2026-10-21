//! The canvas grid (spec 08), drawn adaptively: minor lines fade out as
//! they crowd together, and major lines skip to every 2nd, 4th… line
//! when even they would be too dense.

use eframe::egui::{self, Painter, Stroke};
use vextor_core::{GridSettings, View};

use crate::theme;

/// Minor lines closer than this on screen (points) are not drawn.
const MIN_GAP: f64 = 4.0;
/// Minor lines this far apart (points) or more are drawn at full strength.
const FULL_GAP: f64 = 10.0;
/// Major lines are at least this far apart on screen (points).
const MAJOR_GAP: f64 = 8.0;
/// Opacity of a major line, and of a minor one at full strength.
const MAJOR_ALPHA: f32 = 0.45;
const MINOR_ALPHA: f32 = 0.18;

/// One grid line on screen: vertical (an x) or horizontal (a y), in
/// points from the canvas's top-left corner, and how strong (0..=1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Line {
    pub vertical: bool,
    pub at: f64,
    pub alpha: f32,
}

/// The lines of `grid` visible on a canvas of `size` points at `v`.
pub(super) fn lines(grid: &GridSettings, v: View, size: (f64, f64)) -> Vec<Line> {
    let Some(minor) = vextor_geom::minor_spacing(grid) else {
        return Vec::new();
    };
    let subdivisions = grid.subdivisions.max(1);
    let minor_gap = minor * v.zoom;
    // Major step: every `skip`th major line, so they stay MAJOR_GAP apart.
    let mut skip: u32 = 1;
    while grid.size * v.zoom * f64::from(skip) < MAJOR_GAP && skip < 1 << 20 {
        skip *= 2;
    }
    let minor_alpha = if skip > 1 {
        0.0
    } else {
        #[allow(clippy::cast_possible_truncation)] // a 0..=1 ratio
        let t = ((minor_gap - MIN_GAP) / (FULL_GAP - MIN_GAP)).clamp(0.0, 1.0) as f32;
        t * MINOR_ALPHA
    };
    let (step, every) = if minor_alpha > 0.0 {
        (minor, subdivisions)
    } else {
        (grid.size * f64::from(skip), 1)
    };
    let mut out = Vec::new();
    for (vertical, pan, extent) in [(true, v.pan.x, size.0), (false, v.pan.y, size.1)] {
        // Document coordinates at the canvas's two edges.
        let first = ((-pan) / v.zoom / step).floor();
        let last = ((extent - pan) / v.zoom / step).ceil();
        if !(first.is_finite() && last.is_finite()) {
            continue;
        }
        #[allow(clippy::cast_possible_truncation)] // bounded by the canvas size / gap
        let (first, last) = (first as i64, last as i64);
        for k in first..=last {
            #[allow(clippy::cast_precision_loss)] // line index
            let doc = k as f64 * step;
            let major = every == 1 || k.rem_euclid(i64::from(every)) == 0;
            out.push(Line {
                vertical,
                at: doc * v.zoom + pan,
                alpha: if major { MAJOR_ALPHA } else { minor_alpha },
            });
        }
    }
    out
}

/// Draws the grid over the canvas `rect`.
pub(super) fn draw(painter: &Painter, rect: egui::Rect, grid: &GridSettings, v: View) {
    let size = (f64::from(rect.width()), f64::from(rect.height()));
    for line in lines(grid, v, size) {
        let stroke = Stroke::new(1.0, theme::GREY_500.gamma_multiply(line.alpha));
        #[allow(clippy::cast_possible_truncation)] // screen coordinates
        let at = line.at as f32;
        if line.vertical {
            let x = rect.left() + at;
            painter.vline(x, rect.y_range(), stroke);
        } else {
            let y = rect.top() + at;
            painter.hline(rect.x_range(), y, stroke);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vextor_core::Vec2;

    fn view(zoom: f64) -> View {
        View {
            zoom,
            pan: Vec2::new(0.0, 0.0),
        }
    }

    fn minors(ls: &[Line]) -> usize {
        ls.iter().filter(|l| l.alpha < MAJOR_ALPHA).count()
    }

    #[test]
    fn minor_lines_show_when_zoomed_in() {
        // 8 px / 4 at 800 %: minor lines 16 points apart.
        let ls = lines(&GridSettings::default(), view(8.0), (160.0, 160.0));
        assert!(minors(&ls) > 0);
        assert!(ls.iter().all(|l| l.alpha > 0.0));
        // Every 4th line is a major one: x = 0, 64, 128.
        let majors: Vec<_> = ls
            .iter()
            .filter(|l| l.vertical && l.alpha >= MAJOR_ALPHA)
            .map(|l| l.at)
            .collect();
        assert_eq!(majors, vec![0.0, 64.0, 128.0]);
    }

    #[test]
    fn minor_lines_fade_then_go_when_zoomed_out() {
        let at = |zoom| lines(&GridSettings::default(), view(zoom), (400.0, 400.0));
        // 300 %: minor lines 6 points apart, fading.
        let fading = at(3.0);
        assert!(
            fading
                .iter()
                .any(|l| l.alpha > 0.0 && l.alpha < MINOR_ALPHA)
        );
        // 100 %: 2 points apart, gone; only the majors (8 apart) remain.
        let gone = at(1.0);
        assert_eq!(minors(&gone), 0);
        assert!(gone.iter().all(|l| (l.at / 8.0).fract().abs() < 1e-9));
    }

    #[test]
    fn major_lines_thin_out_when_far_away() {
        // 10 %: majors would be 0.8 points apart; every 16th is drawn.
        let ls = lines(&GridSettings::default(), view(0.1), (400.0, 400.0));
        let xs: Vec<_> = ls.iter().filter(|l| l.vertical).map(|l| l.at).collect();
        assert!(xs.windows(2).all(|w| w[1] - w[0] >= MAJOR_GAP - 1e-9));
        assert!(xs.len() < 60);
    }

    #[test]
    fn an_invalid_grid_draws_nothing() {
        let grid = GridSettings {
            size: 0.0,
            ..GridSettings::default()
        };
        assert_eq!(lines(&grid, view(1.0), (100.0, 100.0)).len(), 0);
    }
}
