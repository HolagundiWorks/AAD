//! Editing commands on a 100 000-entity drawing, through the same `Session::execute` the command
//! line, the menus and scripts use: adding an object, moving, copying, erasing and recolouring a
//! selection of 1 000, and undo. Each measured command is undone outside the timing so every
//! iteration starts from the same drawing.
//!
//! Run: `cargo bench -p cadcraft-engine --bench drawing_operations`.

#[path = "../../../tools/quality/synthetic.rs"]
mod synthetic;

use std::time::{Duration, Instant};

use cadcraft_doc::Space;
use cadcraft_engine::Session;
use cadcraft_engine::geom::{Bounds2, Vec2};
use cadcraft_engine::select;
use criterion::{Criterion, criterion_group, criterion_main};
use serde_json::{Value, json};

const N: usize = 100_000;

/// A session on the synthetic drawing and the handles of about 1 000 entities in a window.
fn setup() -> (Session, Vec<String>) {
    let d = synthetic::drawing(N);
    let side = synthetic::side(N);
    let c = Vec2::new(side / 2.0, side / 2.0);
    let half = side * 0.016;
    let hs: Vec<String> = select::select_window(&d, &Space::Model, Bounds2::new(c - Vec2::new(half, half), c + Vec2::new(half, half)), false)
        .iter()
        .take(1000)
        .map(|h| h.hex())
        .collect();
    let mut s = Session::empty();
    s.open_drawing(d, "bench", None);
    (s, hs)
}

/// Time `command` (then undo it, untimed) `iters` times.
fn timed(s: &mut Session, iters: u64, command: &str, params: &Value) -> Duration {
    let mut total = Duration::ZERO;
    for _ in 0..iters {
        let t = Instant::now();
        let r = s.execute(command, params);
        total += t.elapsed();
        assert!(r.is_ok(), "{command}: {r:?}");
        let _ = s.execute("undo", &json!({}));
    }
    total
}

fn bench(c: &mut Criterion) {
    let (mut s, hs) = setup();
    assert!(hs.len() >= 500, "the window selects {} entities", hs.len());
    let mut g = c.benchmark_group(format!("operations/{N}"));
    g.sample_size(20);

    g.bench_function("add one line", |b| b.iter_custom(|i| timed(&mut s, i, "line", &json!({"points": [[1, 1], [50, 50]]}))));
    g.bench_function("move ~1000 selected", |b| b.iter_custom(|i| timed(&mut s, i, "move", &json!({"handles": hs, "delta": [3.0, 4.0]}))));
    g.bench_function("copy ~1000 selected", |b| b.iter_custom(|i| timed(&mut s, i, "copy", &json!({"handles": hs, "delta": [30.0, 40.0]}))));
    g.bench_function("rotate ~1000 selected", |b| {
        b.iter_custom(|i| timed(&mut s, i, "rotate", &json!({"handles": hs, "base": [0, 0], "angle": 15})))
    });
    g.bench_function("erase ~1000 selected", |b| b.iter_custom(|i| timed(&mut s, i, "erase", &json!({"handles": hs}))));
    g.bench_function("recolour ~1000 selected", |b| b.iter_custom(|i| timed(&mut s, i, "properties.set", &json!({"handles": hs, "color": 3}))));
    g.bench_function("undo of moving ~1000", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let _ = s.execute("move", &json!({"handles": hs, "delta": [3.0, 4.0]}));
                let t = Instant::now();
                let _ = s.execute("undo", &json!({}));
                total += t.elapsed();
            }
            total
        })
    });
    g.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
