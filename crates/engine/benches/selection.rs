//! Selection and snapping on drawings of 10 000, 100 000 and 1 000 000 entities: picking one
//! object, window and crossing selection, fence selection and object snaps. These are the
//! operations that must stay instant while the pointer moves.
//!
//! Run: `cargo bench -p cadcraft-engine --bench selection` (the 1M case is skipped unless
//! `AAD_BENCH_LARGE=1`, because building it takes a few seconds per run).

#[path = "../../../tools/quality/synthetic.rs"]
mod synthetic;

use std::hint::black_box;

use cadcraft_doc::Space;
use cadcraft_engine::geom::{Bounds2, Vec2};
use cadcraft_engine::{select, snap};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

fn sizes() -> Vec<usize> {
    let mut v = vec![10_000, 100_000];
    if std::env::var("AAD_BENCH_LARGE").is_ok_and(|x| x == "1") {
        v.push(1_000_000);
    }
    v
}

fn queries(side: f64, n: usize) -> Vec<Vec2> {
    let mut r = synthetic::Rng(42);
    (0..n).map(|_| Vec2::new(r.range(0.0, side), r.range(0.0, side))).collect()
}

fn bench(c: &mut Criterion) {
    let space = Space::Model;
    for n in sizes() {
        let d = synthetic::drawing(n);
        let side = synthetic::side(n);
        let pts = queries(side, 200);
        let mut g = c.benchmark_group(format!("selection/{n}"));
        g.sample_size(20);

        // The first query builds the spatial index; later ones reuse it.
        g.bench_function("first pick (builds the index)", |b| {
            b.iter_batched(
                || {
                    // A clone shares chunks, and the cache holds the original: edit one entity so this
                    // is a drawing the cache has not seen.
                    let mut e = d.clone();
                    if let Some(h) = e.model.last().map(|x| x.handle) {
                        e.model.modify(h, |x| x.common.layer = "0".into());
                    }
                    e
                },
                |e| select::pick(&e, &space, pts[0], 0.1),
                BatchSize::LargeInput,
            )
        });
        let _ = select::pick(&d, &space, pts[0], 0.1);
        g.bench_function("pick x200 (index warm)", |b| {
            b.iter(|| pts.iter().filter(|p| select::pick(&d, &space, **p, black_box(0.1)).is_some()).count())
        });
        g.bench_function("window select 10% side", |b| {
            let half = side * 0.05;
            b.iter(|| select::select_window(&d, &space, Bounds2::new(pts[1] - Vec2::new(half, half), pts[1] + Vec2::new(half, half)), false).len())
        });
        g.bench_function("crossing select 10% side", |b| {
            let half = side * 0.05;
            b.iter(|| select::select_window(&d, &space, Bounds2::new(pts[2] - Vec2::new(half, half), pts[2] + Vec2::new(half, half)), true).len())
        });
        g.bench_function("fence select", |b| b.iter(|| select::select_fence(&d, &space, black_box(&pts[3..7])).len()));
        let modes = snap::mode::END | snap::mode::MID | snap::mode::CEN | snap::mode::QUA | snap::mode::INT;
        g.bench_function("osnap x200", |b| b.iter(|| pts.iter().filter(|p| snap::osnap(&d, &space, **p, 0.2, modes, None, false).is_some()).count()));
        g.finish();
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
