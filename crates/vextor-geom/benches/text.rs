//! Text layout cost (spec 05c): typing re-lays out the edited text once
//! per keystroke; the budget is the 8 ms frame (`performance`).

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use vextor_core::Text;
use vextor_geom::{FontDb, layout};

const REGULAR: &[u8] = include_bytes!("../tests/fixtures/fonts/NotoSans-Regular.ttf");

fn bench(c: &mut Criterion) {
    let db = FontDb::from_data([REGULAR.to_vec()]);
    let text = |content: String| Text {
        size: 18.0,
        ..Text::new(content, "Noto Sans")
    };
    let one_line = text("Buy now ".repeat(125));
    let lines = text("Buy now, save 20%\n".repeat(55));
    let label = text("Buy now".to_owned());
    c.bench_function("layout, 1 000 chars, one line", |b| {
        b.iter(|| layout(black_box(&one_line), &db));
    });
    c.bench_function("layout, 1 000 chars, 55 lines", |b| {
        b.iter(|| layout(black_box(&lines), &db));
    });
    c.bench_function("layout, button label", |b| {
        b.iter(|| layout(black_box(&label), &db));
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
