//! The edges of a pixel mask as closed loops along the pixel grid: each
//! loop is a run of unit steps between pixel corners, the covered pixels
//! on its right (clockwise on screen around a shape, anticlockwise around
//! a hole).

/// A step to the next pixel corner: east, south, west, north (y down).
pub(super) type Dir = u8;
const EAST: Dir = 0;
const SOUTH: Dir = 1;
const WEST: Dir = 2;
const NORTH: Dir = 3;

/// The turn to the right of `d` (y down: east → south).
pub(super) fn right(d: Dir) -> Dir {
    (d + 1) % 4
}

/// `d` as a unit step.
pub(super) fn step(d: Dir) -> (i64, i64) {
    match d {
        EAST => (1, 0),
        SOUTH => (0, 1),
        WEST => (-1, 0),
        _ => (0, -1),
    }
}

/// An edge along the pixel grid: where it starts (a pixel corner), its
/// steps, and whether it ends where it started (a loop) or is open (a line
/// between two colour areas, ending where three or more meet).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Loop {
    pub start: (i64, i64),
    pub steps: Vec<Dir>,
    pub closed: bool,
}

impl Loop {
    /// Twice the signed area, positive around a shape, negative around a
    /// hole.
    pub fn area2(&self) -> i64 {
        let (mut x, mut y) = self.start;
        let mut sum = 0;
        for &d in &self.steps {
            let (dx, dy) = step(d);
            let (nx, ny) = (x + dx, y + dy);
            sum += x * ny - nx * y;
            (x, y) = (nx, ny);
        }
        sum
    }

    /// Whether this loop goes round a hole.
    #[cfg(test)]
    pub fn is_hole(&self) -> bool {
        self.area2() < 0
    }
}

/// Every edge between covered and uncovered pixels of `mask` (`width` ×
/// `height`, row by row) as closed loops. Where two covered pixels touch
/// only at a corner, the loops keep them apart (covered pixels connect
/// sideways only, as the regions do).
pub(super) fn loops(mask: &[bool], width: usize, height: usize) -> Vec<Loop> {
    let corners = width + 1;
    let at = |x: usize, y: usize| y * corners + x;
    let covered = |x: usize, y: usize| mask[y * width + x];
    // per pixel corner, the steps that leave it (bit per direction)
    let mut out = vec![0_u8; corners * (height + 1)];
    for y in 0..height {
        for x in 0..width {
            if !covered(x, y) {
                continue;
            }
            if y == 0 || !covered(x, y - 1) {
                out[at(x, y)] |= 1 << EAST;
            }
            if x + 1 == width || !covered(x + 1, y) {
                out[at(x + 1, y)] |= 1 << SOUTH;
            }
            if y + 1 == height || !covered(x, y + 1) {
                out[at(x + 1, y + 1)] |= 1 << WEST;
            }
            if x == 0 || !covered(x - 1, y) {
                out[at(x, y + 1)] |= 1 << NORTH;
            }
        }
    }

    // Where two pixels touch at a corner, two steps leave it: the one to
    // the right of the step in hugs the pixel just followed. The choice
    // is made on all the steps, so a loop closes on its own first step.
    let all = out.clone();
    let mut found = Vec::new();
    for first in 0..out.len() {
        while out[first] != 0 {
            let d0 = (0..4).find(|d| out[first] & (1 << d) != 0).unwrap_or(EAST);
            let (sx, sy) = (first % corners, first / corners);
            let mut steps = Vec::new();
            let (mut x, mut y, mut d) = (sx, sy, d0);
            loop {
                out[at(x, y)] &= !(1 << d);
                steps.push(d);
                let (dx, dy) = step(d);
                x = x.wrapping_add_signed(isize::try_from(dx).unwrap_or(0));
                y = y.wrapping_add_signed(isize::try_from(dy).unwrap_or(0));
                let ways = all[at(x, y)];
                d = if ways & (1 << right(d)) != 0 {
                    right(d)
                } else {
                    (0..4).find(|n| ways & (1 << n) != 0).unwrap_or(d)
                };
                if out[at(x, y)] & (1 << d) == 0 {
                    break; // that step is taken: back at the start
                }
            }
            found.push(Loop {
                start: (to_i64(sx), to_i64(sy)),
                steps,
                closed: true,
            });
        }
    }
    found
}

/// `loops` (from [`loops`] on `mask`) grouped into pieces: each area of
/// covered pixels with the holes in it, as indices into `loops`, in the
/// order the areas are met (row by row). A piece becomes one shape, so
/// separate parts of a colour can be moved apart.
pub(super) fn pieces(
    mask: &[bool],
    width: usize,
    height: usize,
    loops: &[Loop],
) -> Vec<Vec<usize>> {
    let labels: Vec<u8> = mask.iter().map(|&m| u8::from(m)).collect();
    let (area, _) = super::regions::regions(&labels, width, height);
    let mut by_area: std::collections::BTreeMap<u32, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, l) in loops.iter().enumerate() {
        // the covered pixel on the right of the first step
        let (x, y) = l.start;
        let (px, py) = match l.steps[0] {
            EAST => (x, y),
            SOUTH => (x - 1, y),
            WEST => (x - 1, y - 1),
            _ => (x, y - 1),
        };
        let (Ok(px), Ok(py)) = (usize::try_from(px), usize::try_from(py)) else {
            continue; // never: a loop's covered pixel is in the image
        };
        by_area.entry(area[py * width + px]).or_default().push(i);
    }
    by_area.into_values().collect()
}

pub(super) fn to_i64(v: usize) -> i64 {
    i64::try_from(v).expect("pixel corners fit in i64")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(rows: &[&str]) -> (Vec<bool>, usize, usize) {
        let w = rows[0].len();
        let m = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| c == '#'))
            .collect();
        (m, w, rows.len())
    }

    #[test]
    fn a_square_is_one_clockwise_loop() {
        let (m, w, h) = mask(&["....", ".##.", ".##.", "...."]);
        let l = loops(&m, w, h);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].steps.len(), 8);
        assert_eq!(l[0].area2(), 8);
        assert!(!l[0].is_hole());
    }

    #[test]
    fn a_ring_gives_a_shape_and_a_hole() {
        let (m, w, h) = mask(&["###", "#.#", "###"]);
        let l = loops(&m, w, h);
        assert_eq!(l.len(), 2);
        assert_eq!(l.iter().filter(|l| l.is_hole()).count(), 1);
        assert_eq!(l.iter().map(Loop::area2).sum::<i64>(), 16);
    }

    #[test]
    fn pixels_touching_at_a_corner_stay_apart() {
        let (m, w, h) = mask(&["#.", ".#"]);
        let l = loops(&m, w, h);
        assert_eq!(l.len(), 2);
        assert!(l.iter().all(|l| l.steps.len() == 4 && !l.is_hole()));
    }

    #[test]
    fn each_area_is_a_piece_with_its_holes() {
        // a ring with a hole, and a dot apart from it
        let (m, w, h) = mask(&["###..", "#.#.#", "###.."]);
        let found = loops(&m, w, h);
        let parts = pieces(&m, w, h, &found);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].len(), 2, "the ring and its hole");
        assert_eq!(parts[1].len(), 1, "the dot");
    }

    #[test]
    fn the_whole_image_is_one_loop_along_its_border() {
        let (m, w, h) = mask(&["###", "###"]);
        let l = loops(&m, w, h);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].area2(), 12);
    }
}
