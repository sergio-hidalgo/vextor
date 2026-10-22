//! The arrange icons (spec 08c), on the same 16 × 16 grid: our own
//! drawings of two stacked squares, the moving one filled, and an arrow
//! up or down; to front / to back end the arrow on a bar.

use super::{Icon, P, Pen};

/// The square behind and the square in front.
const BEHIND: (P, P) = ((1.0, 3.5), (7.5, 10.0));
const IN_FRONT: (P, P) = ((4.0, 7.0), (10.5, 13.5));
/// The arrow's x and the bar's half width.
const ARROW_X: f32 = 13.5;
const BAR: f32 = 2.5;

impl Pen<'_> {
    /// Draws one of the arrange icons; other icons draw nothing here.
    pub(super) fn arrange_icon(&self, icon: Icon) {
        let (up, to_end) = match icon {
            Icon::ArrangeFront => (true, true),
            Icon::ArrangeForward => (true, false),
            Icon::ArrangeBackward => (false, false),
            Icon::ArrangeBack => (false, true),
            _ => return,
        };
        if up {
            self.outline(BEHIND.0, BEHIND.1);
            self.fill(IN_FRONT.0, IN_FRONT.1);
        } else {
            // The filled square shows only where the front one leaves it
            // uncovered (a 1-unit gap around the front one).
            let ((x0, y0), (x1, _)) = BEHIND;
            let ((fx, fy), _) = IN_FRONT;
            self.fill((x0, y0), (x1, fy - 1.0));
            self.fill((x0, fy - 1.0), (fx - 1.0, BEHIND.1.1));
            self.outline(IN_FRONT.0, IN_FRONT.1);
        }
        let (tail, tip, bar) = if up {
            (13.0, 3.5, 1.5)
        } else {
            (3.0, 12.5, 14.5)
        };
        self.arrow((ARROW_X, tail), (ARROW_X, tip));
        if to_end {
            self.line((ARROW_X - BAR, bar), (ARROW_X + BAR, bar));
        }
    }
}
