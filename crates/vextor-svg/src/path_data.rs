//! Shortest valid path data (`svg-output`): per command the shorter of the
//! absolute and relative form, repeated letters left out, `H`/`V` for
//! axis-aligned lines, `S`/`T` for mirrored control points, a closing line
//! left to `z`, and no separator where the next number's sign or dot ends
//! the previous one.

use kurbo::{BezPath, PathEl};

use crate::number::Num;

/// A point in units of the last decimal ([`Num::units`]).
type Pt = (i64, i64);

/// `path` as `d` attribute data with `num`'s decimals; empty when the path
/// has no segments.
pub(crate) fn path_data(path: &BezPath, num: Num) -> String {
    let mut w = PathWriter {
        num,
        out: String::new(),
        last_dot: false,
        letter: None,
        cur: (0, 0),
        start: (0, 0),
        cubic: None,
        quad: None,
        drew: false,
    };
    let pt = |p: kurbo::Point| (num.units(p.x), num.units(p.y));
    let els = path.elements();
    for (i, el) in els.iter().enumerate() {
        match *el {
            PathEl::MoveTo(p) => w.move_to(pt(p)),
            PathEl::LineTo(p) => {
                let closes = matches!(els.get(i + 1), Some(PathEl::ClosePath));
                w.line_to(pt(p), closes);
            }
            PathEl::QuadTo(c, p) => w.quad_to(pt(c), pt(p)),
            PathEl::CurveTo(a, b, p) => w.curve_to(pt(a), pt(b), pt(p)),
            PathEl::ClosePath => w.close(),
        }
    }
    // A path of moves alone draws nothing.
    if w.drew { w.out } else { String::new() }
}

struct PathWriter {
    num: Num,
    out: String,
    /// Whether the last number written holds a `.` (so a following `.5`
    /// needs no space).
    last_dot: bool,
    /// The letter a number list continues without repeating it (after `M`
    /// that is `L`).
    letter: Option<char>,
    cur: Pt,
    start: Pt,
    /// The previous segment's last control point, when it was a cubic.
    cubic: Option<Pt>,
    /// The previous segment's control point, when it was a quadratic.
    quad: Option<Pt>,
    /// Whether any segment was written.
    drew: bool,
}

impl PathWriter {
    fn move_to(&mut self, p: Pt) {
        let letter = self.emit('M', &[p.0, p.1], &[p.0 - self.cur.0, p.1 - self.cur.1]);
        // Pairs after a move are lines.
        self.letter = Some(if letter == 'M' { 'L' } else { 'l' });
        self.cur = p;
        self.start = p;
        self.cubic = None;
        self.quad = None;
    }

    fn line_to(&mut self, p: Pt, closes: bool) {
        self.cubic = None;
        self.quad = None;
        self.drew = true;
        if closes && p == self.start {
            // `z` draws this line.
            self.cur = p;
            return;
        }
        let (dx, dy) = (p.0 - self.cur.0, p.1 - self.cur.1);
        if dy == 0 && dx != 0 {
            self.emit('H', &[p.0], &[dx]);
        } else if dx == 0 && dy != 0 {
            self.emit('V', &[p.1], &[dy]);
        } else {
            self.emit('L', &[p.0, p.1], &[dx, dy]);
        }
        self.cur = p;
    }

    fn quad_to(&mut self, c: Pt, p: Pt) {
        let implied = self.quad.map_or(self.cur, |q| mirror(q, self.cur));
        let (cx, cy) = (self.cur.0, self.cur.1);
        if c == implied {
            self.emit('T', &[p.0, p.1], &[p.0 - cx, p.1 - cy]);
        } else {
            self.emit(
                'Q',
                &[c.0, c.1, p.0, p.1],
                &[c.0 - cx, c.1 - cy, p.0 - cx, p.1 - cy],
            );
        }
        self.quad = Some(c);
        self.cubic = None;
        self.cur = p;
        self.drew = true;
    }

    fn curve_to(&mut self, a: Pt, b: Pt, p: Pt) {
        let implied = self.cubic.map_or(self.cur, |c| mirror(c, self.cur));
        let (cx, cy) = (self.cur.0, self.cur.1);
        if a == implied {
            self.emit(
                'S',
                &[b.0, b.1, p.0, p.1],
                &[b.0 - cx, b.1 - cy, p.0 - cx, p.1 - cy],
            );
        } else {
            self.emit(
                'C',
                &[a.0, a.1, b.0, b.1, p.0, p.1],
                &[a.0 - cx, a.1 - cy, b.0 - cx, b.1 - cy, p.0 - cx, p.1 - cy],
            );
        }
        self.cubic = Some(b);
        self.quad = None;
        self.cur = p;
        self.drew = true;
    }

    fn close(&mut self) {
        if self.out.ends_with('z') {
            return;
        }
        self.out.push('z');
        self.last_dot = false;
        self.letter = None;
        self.cur = self.start;
        self.cubic = None;
        self.quad = None;
    }

    /// Writes command `upper` (absolute numbers `abs`) or its lowercase
    /// relative form (`rel`) when that is shorter; returns the letter used.
    fn emit(&mut self, upper: char, abs: &[i64], rel: &[i64]) -> char {
        let lower = upper.to_ascii_lowercase();
        let a = self.render(upper, abs);
        let r = self.render(lower, rel);
        let ((text, dot), letter) = if r.0.len() < a.0.len() {
            (r, lower)
        } else {
            (a, upper)
        };
        self.out.push_str(&text);
        self.last_dot = dot;
        self.letter = Some(letter);
        letter
    }

    /// What command `letter` with `nums` adds to the output, and whether its
    /// last number holds a dot.
    fn render(&self, letter: char, nums: &[i64]) -> (String, bool) {
        let mut s = String::new();
        let mut dot = self.last_dot;
        // Right after a letter (or at the start) no separator is needed.
        let mut after_number = !self.out.is_empty() && !self.out.ends_with(char::is_alphabetic);
        if self.letter != Some(letter) {
            s.push(letter);
            after_number = false;
        }
        for &n in nums {
            let v = self.num.fmt_units(n);
            if after_number && !(v.starts_with('-') || (v.starts_with('.') && dot)) {
                s.push(' ');
            }
            dot = v.contains('.');
            s.push_str(&v);
            after_number = true;
        }
        (s, dot)
    }
}

/// `c` mirrored about `about`.
fn mirror(c: Pt, about: Pt) -> Pt {
    (2 * about.0 - c.0, 2 * about.1 - c.1)
}

#[cfg(test)]
mod tests {
    use kurbo::BezPath;

    use super::*;

    fn d(path: &BezPath) -> String {
        path_data(path, Num::new(2))
    }

    #[test]
    fn a_closed_square_uses_h_v_and_z() {
        let mut p = BezPath::new();
        p.move_to((10.0, 10.0));
        p.line_to((20.0, 10.0));
        p.line_to((20.0, 20.0));
        p.line_to((10.0, 20.0));
        p.line_to((10.0, 10.0));
        p.close_path();
        assert_eq!(d(&p), "M10 10H20V20H10z");
    }

    #[test]
    fn repeated_commands_and_signs_drop_letters_and_spaces() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((1.0, 2.0));
        p.line_to((-1.0, 3.0));
        p.line_to((-1.5, 3.25));
        assert_eq!(d(&p), "M0 0 1 2-1 3l-.5.25");
    }

    #[test]
    fn mirrored_controls_become_s_and_t() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.curve_to((0.0, 10.0), (10.0, 10.0), (10.0, 0.0));
        p.curve_to((10.0, -10.0), (20.0, -10.0), (20.0, 0.0));
        assert_eq!(d(&p), "M0 0C0 10 10 10 10 0S20-10 20 0");
        let mut q = BezPath::new();
        q.move_to((0.0, 0.0));
        q.quad_to((5.0, 10.0), (10.0, 0.0));
        q.quad_to((15.0, -10.0), (20.0, 0.0));
        assert_eq!(d(&q), "M0 0Q5 10 10 0T20 0");
    }

    #[test]
    fn far_from_the_origin_relative_numbers_are_shorter() {
        let mut p = BezPath::new();
        for x in [0.0, 100.0] {
            p.move_to((x, 0.0));
            p.line_to((x + 4.0, 0.0));
            p.line_to((x + 4.0, 4.0));
            p.close_path();
        }
        assert_eq!(d(&p), "M0 0H4V4zM100 0h4V4z");
    }

    #[test]
    fn relative_numbers_never_drift_from_rounding() {
        // Each step is .333; rounded one by one they would end at 1000.99.
        let mut p = BezPath::new();
        p.move_to((1000.0, 0.0));
        for x in [1000.333, 1000.666, 1000.999] {
            p.line_to((x, 0.0));
        }
        assert_eq!(d(&p), "M1000 0h.33.34.33");
    }

    #[test]
    fn moves_alone_draw_nothing() {
        let mut p = BezPath::new();
        p.move_to((1.0, 1.0));
        assert_eq!(d(&p), "");
        assert_eq!(d(&BezPath::new()), "");
    }
}
