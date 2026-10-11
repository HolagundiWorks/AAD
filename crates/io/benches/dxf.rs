//! Reading and writing DXF at 10 000 and 50 000 entities: what opening and saving a drawing cost
//! (the application does both on the thread that draws the window).
//!
//! Run: `cargo bench -p cadcraft-io`.

#[path = "../../../tools/quality/synthetic.rs"]
mod synthetic;

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};

fn bench(c: &mut Criterion) {
    for n in [10_000usize, 50_000] {
        let d = synthetic::drawing(n);
        let bytes = cadcraft_io::write(&d, "bench.dxf").map(|b| b.len()).unwrap_or(0);
        let text = cadcraft_io::write(&d, "bench.dxf").unwrap_or_default();
        let mut g = c.benchmark_group(format!("dxf/{n}"));
        g.sample_size(10);
        g.throughput(Throughput::Bytes(bytes as u64));
        g.bench_function("write", |b| b.iter(|| cadcraft_io::write(black_box(&d), "bench.dxf").map(|v| v.len()).unwrap_or(0)));
        g.bench_function("read", |b| b.iter(|| cadcraft_io::read(black_box(&text), "bench.dxf").map(|d| d.entity_count()).unwrap_or(0)));
        g.finish();
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
