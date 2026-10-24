//! Image to vector cost (spec 13b). Budget: ≤ 1 s in release to decode and
//! trace a 4 000 × 3 000 image (a photo-like PNG: gradients, noise, soft
//! blobs), per mode.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use vextor_core::ImageMime;
use vextor_geom::{Pixels, TraceMode, trace};
use vextor_render::{rgba_png, trace_pixels};

/// A `w` × `h` photo-like image as PNG bytes.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn photo(w: u32, h: u32) -> Vec<u8> {
    let mut seed = 0x2545_f491_u32;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let noise = f64::from(seed % 41) - 20.0;
            let (fx, fy) = (f64::from(x) / f64::from(w), f64::from(y) / f64::from(h));
            let blob =
                |cx: f64, cy: f64, r: f64| (-((fx - cx).powi(2) + (fy - cy).powi(2)) / r).exp();
            let red = 90.0 + 120.0 * fx + 80.0 * blob(0.3, 0.4, 0.02);
            let green = 60.0 + 140.0 * fy + 60.0 * blob(0.7, 0.3, 0.05);
            let blue =
                140.0 + 60.0 * (fx * 9.0).sin() * (fy * 7.0).cos() + 90.0 * blob(0.5, 0.8, 0.03);
            for v in [red, green, blue] {
                rgba.push((v + noise).clamp(0.0, 255.0) as u8);
            }
            rgba.push(255);
        }
    }
    rgba_png(w, h, rgba).expect("encodes")
}

fn bench(c: &mut Criterion) {
    let png = photo(4000, 3000);
    let mut group = c.benchmark_group("trace 4000x3000");
    group.sample_size(10);
    for (name, mode) in [
        ("black and white", TraceMode::BlackAndWhite),
        ("3 colours", TraceMode::Colors3),
        ("6 colours", TraceMode::Colors6),
        ("outlines only", TraceMode::Outlines),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| {
                let px = trace_pixels(ImageMime::Png, black_box(&png)).expect("decodes");
                let pixels = Pixels {
                    width: px.width as usize,
                    height: px.height as usize,
                    rgba: &px.rgba,
                };
                black_box(trace(pixels, mode))
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
