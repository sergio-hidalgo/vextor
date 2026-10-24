//! Each pixel's colour: an Otsu threshold for black and white, k-means in
//! sRGB for 3 and 6 colours (and for outlines, which follow the edges
//! between 6 colours).

use std::collections::HashMap;

use vextor_core::Rgba;

use super::{Pixels, TraceMode};

/// A pixel that is not traced: transparent, or light in black and white.
pub(super) const EMPTY: u8 = u8::MAX;
/// Pixels with less alpha than this are transparent (50 %).
const OPAQUE_FROM: u8 = 128;
/// Bits kept per channel in the k-means colour histogram.
const BIN_BITS: u32 = 5;
/// k-means rounds at most.
const ROUNDS: usize = 32;
/// 3 colours: an area at most this share of the image's longer side wide
/// (on average) is a drawn line...
const LINE_SHARE: f64 = 0.015;
/// ... or at most this many pixels, on small images.
const MIN_LINE_WIDTH: f64 = 3.0;
/// ... and when its surroundings are this much lighter (luma), it is drawn
/// in the darkest colour.
const LINE_CONTRAST: u32 = 50;
/// 3 colours: an area enclosed by one other area, at most this share of
/// the image's pixels...
const SPOT_SHARE: f64 = 0.01;
/// ... and this much darker (luma) than the area around it, is a drawn
/// spot (a darker patch inside an ear) and takes the darkest colour too.
const SPOT_CONTRAST: u32 = 35;
/// Colours of the finer trace the details are looked for in: more than 6,
/// so a detail keeps its own colour on a busy drawing.
const DETAIL_COLOURS: usize = 10;

/// Each pixel's palette index ([`EMPTY`] when not traced) and the palette.
pub(super) fn classify(px: Pixels<'_>, mode: TraceMode) -> (Vec<u8>, Vec<Rgba>) {
    match mode {
        TraceMode::BlackAndWhite => black_and_white(px),
        TraceMode::Colors3 => {
            let (mut labels, palette) = k_means(px, 3);
            keep_dark_details(px, &mut labels, &palette);
            (labels, palette)
        }
        TraceMode::Colors6 | TraceMode::Outlines => k_means(px, 6),
    }
}

fn opaque(p: [u8; 4]) -> bool {
    p[3] >= OPAQUE_FROM
}

/// Rec. 709 luma of an sRGB pixel seen on a white page (its colour
/// blended over white by its alpha), 0–255.
fn luma(p: [u8; 4]) -> u8 {
    let a = u32::from(p[3]);
    let on_white = |c: u8| (u32::from(c) * a + 255 * (255 - a) + 127) / 255;
    let y = (2126 * on_white(p[0]) + 7152 * on_white(p[1]) + 722 * on_white(p[2])) / 10_000;
    u8::try_from(y).unwrap_or(u8::MAX)
}

/// The image as seen on a white page, cut into dark and light. Every pixel
/// counts towards the cut, transparent ones as white: a cut-out drawing
/// gets the same cut as the same drawing on white (without them, the cut
/// fell among the drawing's mid-tone lines and dropped some). Transparent
/// pixels are still never traced.
fn black_and_white(px: Pixels<'_>) -> (Vec<u8>, Vec<Rgba>) {
    let mut histogram = [0_u64; 256];
    for &p in px.rgba.as_chunks::<4>().0 {
        histogram[usize::from(luma(p))] += 1;
    }
    let threshold = otsu(&histogram);
    let labels = px
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            if opaque(*p) && luma(*p) <= threshold {
                0
            } else {
                EMPTY
            }
        })
        .collect();
    (labels, vec![Rgba::BLACK])
}

/// The level that splits `histogram` into dark (≤ level) and light with
/// the largest between-class variance (Otsu 1979). With one level only,
/// it is dark when below the middle.
#[allow(clippy::cast_precision_loss)] // pixel counts stay far below 2^52
pub(super) fn otsu(histogram: &[u64; 256]) -> u8 {
    let total: u64 = histogram.iter().sum();
    let levels = histogram.iter().filter(|&&n| n > 0).count();
    if levels <= 1 {
        let only = histogram.iter().position(|&n| n > 0).unwrap_or(255);
        return if only < 128 { u8::MAX } else { 0 };
    }
    let weighted: f64 = histogram
        .iter()
        .enumerate()
        .map(|(i, &n)| i as f64 * n as f64)
        .sum();
    let (mut dark, mut dark_sum) = (0.0, 0.0);
    let (mut best, mut best_level) = (-1.0, 0_u8);
    for (level, &n) in (0_u8..=255).zip(histogram) {
        dark += n as f64;
        dark_sum += f64::from(level) * n as f64;
        let light = total as f64 - dark;
        if dark == 0.0 || light == 0.0 {
            continue;
        }
        let gap = dark_sum / dark - (weighted - dark_sum) / light;
        let between = dark * light * gap * gap;
        if between > best {
            best = between;
            best_level = level;
        }
    }
    best_level
}

/// A histogram bin: how many pixels and the sum of their colours.
#[derive(Clone, Copy, Default)]
struct Bin {
    count: u64,
    sum: [u64; 3],
}

#[allow(clippy::cast_precision_loss)] // pixel counts stay far below 2^52
impl Bin {
    fn mean(&self) -> [f64; 3] {
        let n = self.count as f64;
        [
            self.sum[0] as f64 / n,
            self.sum[1] as f64 / n,
            self.sum[2] as f64 / n,
        ]
    }
}

fn distance2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn nearest(c: [f64; 3], centres: &[[f64; 3]]) -> usize {
    let mut best = (f64::INFINITY, 0);
    for (i, &m) in centres.iter().enumerate() {
        let d = distance2(c, m);
        if d < best.0 {
            best = (d, i);
        }
    }
    best.1
}

fn bin_of(p: [u8; 4]) -> usize {
    let shift = 8 - BIN_BITS;
    (usize::from(p[0] >> shift) << (2 * BIN_BITS))
        | (usize::from(p[1] >> shift) << BIN_BITS)
        | usize::from(p[2] >> shift)
}

/// The opaque pixels reduced to at most `k` colours. k-means runs on a
/// colour histogram (fast on big images); the first centre is the most
/// common colour, each next one the colour furthest from the centres,
/// weighted by how common it is, so no random seed is needed and the same
/// image always gives the same colours. Each colour of the palette is a
/// real one: the most common colour among its pixels (see [`palette`]).
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // means of u8 values: 0–255
fn k_means(px: Pixels<'_>, k: usize) -> (Vec<u8>, Vec<Rgba>) {
    let mut histogram = vec![Bin::default(); 1 << (3 * BIN_BITS)];
    for &p in px.rgba.as_chunks::<4>().0.iter().filter(|p| opaque(**p)) {
        let bin = &mut histogram[bin_of(p)];
        bin.count += 1;
        for (s, c) in bin.sum.iter_mut().zip(p) {
            *s += u64::from(c);
        }
    }
    let bins: Vec<([f64; 3], f64)> = histogram
        .iter()
        .filter(|b| b.count > 0)
        .map(|b| (b.mean(), b.count as f64))
        .collect();
    let Some(first) = bins
        .iter()
        .fold(None::<&([f64; 3], f64)>, |best, b| match best {
            Some(m) if m.1 >= b.1 => Some(m),
            _ => Some(b),
        })
    else {
        return (vec![EMPTY; px.rgba.len() / 4], Vec::new());
    };
    let mut centres = vec![first.0];
    while centres.len() < k {
        let mut best = (0.0, None);
        for &(c, w) in &bins {
            let d = w * centres
                .iter()
                .map(|&m| distance2(c, m))
                .fold(f64::INFINITY, f64::min);
            if d > best.0 {
                best = (d, Some(c));
            }
        }
        match best.1 {
            Some(c) => centres.push(c),
            None => break,
        }
    }

    let mut assigned = vec![usize::MAX; bins.len()];
    for _ in 0..ROUNDS {
        let mut changed = false;
        let mut sums = vec![([0.0; 3], 0.0); centres.len()];
        for (i, &(c, w)) in bins.iter().enumerate() {
            let n = nearest(c, &centres);
            changed |= assigned[i] != n;
            assigned[i] = n;
            let s = &mut sums[n];
            for (a, v) in s.0.iter_mut().zip(c) {
                *a += v * w;
            }
            s.1 += w;
        }
        for (m, (s, w)) in centres.iter_mut().zip(&sums) {
            if *w > 0.0 {
                *m = [s[0] / w, s[1] / w, s[2] / w];
            }
        }
        if !changed {
            break;
        }
    }

    // Every pixel to its nearest centre.
    let mut labels: Vec<u8> = px
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            if !opaque(*p) {
                return EMPTY;
            }
            let c = [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
            nearest(c, &centres) as u8
        })
        .collect();
    let colors = palette(px, &labels, centres.len());
    let mut index = vec![EMPTY; centres.len()];
    let mut out = Vec::new();
    for (i, c) in colors.into_iter().enumerate() {
        if let Some(c) = c {
            index[i] = out.len() as u8;
            out.push(c);
        }
    }
    for l in labels.iter_mut().filter(|l| **l != EMPTY) {
        *l = index[usize::from(*l)];
    }
    (labels, out)
}

/// 3 colours: drawn details darker than what surrounds them take the
/// darkest of the 3 colours. With 3 slots a mid-tone detail otherwise
/// joins the lighter area it is drawn on and vanishes (owner reviews).
/// Details are found in the areas of a finer, [`DETAIL_COLOURS`] trace:
/// - lines (a ring round a snout, an eye's rim): thin (twice the area over
///   the edge length, at most [`LINE_SHARE`] of the image) and
///   [`LINE_CONTRAST`] darker than their neighbours on average;
/// - spots (a darker patch inside an ear): enclosed by one other area, at
///   most [`SPOT_SHARE`] of the image, and [`SPOT_CONTRAST`] darker than
///   it. Shading that touches several areas is neither, and keeps its
///   colour.
///
/// Transparency counts as white.
#[allow(clippy::cast_precision_loss)] // pixel counts, far below 2^52
fn keep_dark_details(px: Pixels<'_>, labels: &mut [u8], palette: &[Rgba]) {
    let as_pixel = |c: Rgba| [c.r, c.g, c.b, 255];
    let Some(darkest) = (0..palette.len()).min_by_key(|&i| luma(as_pixel(palette[i]))) else {
        return;
    };
    let (w, h) = (px.width, px.height);
    let (mut fine, fine_palette) = k_means(px, DETAIL_COLOURS);
    super::regions::merge_speckles(&mut fine, w, h);
    super::regions::merge_slivers(&mut fine, w, h);
    let fine_luma = |l: u8| {
        if l == EMPTY {
            255
        } else {
            u32::from(luma(as_pixel(fine_palette[usize::from(l)])))
        }
    };
    let max_width = (w.max(h) as f64 * LINE_SHARE).max(MIN_LINE_WIDTH);
    let max_spot = (w * h) as f64 * SPOT_SHARE;
    let (region, members) = super::regions::regions(&fine, w, h);
    for pixels in &members {
        let own = fine[pixels[0]];
        if own == EMPTY {
            continue;
        }
        let (mut edges, mut around) = (0_u64, 0_u64);
        // the one area around it, while there is only one
        let (mut outside, mut inside_one) = (None, true);
        for &p in pixels {
            let (x, y) = (p % w, p / w);
            let near = [
                (x > 0).then(|| p - 1),
                (x + 1 < w).then(|| p + 1),
                (y > 0).then(|| p - w),
                (y + 1 < h).then(|| p + w),
            ];
            for q in near.into_iter().flatten() {
                if region[q] != region[p] {
                    edges += 1;
                    around += u64::from(fine_luma(fine[q]));
                    inside_one &= *outside.get_or_insert(region[q]) == region[q];
                }
                // touching the image's border: not enclosed
                inside_one &= x > 0 && y > 0 && x + 1 < w && y + 1 < h;
            }
        }
        if edges == 0 {
            continue;
        }
        let width = 2.0 * pixels.len() as f64 / edges as f64;
        let lighter_around = around / edges >= u64::from(fine_luma(own) + LINE_CONTRAST);
        let line = width <= max_width && lighter_around;
        let spot = inside_one
            && pixels.len() as f64 <= max_spot
            && around / edges >= u64::from(fine_luma(own) + SPOT_CONTRAST);
        if line || spot {
            for &p in pixels {
                labels[p] = u8::try_from(darkest).unwrap_or(0);
            }
        }
    }
}

/// Each of the `k` colours as a colour of the image: the most common exact
/// colour within its most common histogram bin. A mean would mix the
/// antialiased edges and the neighbouring shades in (black outlines came
/// out `#061004`); the bin first keeps a photo's noise from picking a
/// stray pixel. `None` for a colour no pixel has. Ties go to the lower
/// bin and colour, so the result is repeatable.
fn palette(px: Pixels<'_>, labels: &[u8], k: usize) -> Vec<Option<Rgba>> {
    let bins = 1 << (3 * BIN_BITS);
    let mut counts = vec![0_u32; k * bins];
    for (&p, &l) in px.rgba.as_chunks::<4>().0.iter().zip(labels) {
        if l != EMPTY {
            counts[usize::from(l) * bins + bin_of(p)] += 1;
        }
    }
    let best: Vec<Option<usize>> = (0..k)
        .map(|c| {
            let row = &counts[c * bins..(c + 1) * bins];
            let (bin, &n) = row
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(&a.0)))?;
            (n > 0).then_some(bin)
        })
        .collect();
    let mut exact: Vec<HashMap<[u8; 3], u32>> = vec![HashMap::new(); k];
    for (&p, &l) in px.rgba.as_chunks::<4>().0.iter().zip(labels) {
        if l != EMPTY && best[usize::from(l)] == Some(bin_of(p)) {
            *exact[usize::from(l)].entry([p[0], p[1], p[2]]).or_default() += 1;
        }
    }
    exact
        .iter()
        .map(|seen| {
            seen.iter()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                .map(|(c, _)| Rgba::rgb(c[0], c[1], c[2]))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(colors: &[[u8; 4]]) -> Vec<u8> {
        colors.iter().flatten().copied().collect()
    }

    #[test]
    fn otsu_splits_two_tones_between_them() {
        let mut h = [0_u64; 256];
        h[30] = 500;
        h[200] = 300;
        let t = otsu(&h);
        assert!((30..200).contains(&t), "{t}");
    }

    #[test]
    fn otsu_with_one_level_is_dark_only_below_the_middle() {
        let mut h = [0_u64; 256];
        h[10] = 9;
        assert_eq!(otsu(&h), 255);
        let mut h = [0_u64; 256];
        h[240] = 9;
        assert_eq!(otsu(&h), 0);
    }

    #[test]
    fn transparent_pixels_are_never_traced() {
        let rgba = image(&[[0, 0, 0, 127], [0, 0, 0, 128], [255, 255, 255, 255]]);
        let px = Pixels {
            width: 3,
            height: 1,
            rgba: &rgba,
        };
        let (labels, _) = classify(px, TraceMode::BlackAndWhite);
        assert_eq!(labels, [EMPTY, 0, EMPTY]);
        let (labels, palette) = classify(px, TraceMode::Colors3);
        assert_eq!(labels[0], EMPTY);
        assert_eq!(palette.len(), 2);
    }

    #[test]
    fn a_transparent_background_counts_as_white_for_the_cut() {
        // a cut-out drawing: half transparent, a green body, a mid-dark
        // ring (luma 117) and black; on white the ring is dark
        let mut pixels = vec![[0, 0, 0, 0]; 50];
        pixels.extend(vec![[0x6b, 0xe2, 0x48, 255]; 40]);
        pixels.extend(vec![[0x4d, 0x8e, 0x00, 255]; 5]);
        pixels.extend(vec![[0, 0, 0, 255]; 5]);
        let rgba = image(&pixels);
        let px = Pixels {
            width: 10,
            height: 10,
            rgba: &rgba,
        };
        let (labels, _) = classify(px, TraceMode::BlackAndWhite);
        assert_eq!(labels[0], EMPTY, "transparent is never traced");
        assert_eq!(labels[50], EMPTY, "the body is light");
        assert_eq!(labels[90], 0, "the ring is dark");
        assert_eq!(labels[95], 0);
    }

    #[test]
    fn each_colour_is_the_most_common_real_one_not_a_mix() {
        let mut pixels = vec![[250, 250, 250, 255]; 60];
        pixels.extend(vec![[0, 0, 0, 255]; 20]);
        // antialiased edges and darker shades join black's colour
        pixels.extend([[3, 6, 2, 255], [19, 56, 14, 255], [12, 30, 8, 255]]);
        pixels.extend(vec![[0x6b, 0xe2, 0x48, 255]; 17]);
        let rgba = image(&pixels);
        let px = Pixels {
            width: 10,
            height: 10,
            rgba: &rgba,
        };
        let (_, palette) = classify(px, TraceMode::Colors3);
        assert!(palette.contains(&Rgba::BLACK), "{palette:?}");
        assert!(
            palette.contains(&Rgba::rgb(0x6b, 0xe2, 0x48)),
            "{palette:?}"
        );
    }

    #[test]
    #[allow(clippy::cast_precision_loss, clippy::many_single_char_names)]
    fn three_colours_draw_a_thin_mid_tone_ring_in_the_darkest_colour() {
        // a lime disc ringed in mid green on a green field outlined in
        // black, white around: the ring would join the green; it is a
        // drawn line
        let (w, h) = (60, 60);
        let pixels: Vec<[u8; 4]> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f64 - 30.0, (i / w) as f64 - 30.0);
                let d = x.hypot(y);
                if d < 3.0 {
                    [0, 0, 0, 255]
                } else if d < 12.0 {
                    [0xa5, 0xe8, 0x01, 255]
                } else if d < 15.0 {
                    [0x4d, 0x8e, 0x00, 255]
                } else if d < 24.0 {
                    [0x6b, 0xe2, 0x48, 255]
                } else if d < 29.0 {
                    [0, 0, 0, 255] // the outline
                } else {
                    [255, 255, 255, 255]
                }
            })
            .collect();
        let rgba = image(&pixels);
        let px = Pixels {
            width: w,
            height: h,
            rgba: &rgba,
        };
        let (labels, palette) = classify(px, TraceMode::Colors3);
        let ring = labels[30 * w + 30 + 13];
        assert_eq!(palette[usize::from(ring)], Rgba::BLACK, "{palette:?}");
        let field = labels[30 * w + 30 + 19];
        assert_ne!(palette[usize::from(field)], Rgba::BLACK, "the field stays");
    }

    #[test]
    #[allow(clippy::cast_precision_loss, clippy::many_single_char_names)]
    fn three_colours_draw_a_darker_spot_in_the_darkest_colour_but_not_shading() {
        // a green field outlined in black, white around; inside it a
        // darker-green spot (enclosed by the field) and a band of the same
        // darker green touching both the field and the outline (shading)
        let (w, h) = (100, 100);
        let pixels: Vec<[u8; 4]> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f64 - 50.0, (i / w) as f64 - 50.0);
                let d = x.hypot(y);
                if d >= 44.0 {
                    [255, 255, 255, 255]
                } else if d >= 38.0 {
                    [0, 0, 0, 255]
                } else if (x + 15.0).hypot(y + 15.0) < 5.0 || d >= 30.0 && y > 0.0 {
                    [0x46, 0xb3, 0x28, 255]
                } else {
                    [0x6b, 0xe2, 0x48, 255]
                }
            })
            .collect();
        let rgba = image(&pixels);
        let px = Pixels {
            width: w,
            height: h,
            rgba: &rgba,
        };
        let (labels, palette) = classify(px, TraceMode::Colors3);
        let at = |x: usize, y: usize| palette[usize::from(labels[y * w + x])];
        assert_eq!(at(35, 35), Rgba::BLACK, "the spot: {palette:?}");
        assert_ne!(at(50, 84), Rgba::BLACK, "the shading keeps a green");
        assert_ne!(at(50, 50), Rgba::BLACK, "the field");
    }

    #[test]
    fn k_means_finds_the_flat_colours_and_is_repeatable() {
        let red = [220, 30, 30, 255];
        let blue = [20, 40, 200, 255];
        let white = [250, 250, 250, 255];
        let mut pixels = vec![white; 60];
        pixels.extend(vec![red; 30]);
        pixels.extend(vec![blue; 10]);
        let rgba = image(&pixels);
        let px = Pixels {
            width: 10,
            height: 10,
            rgba: &rgba,
        };
        let (labels, mut palette) = classify(px, TraceMode::Colors3);
        assert_eq!(classify(px, TraceMode::Colors3).0, labels);
        palette.sort_by_key(|c| (c.r, c.g, c.b));
        assert_eq!(
            palette,
            [
                Rgba::rgb(20, 40, 200),
                Rgba::rgb(220, 30, 30),
                Rgba::rgb(250, 250, 250)
            ]
        );
        // 6 asked, 3 there: 3 given
        assert_eq!(classify(px, TraceMode::Colors6).1.len(), 3);
    }
}
