//! Regions of one colour: speckles merged away, and the stacking order.

use super::colors::EMPTY;

/// Regions smaller than this share of the image are speckles (0.05 %).
const SPECKLE_SHARE: f64 = 0.0005;

/// Union-find over regions, each root holding its region's size and label.
struct Regions {
    parent: Vec<u32>,
    size: Vec<usize>,
    label: Vec<u8>,
}

impl Regions {
    fn find(&mut self, mut r: u32) -> u32 {
        while self.parent[r as usize] != r {
            let up = self.parent[self.parent[r as usize] as usize];
            self.parent[r as usize] = up;
            r = up;
        }
        r
    }
}

/// Each pixel's region (4-connected, same label) and the regions' pixels,
/// in the order first met, row by row.
pub(super) fn regions(labels: &[u8], width: usize, height: usize) -> (Vec<u32>, Vec<Vec<usize>>) {
    let mut region = vec![u32::MAX; labels.len()];
    let mut pixels: Vec<Vec<usize>> = Vec::new();
    let mut stack = Vec::new();
    for start in 0..labels.len() {
        if region[start] != u32::MAX {
            continue;
        }
        let id = u32::try_from(pixels.len()).expect("fewer regions than pixels, < 2^32");
        let mut members = Vec::new();
        region[start] = id;
        stack.push(start);
        while let Some(p) = stack.pop() {
            members.push(p);
            let (x, y) = (p % width, p / width);
            let near = [
                (x > 0).then(|| p - 1),
                (x + 1 < width).then(|| p + 1),
                (y > 0).then(|| p - width),
                (y + 1 < height).then(|| p + width),
            ];
            for q in near.into_iter().flatten() {
                if region[q] == u32::MAX && labels[q] == labels[p] {
                    region[q] = id;
                    stack.push(q);
                }
            }
        }
        pixels.push(members);
    }
    (region, pixels)
}

/// Merges every region smaller than [`SPECKLE_SHARE`] of the image into
/// its largest neighbour, smallest first. Transparent regions count too: a
/// speck of colour on a transparent background goes away, a pinhole in a
/// shape is filled.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub(super) fn merge_speckles(labels: &mut [u8], width: usize, height: usize) {
    let min = ((labels.len() as f64 * SPECKLE_SHARE).ceil() as usize).max(1);
    let (region, pixels) = regions(labels, width, height);
    let count = pixels.len();
    let mut regions = Regions {
        parent: (0..u32::try_from(count).expect("checked in regions")).collect(),
        size: pixels.iter().map(Vec::len).collect(),
        label: pixels.iter().map(|p| labels[p[0]]).collect(),
    };
    let mut small: Vec<usize> = (0..count).filter(|&r| pixels[r].len() < min).collect();
    small.sort_by_key(|&r| (pixels[r].len(), r));
    for r in small {
        let me = regions.find(r as u32);
        let mut best: Option<(usize, u32)> = None;
        for &p in &pixels[r] {
            let (x, y) = (p % width, p / width);
            let near = [
                (x > 0).then(|| p - 1),
                (x + 1 < width).then(|| p + 1),
                (y > 0).then(|| p - width),
                (y + 1 < height).then(|| p + width),
            ];
            for q in near.into_iter().flatten() {
                let other = regions.find(region[q]);
                if other == me {
                    continue;
                }
                let size = regions.size[other as usize];
                // largest; equal sizes: the lowest region, so it is repeatable
                if best.is_none_or(|(s, o)| size > s || (size == s && other < o)) {
                    best = Some((size, other));
                }
            }
        }
        if let Some((_, into)) = best {
            regions.parent[me as usize] = into;
            regions.size[into as usize] += regions.size[me as usize];
        }
    }
    for (p, l) in labels.iter_mut().enumerate() {
        let root = regions.find(region[p]);
        *l = regions.label[root as usize];
    }
}

/// Areas thinner than this on average (twice the area over the edge
/// length, pixels) are slivers: the antialiased band along an edge.
const SLIVER_WIDTH: f64 = 1.5;

/// Gives every sliver (an area under [`SLIVER_WIDTH`] wide: the band of
/// in-between colours an antialiased edge leaves) the label of its
/// largest neighbour, so a spot is seen inside the area around it, not
/// inside its own edge (owner review: ear spots).
#[allow(clippy::cast_precision_loss)] // pixel counts, far below 2^52
pub(super) fn merge_slivers(labels: &mut [u8], width: usize, height: usize) {
    let (region, members) = regions(labels, width, height);
    let mut relabel = Vec::new();
    for pixels in &members {
        let (mut edges, mut best) = (0_usize, None::<(usize, u32)>);
        for &p in pixels {
            let (x, y) = (p % width, p / width);
            let near = [
                (x > 0).then(|| p - 1),
                (x + 1 < width).then(|| p + 1),
                (y > 0).then(|| p - width),
                (y + 1 < height).then(|| p + width),
            ];
            for q in near.into_iter().flatten() {
                let other = region[q];
                if other == region[p] {
                    continue;
                }
                edges += 1;
                let size = members[other as usize].len();
                if best.is_none_or(|(s, o)| size > s || (size == s && other < o)) {
                    best = Some((size, other));
                }
            }
        }
        let thin = edges > 0 && 2.0 * pixels.len() as f64 / (edges as f64) < SLIVER_WIDTH;
        if let (true, Some((_, into))) = (thin, best) {
            relabel.push((pixels, labels[members[into as usize][0]]));
        }
    }
    for (pixels, label) in relabel {
        for &p in pixels {
            labels[p] = label;
        }
    }
}

/// Palette indices in stacking order, bottom first: most pixels first, so
/// smaller areas sit on top. Colours left with no pixels are dropped.
pub(super) fn stacking(labels: &[u8], colors: usize) -> Vec<usize> {
    let mut counts = vec![0_usize; colors];
    for &l in labels.iter().filter(|&&l| l != EMPTY) {
        counts[usize::from(l)] += 1;
    }
    let mut order: Vec<usize> = (0..colors).filter(|&c| counts[c] > 0).collect();
    order.sort_by_key(|&c| (std::cmp::Reverse(counts[c]), c));
    order
}

/// How far, in pixels, a colour's shape reaches under the colours stacked
/// above it: enough that no seam shows where they meet, little enough
/// that moving the upper piece away shows only a thin rim.
const UNDERLAP: usize = 2;

/// Which pixels the `rank`-th colour's shape covers: its own; those of the
/// colours stacked above it within [`UNDERLAP`] of them, so the colours
/// above paint over the join and no seam shows; and the holes it encloses
/// that hold only colours above it (a face stays whole under its eyes).
/// Nothing else of the colours above, so each piece can be moved on its
/// own without a copy of another piece under it (owner review).
pub(super) fn stacked_mask(
    labels: &[u8],
    order: &[usize],
    rank: usize,
    width: usize,
    height: usize,
) -> Vec<bool> {
    let mut rank_of = [usize::MAX; 256];
    for (r, &c) in order.iter().enumerate() {
        rank_of[c] = r;
    }
    let above = |l: u8| {
        l != EMPTY && rank_of[usize::from(l)] != usize::MAX && rank_of[usize::from(l)] > rank
    };
    let mut mask: Vec<bool> = labels
        .iter()
        .map(|&l| l != EMPTY && rank_of[usize::from(l)] == rank)
        .collect();
    for _ in 0..UNDERLAP {
        let before = mask.clone();
        for (p, m) in mask.iter_mut().enumerate() {
            if *m || !above(labels[p]) {
                continue;
            }
            let (x, y) = (p % width, p / width);
            *m = (x > 0 && before[p - 1])
                || (x + 1 < width && before[p + 1])
                || (y > 0 && before[p - width])
                || (y + 1 < height && before[p + width]);
        }
    }
    let as_labels: Vec<u8> = mask.iter().map(|&m| u8::from(m)).collect();
    let (_, areas) = regions(&as_labels, width, height);
    for pixels in areas {
        let edge = |p: usize| {
            let (x, y) = (p % width, p / width);
            x == 0 || y == 0 || x + 1 == width || y + 1 == height
        };
        let enclosed_above =
            !mask[pixels[0]] && pixels.iter().all(|&p| !edge(p) && above(labels[p]));
        if enclosed_above {
            for p in pixels {
                mask[p] = true;
            }
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_speck_merges_into_its_largest_neighbour() {
        // one dark pixel in light; regions under 5 px are speckles
        let (w, h) = (100, 100);
        let mut labels = vec![EMPTY; w * h];
        labels[50 * w + 50] = 0;
        for x in 0..10 {
            for y in 0..10 {
                labels[y * w + x] = 1;
            }
        }
        merge_speckles(&mut labels, w, h);
        assert_eq!(labels[50 * w + 50], EMPTY);
        assert_eq!(labels[0], 1, "a big region stays");
    }

    #[test]
    fn a_pinhole_in_a_shape_is_filled() {
        let (w, h) = (100, 100);
        let mut labels = vec![0; w * h];
        labels[10 * w + 10] = EMPTY;
        merge_speckles(&mut labels, w, h);
        assert!(labels.iter().all(|&l| l == 0));
    }

    #[test]
    fn a_one_pixel_band_along_an_edge_joins_its_largest_neighbour() {
        // a spot (1) inside a field (0), with a 1 px band (2) round it
        let (mut l, w, h) = grid(&[
            "000000000",
            "022222220",
            "021111120",
            "021111120",
            "021111120",
            "022222220",
            "000000000",
        ]);
        merge_slivers(&mut l, w, h);
        assert_eq!(l[w + 1], 0, "the band joins the field");
        assert_eq!(l[3 * w + 4], 1, "the spot stays");
    }

    #[test]
    fn stacking_puts_the_largest_colour_at_the_bottom() {
        let labels = [0, 1, 1, 1, 2, 2, EMPTY];
        assert_eq!(stacking(&labels, 4), [1, 2, 0], "colour 3 has no pixels");
    }

    /// `rows` of colour digits (`.` transparent) as labels.
    fn grid(rows: &[&str]) -> (Vec<u8>, usize, usize) {
        let labels = rows
            .iter()
            .flat_map(|r| r.bytes().map(|b| if b == b'.' { EMPTY } else { b - b'0' }))
            .collect();
        (labels, rows[0].len(), rows.len())
    }

    fn shown(mask: &[bool], width: usize) -> Vec<String> {
        mask.chunks(width)
            .map(|r| r.iter().map(|&m| if m { '#' } else { '.' }).collect())
            .collect()
    }

    #[test]
    fn a_colour_reaches_two_pixels_under_a_neighbour_above_it() {
        // 0 (big) meets 1 (smaller, stacked above) along a straight line
        let (l, w, h) = grid(&["0000011111", "0000011111", "0000011111"]);
        let order = stacking(&l, 2);
        assert_eq!(order, [0, 1]);
        assert_eq!(
            shown(&stacked_mask(&l, &order, 0, w, h), w),
            ["#######...", "#######...", "#######..."]
        );
        assert_eq!(
            shown(&stacked_mask(&l, &order, 1, w, h), w),
            [".....#####", ".....#####", ".....#####"],
            "the top colour covers only itself"
        );
    }

    #[test]
    fn a_colour_stays_whole_under_what_it_encloses_and_nothing_else() {
        // a face (0) with an eye (1) inside; another 1 apart, on transparency
        let (l, w, h) = grid(&[
            "0000000...111",
            "0000000...111",
            "0001100...111",
            "0001100......",
            "0000000......",
            "0000000......",
        ]);
        let order = stacking(&l, 2);
        let face = shown(&stacked_mask(&l, &order, 0, w, h), w);
        assert_eq!(
            face,
            [
                "#######......",
                "#######......",
                "#######......",
                "#######......",
                "#######......",
                "#######......",
            ],
            "the eye's hole filled, the other 1 not copied"
        );
    }
}
