//! SVG export of a 1 000-node document (`performance`: ≤ 20 ms).

use criterion::{Criterion, criterion_group, criterion_main};
use vextor_geom::Fonts;
use vextor_render::fixtures;
use vextor_svg::{Format, Scope, export};

fn bench(c: &mut Criterion) {
    let doc = fixtures::many_nodes(1_000);
    let fonts = Fonts::default();
    let artboard = doc.artboards[0].id;
    for (name, format) in [
        ("svg_1000_nodes", Format::Svg),
        ("jsx_1000_nodes", Format::Jsx),
    ] {
        c.bench_function(name, |b| {
            b.iter(|| export(&doc, &fonts, Scope::Artboard(artboard), format, &doc.export));
        });
    }
    let mut paints = fixtures::paints();
    paints.export.styling = vextor_core::Styling::StyleElement;
    let artboard = paints.artboards[0].id;
    c.bench_function("paints_style_element", |b| {
        b.iter(|| {
            export(
                &paints,
                &fonts,
                Scope::Artboard(artboard),
                Format::Svg,
                &paints.export,
            )
        });
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
