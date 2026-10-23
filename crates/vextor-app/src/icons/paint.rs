//! The paint and inspector icons (spec 09 and its inspector polish), on
//! the same 16 × 16 grid: the pipette, the rect corners, flip, and the
//! stroke caps and joins (filled bars and a guide line, like the Align
//! icons).

use std::f32::consts::{FRAC_PI_2, TAU};

use eframe::egui::{self, Pos2, Stroke};

use super::{Icon, P, Pen};

impl Pen<'_> {
    pub(super) fn paint_icon(&self, icon: Icon) {
        match icon {
            Icon::CornerTopLeft => self.corner((2.0, 2.0), (1.0, 1.0), 12.0),
            Icon::CornerTopRight => self.corner((14.0, 2.0), (-1.0, 1.0), 12.0),
            Icon::CornerBottomRight => self.corner((14.0, 14.0), (-1.0, -1.0), 12.0),
            Icon::CornerBottomLeft => self.corner((2.0, 14.0), (1.0, -1.0), 12.0),
            Icon::CornerAll => {
                self.corner((1.5, 1.5), (1.0, 1.0), 5.0);
                self.corner((14.5, 1.5), (-1.0, 1.0), 5.0);
                self.corner((14.5, 14.5), (-1.0, -1.0), 5.0);
                self.corner((1.5, 14.5), (1.0, -1.0), 5.0);
            }
            Icon::CapButt | Icon::CapRound | Icon::CapSquare => self.cap(icon),
            Icon::JoinMiter | Icon::JoinRound | Icon::JoinBevel => self.join(icon),
            Icon::FlipHorizontal => self.flip(false),
            Icon::FlipVertical => self.flip(true),
            Icon::Eyedropper => {
                self.line((2.0, 14.0), (3.5, 12.5));
                self.line((3.5, 12.5), (10.0, 6.0));
                self.line((8.5, 4.5), (11.5, 7.5));
                self.arc((12.0, 4.0), 2.6, 0.0, TAU);
            }
            _ => {}
        }
    }

    /// A filled stroke along y = 8 from the left, a guide line across it
    /// at x = 9 where its path ends (standing out above and below), and
    /// `icon`'s cap past the guide.
    fn cap(&self, icon: Icon) {
        let (top, bottom, end) = (5.0, 11.0, 9.0);
        self.fill((1.5, top), (end, bottom));
        match icon {
            Icon::CapRound => {
                self.painter
                    .circle_filled(self.at((end, 8.0)), 3.0 * self.k, self.color);
            }
            Icon::CapSquare => self.fill((end, top), (end + 3.0, bottom)),
            _ => {}
        }
        self.line((end + 1.5, 2.0), (end + 1.5, 14.0));
    }

    /// A filled stroke turning a right-angle corner (along the top, then
    /// down the right), its outer corner per `icon`'s join.
    fn join(&self, icon: Icon) {
        // Arms 5 wide: across y 3..8 from the left, down x 9..14.
        self.fill((1.5, 3.0), (9.0, 8.0));
        self.fill((9.0, 8.0), (14.0, 14.5));
        let corner: Vec<(f32, f32)> = match icon {
            Icon::JoinMiter => vec![(9.0, 3.0), (14.0, 3.0), (14.0, 8.0), (9.0, 8.0)],
            Icon::JoinRound => {
                let mut pts = vec![(9.0, 8.0)];
                for i in 0..=8_u8 {
                    let a = -FRAC_PI_2 + FRAC_PI_2 * f32::from(i) / 8.0;
                    pts.push((9.0 + 5.0 * a.cos(), 8.0 + 5.0 * a.sin()));
                }
                pts
            }
            _ => vec![(9.0, 3.0), (14.0, 8.0), (9.0, 8.0)],
        };
        let points: Vec<Pos2> = corner.into_iter().map(|p| self.at(p)).collect();
        self.painter.add(egui::Shape::convex_polygon(
            points,
            self.color,
            Stroke::NONE,
        ));
    }

    /// Two outlined right triangles mirrored across a narrow gap (no
    /// axis, no fill, like the other line icons); `vertical` turns the
    /// drawing on its side, so they mirror top to bottom.
    fn flip(&self, vertical: bool) {
        let t = |(x, y): P| if vertical { (y, x) } else { (x, y) };
        for half in [
            [(7.0, 2.0), (7.0, 14.0), (1.5, 14.0)],
            [(9.0, 2.0), (9.0, 14.0), (14.5, 14.0)],
        ] {
            let points: Vec<Pos2> = half.map(|p| self.at(t(p))).to_vec();
            self.painter
                .add(egui::Shape::closed_line(points, self.stroke));
        }
    }

    /// A corner bracket at `at` whose arms run `len` along `dir` (±1 per
    /// axis, into the square), with a dot inside the corner.
    fn corner(&self, at: P, dir: P, len: f32) {
        let (x, y) = at;
        self.line((x, y + dir.1 * len), at);
        self.line(at, (x + dir.0 * len, y));
        let inset = (len * 0.3).max(2.5);
        let dot = self.at((x + dir.0 * inset, y + dir.1 * inset));
        self.painter
            .circle_filled(dot, (len * 0.12).max(1.1) * self.k, self.color);
    }
}
