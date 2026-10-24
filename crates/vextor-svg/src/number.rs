//! Numbers as SVG writes them (`svg-output`): at most `decimals` places,
//! trailing zeros and the leading `0` of `0.x` dropped, `-0` as `0`.

/// Rounds and writes numbers to a fixed number of decimals (0..=6).
///
/// Values are first rounded to whole *units* of the last decimal, so
/// differences of rounded values (relative path data) are exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Num {
    decimals: u32,
}

/// Most decimals any number is written with.
const MAX_DECIMALS: u32 = 8;

impl Num {
    pub(crate) const fn new(decimals: u8) -> Self {
        let decimals = decimals as u32;
        Self {
            decimals: if decimals > MAX_DECIMALS {
                MAX_DECIMALS
            } else {
                decimals
            },
        }
    }

    /// How many decimals it writes at most.
    pub(crate) fn decimals(self) -> u32 {
        self.decimals
    }

    /// The same, with at least `decimals` places: for values in 0..1
    /// (opacity, offsets, bounding-box units) that would lose their meaning
    /// at 0 or 1 decimals.
    pub(crate) fn at_least(self, decimals: u32) -> Self {
        Self {
            decimals: self.decimals.max(decimals).min(MAX_DECIMALS),
        }
    }

    fn scale(self) -> f64 {
        10f64.powi(i32::try_from(self.decimals).unwrap_or(0))
    }

    /// `v` in units of the last decimal. Not-finite values count as 0 (a
    /// validated document has none).
    #[allow(clippy::cast_possible_truncation)] // clamped to the i64 range first
    pub(crate) fn units(self, v: f64) -> i64 {
        if !v.is_finite() {
            return 0;
        }
        let max = 9.0e15;
        (v * self.scale()).round().clamp(-max, max) as i64
    }

    /// `v` rounded and written.
    pub(crate) fn fmt(self, v: f64) -> String {
        self.fmt_units(self.units(v))
    }

    /// A count of units written as a decimal number.
    pub(crate) fn fmt_units(self, n: i64) -> String {
        if n == 0 {
            return "0".to_owned();
        }
        let per = 10u64.pow(self.decimals);
        let (int, frac) = (n.unsigned_abs() / per, n.unsigned_abs() % per);
        let mut s = String::with_capacity(12);
        if n < 0 {
            s.push('-');
        }
        if int != 0 || frac == 0 {
            s.push_str(&int.to_string());
        }
        if frac != 0 {
            let digits = format!("{frac:0width$}", width = self.decimals as usize);
            s.push('.');
            s.push_str(digits.trim_end_matches('0'));
        }
        s
    }

    /// True when `a` and `b` round to the same written value.
    pub(crate) fn same(self, a: f64, b: f64) -> bool {
        self.units(a) == self.units(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_short() {
        let n = Num::new(2);
        assert_eq!(n.fmt(1.0), "1");
        assert_eq!(n.fmt(0.5), ".5");
        assert_eq!(n.fmt(-0.25), "-.25");
        assert_eq!(n.fmt(-0.001), "0");
        assert_eq!(n.fmt(12.345_6), "12.35");
        assert_eq!(n.fmt(100.0), "100");
        assert_eq!(n.fmt(-3.10), "-3.1");
        assert_eq!(n.fmt(f64::NAN), "0");
    }

    #[test]
    fn precision_sets_the_decimals() {
        assert_eq!(Num::new(0).fmt(12.6), "13");
        assert_eq!(Num::new(0).fmt(0.4), "0");
        assert_eq!(Num::new(4).fmt(1.234_56), "1.2346");
        assert_eq!(Num::new(0).at_least(3).fmt(0.25), ".25");
        assert!(Num::new(1).same(1.04, 0.96));
    }
}
