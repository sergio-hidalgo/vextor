//! The Layers panel icons (spec 10), on the same 16 × 16 grid: our own
//! drawings of an eye (an almond with an iris; struck through when hidden), a padlock
//! (shut, or open), stacked sheets for a layer, a framed hill for an
//! image, and two offset squares for duplicate.

use std::f32::consts::PI;

use super::{Icon, Pen};

impl Pen<'_> {
    /// Draws one of the Layers panel icons; other icons draw nothing here.
    pub(super) fn layers_icon(&self, icon: Icon) {
        match icon {
            Icon::Eye | Icon::EyeOff => {
                // An almond: two lid arcs meeting in points at (1, 8) and
                // (15, 8), 4 units high each; an iris ring and a pupil.
                let (half, rise) = (7.0_f32, 4.0_f32);
                let r = (half * half + rise * rise) / (2.0 * rise);
                let spread = (half / r).asin();
                let up = -PI / 2.0;
                self.arc((8.0, 8.0 - rise + r), r, up - spread, up + spread);
                self.arc((8.0, 8.0 + rise - r), r, -up - spread, -up + spread);
                self.painter
                    .circle_stroke(self.at((8.0, 8.0)), 2.8 * self.k, self.stroke);
                self.painter
                    .circle_filled(self.at((8.0, 8.0)), 1.3 * self.k, self.color);
                if icon == Icon::EyeOff {
                    self.line((2.5, 14.0), (13.5, 2.0));
                }
            }
            Icon::Lock | Icon::Unlock => {
                self.fill((3.0, 7.5), (13.0, 14.5));
                let x1 = if icon == Icon::Lock { 11.0 } else { 15.0 };
                let x0 = x1 - 6.0;
                self.line((x0, 7.5), (x0, 5.0));
                self.line((x1, if icon == Icon::Lock { 7.5 } else { 5.0 }), (x1, 5.0));
                self.arc((x0 + 3.0, 5.0), 3.0, PI, 2.0 * PI);
            }
            Icon::Layer => {
                // Three flat sheets, seen from the side, stacked.
                for y in [4.0, 8.0, 12.0] {
                    self.line((2.0, y), (8.0, y - 2.5));
                    self.line((8.0, y - 2.5), (14.0, y));
                    self.line((14.0, y), (8.0, y + 2.5));
                    self.line((8.0, y + 2.5), (2.0, y));
                }
            }
            Icon::Image => {
                self.outline((1.5, 2.5), (14.5, 13.5));
                self.line((3.0, 12.0), (7.0, 7.0));
                self.line((7.0, 7.0), (10.0, 10.0));
                self.line((10.0, 10.0), (11.5, 8.5));
                self.line((11.5, 8.5), (13.5, 11.0));
                self.painter
                    .circle_filled(self.at((11.0, 5.5)), 1.3 * self.k, self.color);
            }
            Icon::Duplicate => {
                self.outline((1.5, 1.5), (10.0, 10.0));
                self.fill((6.0, 6.0), (14.5, 14.5));
            }
            _ => {}
        }
    }
}
