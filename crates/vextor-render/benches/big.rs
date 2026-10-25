//! Whole-document costs on very big files (spec 14 budgets, release, M1,
//! 20 000 nodes): open ≤ 300 ms, save ≤ 300 ms in the background and
//! ≤ 16 ms on the UI thread, a pan/zoom/drag frame ≤ 8 ms, undo/redo
//! ≤ 16 ms, hit-test or snap ≤ 1 ms per pointer move, SVG export ≤ 300 ms.
//!
//! Lives here because this crate already sees every other one (core, geom,
//! and svg as a dev-dependency) and owns the fixtures.
//!
//! `cargo bench -p vextor-render --bench big`

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use vextor_core::{Command, History, Selection, Transform, Vec2, View, from_bytes, to_bytes};
use vextor_geom::{Fonts, HitIndex, SnapTargets, pick, selectable};
use vextor_render::fixtures::big;
use vextor_render::{Pixmap, Viewport, render_into};
use vextor_svg::{Format, Scope, export, import_svg};

const NODES: usize = 20_000;

fn bench_file(c: &mut Criterion) {
    let doc = big::nodes(NODES);
    let bytes = to_bytes(&doc).expect("encodes");
    let mut g = c.benchmark_group("file 20k");
    g.sample_size(20);
    g.bench_function("save (encode)", |b| b.iter(|| to_bytes(black_box(&doc))));
    g.bench_function("open (decode + validate)", |b| {
        b.iter(|| from_bytes(black_box(&bytes)));
    });
    g.bench_function("snapshot (clone)", |b| b.iter(|| black_box(&doc).clone()));
    let images = big::images(200, 20);
    g.bench_function("snapshot 200 images (clone)", |b| {
        b.iter(|| black_box(&images).clone());
    });
    g.finish();
}

fn bench_frame(c: &mut Criterion) {
    let doc = big::nodes(NODES);
    let fonts = Fonts::default();
    let mut pixmap = Pixmap::new(2560, 1600).expect("non-empty size");
    let fit = Viewport {
        view: View {
            zoom: 0.4,
            pan: Vec2::new(0.0, 0.0),
        },
        width: 2560,
        height: 1600,
        pixels_per_point: 2.0,
    };
    let mut g = c.benchmark_group("frame 20k");
    g.sample_size(10);
    g.bench_function("full render, fit", |b| {
        b.iter(|| render_into(black_box(&doc), &fonts, &fit, &mut pixmap));
    });
    g.finish();
}

fn bench_pointer(c: &mut Criterion) {
    let doc = big::nodes(NODES);
    let fonts = Fonts::default();
    let mut g = c.benchmark_group("pointer 20k");
    g.bench_function("hit-test (pick, miss)", |b| {
        b.iter(|| {
            pick(
                black_box(&doc),
                &fonts,
                black_box(kurbo::Point::new(-5.0, -5.0)),
                1.0,
            )
        });
    });
    g.bench_function("hit-test (pick, last shape)", |b| {
        b.iter(|| {
            pick(
                black_box(&doc),
                &fonts,
                black_box(kurbo::Point::new(20.0, 20.0)),
                1.0,
            )
        });
    });
    g.bench_function("hit index (build once per change)", |b| {
        b.iter(|| HitIndex::build(black_box(&doc), &fonts));
    });
    let index = HitIndex::build(&doc, &fonts);
    g.bench_function("hit-test indexed (pick, miss)", |b| {
        b.iter(|| index.pick(black_box(kurbo::Point::new(-5.0, -5.0)), 1.0));
    });
    g.bench_function("hit-test indexed (pick, last shape)", |b| {
        b.iter(|| index.pick(black_box(kurbo::Point::new(20.0, 20.0)), 1.0));
    });
    g.bench_function("marquee indexed (200×200)", |b| {
        b.iter(|| index.touching(black_box(kurbo::Rect::new(100.0, 100.0, 300.0, 300.0))));
    });
    g.bench_function("snap targets (collect at press)", |b| {
        b.iter(|| SnapTargets::collect(black_box(&doc), &fonts, &[]));
    });
    let targets = SnapTargets::collect(&doc, &fonts, &[]);
    g.bench_function("snap one point", |b| {
        b.iter(|| targets.point(black_box(kurbo::Point::new(503.0, 497.0)), 4.0));
    });
    g.bench_function("select all", |b| b.iter(|| selectable(black_box(&doc))));
    g.finish();
}

fn bench_history(c: &mut Criterion) {
    let doc = big::nodes(NODES);
    let id = *doc.nodes.keys().nth(10).expect("the fixture has nodes");
    let mut g = c.benchmark_group("history 20k");
    g.bench_function("apply + undo + redo one move", |b| {
        b.iter_batched(
            || (doc.clone(), History::new()),
            |(mut doc, mut history)| {
                history
                    .apply(
                        &mut doc,
                        Command::SetTransform {
                            ids: vec![id],
                            transform: Transform([1.0, 0.0, 0.0, 1.0, 3.0, 4.0]),
                        },
                        &Selection::default(),
                    )
                    .expect("moves");
                history.undo(&mut doc);
                history.redo(&mut doc);
                doc
            },
            BatchSize::LargeInput,
        );
    });
    g.finish();
}

fn bench_svg(c: &mut Criterion) {
    let doc = big::nodes(NODES);
    let fonts = Fonts::default();
    let artboard = doc.artboards[0].id;
    let svg = export(
        &doc,
        &fonts,
        Scope::Artboard(artboard),
        Format::Svg,
        &doc.export,
    )
    .expect("exports")
    .code;
    let mut g = c.benchmark_group("svg 20k");
    g.sample_size(10);
    g.bench_function("export", |b| {
        b.iter(|| {
            export(
                black_box(&doc),
                &fonts,
                Scope::Artboard(artboard),
                Format::Svg,
                &doc.export,
            )
        });
    });
    g.bench_function("import", |b| {
        b.iter(|| import_svg(black_box(svg.as_bytes()), &fonts));
    });
    g.finish();
}

criterion_group!(
    benches,
    bench_file,
    bench_frame,
    bench_pointer,
    bench_history,
    bench_svg
);
criterion_main!(benches);
