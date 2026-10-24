//! Outlines only (spec 13b): the edges between colour areas, each drawn
//! once. Every pixel side between two different colours (transparent
//! counts as one) is an edge; edges join into lines that end where three
//! or more areas meet, or at the image's border, and loops where an area
//! sits inside another. The image's own border is not an edge.

use super::outline::{Dir, Loop, step, to_i64};

/// The opposite step.
fn back(d: Dir) -> Dir {
    (d + 2) % 4
}

/// Every edge between differently coloured pixels of `labels` (`width` ×
/// `height`, row by row), as open lines and closed loops along the pixel
/// grid, each edge in exactly one of them.
pub(super) fn lines(labels: &[u8], width: usize, height: usize) -> Vec<Loop> {
    let corners = width + 1;
    let at = |x: usize, y: usize| y * corners + x;
    // per pixel corner, the edges that meet there (bit per direction)
    let mut ways = vec![0_u8; corners * (height + 1)];
    for y in 0..height {
        for x in 0..width {
            let here = labels[y * width + x];
            // the edge along this pixel's top, between it and the one above
            if y > 0 && labels[(y - 1) * width + x] != here {
                ways[at(x, y)] |= 1 << 0; // east
                ways[at(x + 1, y)] |= 1 << 2; // west
            }
            // the edge along its left side
            if x > 0 && labels[y * width + x - 1] != here {
                ways[at(x, y)] |= 1 << 1; // south
                ways[at(x, y + 1)] |= 1 << 3; // north
            }
        }
    }
    // where a line ends: any corner where other than two edges meet
    let ends: Vec<bool> = ways.iter().map(|w| w.count_ones() != 2).collect();
    let mut left = ways;
    let mut found = Vec::new();
    let walk = |from: usize, d: Dir, left: &mut Vec<u8>| {
        let (mut x, mut y) = (from % corners, from / corners);
        let mut steps = Vec::new();
        let mut dir = d;
        loop {
            left[at(x, y)] &= !(1 << dir);
            let (dx, dy) = step(dir);
            x = x.wrapping_add_signed(isize::try_from(dx).unwrap_or(0));
            y = y.wrapping_add_signed(isize::try_from(dy).unwrap_or(0));
            left[at(x, y)] &= !(1 << back(dir));
            steps.push(dir);
            let here = at(x, y);
            if here == from || ends[here] {
                break;
            }
            // a corner on the line: the one edge left goes on
            match (0..4).find(|n| left[here] & (1 << n) != 0) {
                Some(next) => dir = next,
                None => break,
            }
        }
        let closed = at(x, y) == from && !ends[from];
        Loop {
            start: (to_i64(from % corners), to_i64(from / corners)),
            steps,
            closed,
        }
    };
    // lines first, from their ends; what is left are loops
    for v in 0..left.len() {
        if !ends[v] {
            continue;
        }
        while let Some(d) = (0..4).find(|d| left[v] & (1 << d) != 0) {
            found.push(walk(v, d, &mut left));
        }
    }
    for v in 0..left.len() {
        while let Some(d) = (0..4).find(|d| left[v] & (1 << d) != 0) {
            found.push(walk(v, d, &mut left));
        }
    }
    found
}

/// Where `line` ends (its start when it is a loop).
fn end(line: &Loop) -> (i64, i64) {
    line.steps.iter().fold(line.start, |(x, y), &d| {
        let (dx, dy) = step(d);
        (x + dx, y + dy)
    })
}

/// The root of `i` in the union-find `parent`.
fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// `lines` grouped into pieces: lines that meet (share an end) are one
/// piece, as indices into `lines`, in the order of their first line. A
/// piece becomes one shape.
pub(super) fn pieces(lines: &[Loop]) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..lines.len()).collect();
    let mut first_at: std::collections::HashMap<(i64, i64), usize> =
        std::collections::HashMap::new();
    for (i, line) in lines.iter().enumerate() {
        if line.closed {
            continue; // touches no other line
        }
        for point in [line.start, end(line)] {
            let other = *first_at.entry(point).or_insert(i);
            let (a, b) = (find(&mut parent, i), find(&mut parent, other));
            // the lower index leads, so pieces keep the lines' order
            parent[a.max(b)] = a.min(b);
        }
    }
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut slot: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for i in 0..lines.len() {
        let root = find(&mut parent, i);
        let at = *slot.entry(root).or_insert_with(|| {
            out.push(Vec::new());
            out.len() - 1
        });
        out[at].push(i);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(rows: &[&str]) -> (Vec<u8>, usize, usize) {
        let w = rows[0].len();
        let l = rows.iter().flat_map(|r| r.bytes()).collect();
        (l, w, rows.len())
    }

    fn edges(found: &[Loop]) -> usize {
        found.iter().map(|l| l.steps.len()).sum()
    }

    #[test]
    fn one_colour_has_no_edges() {
        let (l, w, h) = labels(&["aaa", "aaa"]);
        assert_eq!(lines(&l, w, h), Vec::new());
    }

    #[test]
    fn two_halves_meet_along_one_open_line_ending_at_the_border() {
        let (l, w, h) = labels(&["aabb", "aabb", "aabb"]);
        let found = lines(&l, w, h);
        assert_eq!(found.len(), 1);
        assert!(!found[0].closed);
        assert_eq!(found[0].steps.len(), 3, "the image border is no edge");
    }

    #[test]
    fn an_area_inside_another_is_one_loop_drawn_once() {
        let (l, w, h) = labels(&["aaaa", "abba", "abba", "aaaa"]);
        let found = lines(&l, w, h);
        assert_eq!(found.len(), 1);
        assert!(found[0].closed);
        assert_eq!(found[0].steps.len(), 8);
    }

    #[test]
    fn lines_that_meet_are_one_piece_and_a_loop_apart_is_another() {
        // a | b over c (three lines meeting), and a dot of d inside c
        let (l, w, h) = labels(&["aabb", "aabb", "cccc", "cdcc", "cccc"]);
        let found = lines(&l, w, h);
        let p = pieces(&found);
        assert_eq!(p.len(), 2, "{p:?}");
        assert_eq!(p.iter().map(Vec::len).sum::<usize>(), found.len());
        assert!(p.iter().any(|piece| piece.len() == 3));
    }

    #[test]
    fn three_areas_meeting_give_three_lines_and_every_edge_once() {
        // a | b on top, c below: they meet at one corner
        let (l, w, h) = labels(&["aabb", "aabb", "cccc", "cccc"]);
        let found = lines(&l, w, h);
        assert_eq!(found.len(), 3);
        assert!(found.iter().all(|l| !l.closed));
        // a|b: 2 edges; a/c: 2; b/c: 2
        assert_eq!(edges(&found), 6);
    }
}
