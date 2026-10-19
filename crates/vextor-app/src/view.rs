//! Canvas view math: zoom, pan and fit, as pure functions.
//!
//! Screen positions are in logical points relative to the canvas's
//! top-left corner; document positions are in document px.

use vextor_core::{Point, Rect, Vec2, View};

/// Smallest zoom (10 %).
pub const MIN_ZOOM: f64 = 0.1;
/// Largest zoom (6 400 %), enough to inspect a 16 px icon.
pub const MAX_ZOOM: f64 = 64.0;
/// Space kept around an artboard by "fit", in points.
pub const FIT_MARGIN: f64 = 40.0;

/// Where a document point appears on the canvas.
#[must_use]
pub fn doc_to_screen(view: View, p: Point) -> Point {
    Point::new(p.x * view.zoom + view.pan.x, p.y * view.zoom + view.pan.y)
}

/// The document point under a canvas position.
#[must_use]
pub fn screen_to_doc(view: View, s: Point) -> Point {
    Point::new(
        (s.x - view.pan.x) / view.zoom,
        (s.y - view.pan.y) / view.zoom,
    )
}

/// The view moved by `delta` points.
#[must_use]
pub fn pan_by(view: View, delta: Vec2) -> View {
    View {
        pan: view.pan + delta,
        ..view
    }
}

/// The view zoomed by `factor`, keeping the document point under `cursor`
/// where it is. The zoom is clamped to [`MIN_ZOOM`]..=[`MAX_ZOOM`].
#[must_use]
pub fn zoom_at(view: View, cursor: Point, factor: f64) -> View {
    if !factor.is_finite() || factor <= 0.0 {
        return view;
    }
    let zoom = (view.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    let k = zoom / view.zoom;
    View {
        zoom,
        pan: Vec2::new(
            cursor.x - (cursor.x - view.pan.x) * k,
            cursor.y - (cursor.y - view.pan.y) * k,
        ),
    }
}

/// The view at 100 %, keeping the document point at the canvas centre.
#[must_use]
pub fn actual_size(view: View, canvas: Vec2) -> View {
    zoom_at(view, centre(canvas), 1.0 / view.zoom)
}

/// The view that shows `target` whole and centred on a canvas of size
/// `canvas`, with [`FIT_MARGIN`] around it.
#[must_use]
pub fn fit(target: Rect, canvas: Vec2) -> View {
    let room = Vec2::new(
        (canvas.x - 2.0 * FIT_MARGIN).max(1.0),
        (canvas.y - 2.0 * FIT_MARGIN).max(1.0),
    );
    let zoom = if target.width > 0.0 && target.height > 0.0 {
        (room.x / target.width).min(room.y / target.height)
    } else {
        1.0
    }
    .clamp(MIN_ZOOM, MAX_ZOOM);
    let c = centre(canvas);
    View {
        zoom,
        pan: Vec2::new(
            c.x - (target.x + target.width / 2.0) * zoom,
            c.y - (target.y + target.height / 2.0) * zoom,
        ),
    }
}

/// The smallest rectangle holding all `rects`, or `None` when there are none.
#[must_use]
pub fn bounds(rects: impl IntoIterator<Item = Rect>) -> Option<Rect> {
    rects.into_iter().reduce(|a, b| {
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        let right = (a.x + a.width).max(b.x + b.width);
        let bottom = (a.y + a.height).max(b.y + b.height);
        Rect::new(x, y, right - x, bottom - y)
    })
}

/// Where to draw an image rendered for `rendered` so that it lines up with
/// `now`: its top-left corner and the scale to apply to its size. Used
/// while a pan or zoom is in progress, before the canvas renders again.
#[must_use]
pub fn reproject(rendered: View, now: View) -> (Point, f64) {
    let k = now.zoom / rendered.zoom;
    (
        Point::new(
            now.pan.x - rendered.pan.x * k,
            now.pan.y - rendered.pan.y * k,
        ),
        k,
    )
}

fn centre(canvas: Vec2) -> Point {
    Point::new(canvas.x / 2.0, canvas.y / 2.0)
}

#[cfg(test)]
mod tests {
    use vextor_core::EPS;

    use super::*;

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
    }

    fn view(zoom: f64, x: f64, y: f64) -> View {
        View {
            zoom,
            pan: Vec2::new(x, y),
        }
    }

    #[test]
    fn screen_and_doc_are_inverse() {
        let v = view(2.5, 30.0, -12.0);
        let p = Point::new(7.0, 9.0);
        assert!(close(screen_to_doc(v, doc_to_screen(v, p)), p));
    }

    #[test]
    fn zoom_at_keeps_the_point_under_the_cursor() {
        let v = view(1.5, 40.0, 20.0);
        let cursor = Point::new(300.0, 200.0);
        let before = screen_to_doc(v, cursor);
        let after = screen_to_doc(zoom_at(v, cursor, 2.0), cursor);
        assert!(close(before, after));
    }

    #[test]
    fn zoom_is_clamped() {
        let v = view(1.0, 0.0, 0.0);
        let c = Point::new(0.0, 0.0);
        assert!((zoom_at(v, c, 1e6).zoom - MAX_ZOOM).abs() < EPS);
        assert!((zoom_at(v, c, 1e-6).zoom - MIN_ZOOM).abs() < EPS);
    }

    #[test]
    fn invalid_zoom_factor_is_ignored() {
        let v = view(2.0, 1.0, 1.0);
        let c = Point::new(5.0, 5.0);
        assert_eq!(zoom_at(v, c, f64::NAN), v);
        assert_eq!(zoom_at(v, c, 0.0), v);
    }

    #[test]
    fn actual_size_is_100_percent_about_the_centre() {
        let v = view(3.0, 10.0, 10.0);
        let canvas = Vec2::new(800.0, 600.0);
        let centre = Point::new(400.0, 300.0);
        let a = actual_size(v, canvas);
        assert!((a.zoom - 1.0).abs() < EPS);
        assert!(close(screen_to_doc(v, centre), screen_to_doc(a, centre)));
    }

    #[test]
    fn fit_centres_the_artboard_inside_the_margin() {
        let card = Rect::new(0.0, 0.0, 320.0, 200.0);
        let canvas = Vec2::new(800.0, 600.0);
        let v = fit(card, canvas);
        // Width limits: (800 - 80) / 320 = 2.25.
        assert!((v.zoom - 2.25).abs() < EPS);
        let tl = doc_to_screen(v, Point::new(0.0, 0.0));
        let br = doc_to_screen(v, Point::new(320.0, 200.0));
        assert!(close(tl, Point::new(40.0, 75.0)));
        assert!(close(br, Point::new(760.0, 525.0)));
    }

    #[test]
    fn fit_of_a_tiny_icon_stops_at_max_zoom() {
        let icon = Rect::new(0.0, 0.0, 16.0, 16.0);
        assert!((fit(icon, Vec2::new(4000.0, 4000.0)).zoom - MAX_ZOOM).abs() < EPS);
    }

    #[test]
    fn bounds_unites_rects() {
        let b = bounds([
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Rect::new(20.0, -5.0, 5.0, 5.0),
        ]);
        assert_eq!(b, Some(Rect::new(0.0, -5.0, 25.0, 15.0)));
        assert_eq!(bounds([]), None);
    }

    #[test]
    fn reproject_of_the_same_view_is_identity() {
        let v = view(2.0, 13.0, -4.0);
        let (origin, k) = reproject(v, v);
        assert!(close(origin, Point::new(0.0, 0.0)));
        assert!((k - 1.0).abs() < EPS);
    }

    #[test]
    fn reproject_lines_up_document_points() {
        let rendered = view(2.0, 10.0, 20.0);
        let now = zoom_at(
            pan_by(rendered, Vec2::new(5.0, 5.0)),
            Point::new(100.0, 50.0),
            1.5,
        );
        let (origin, k) = reproject(rendered, now);
        let p = Point::new(33.0, 44.0);
        // Where p sits in the old image, mapped through the reprojection…
        let in_image = doc_to_screen(rendered, p);
        let shown = Point::new(origin.x + in_image.x * k, origin.y + in_image.y * k);
        // …is where the new view puts it.
        assert!(close(shown, doc_to_screen(now, p)));
    }
}
