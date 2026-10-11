//! Microbenchmarks of the geometry kernel: intersections, polygon tests, area, tessellation and
//! transforms, on batches of pseudo-random inputs (so one iteration is a batch, and the time per
//! operation is the reported time divided by the batch size, which is in the benchmark name).
//!
//! Run: `cargo bench -p cadcraft-geom`. Numbers are only comparable on the same machine and build.

use std::hint::black_box;

use cadcraft_geom::{Arc, Circle, Line, Mat3, PolyVertex, Polyline, Vec2, circle_circle, line_circle, line_line, point_in_polygon, shoelace};
use criterion::{Criterion, criterion_group, criterion_main};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    fn pt(&mut self, side: f64) -> Vec2 {
        Vec2::new(self.next() * side, self.next() * side)
    }
}

fn lines(n: usize) -> Vec<Line> {
    let mut r = Rng(1);
    (0..n).map(|_| Line::new(r.pt(1000.0), r.pt(1000.0))).collect()
}

fn circles(n: usize) -> Vec<Circle> {
    let mut r = Rng(2);
    (0..n).map(|_| Circle::new(r.pt(1000.0), 20.0 + r.next() * 200.0)).collect()
}

fn polygon(n: usize) -> Vec<Vec2> {
    // A star-ish closed polygon: radius varies with angle, so it is not convex.
    (0..n)
        .map(|i| {
            let a = i as f64 / n as f64 * std::f64::consts::TAU;
            let r = 500.0 + 200.0 * (a * 7.0).sin();
            Vec2::new(r * a.cos(), r * a.sin())
        })
        .collect()
}

fn bench(c: &mut Criterion) {
    let ls = lines(1000);
    let cs = circles(1000);
    c.bench_function("line_line x1000 pairs", |b| {
        b.iter(|| {
            let mut hits = 0;
            for w in ls.windows(2) {
                hits += usize::from(line_line(black_box(&w[0]), black_box(&w[1])).is_some());
            }
            hits
        })
    });
    c.bench_function("line_circle x1000", |b| {
        b.iter(|| ls.iter().zip(&cs).map(|(l, ci)| line_circle(black_box(l), black_box(ci)).len()).sum::<usize>())
    });
    c.bench_function("circle_circle x1000 pairs", |b| {
        b.iter(|| cs.windows(2).map(|w| circle_circle(black_box(&w[0]), black_box(&w[1])).len()).sum::<usize>())
    });
    c.bench_function("Line::closest x1000", |b| {
        let p = Vec2::new(500.0, 500.0);
        b.iter(|| ls.iter().map(|l| l.closest(black_box(p)).x).sum::<f64>())
    });

    for n in [100usize, 1_000, 10_000] {
        let poly = polygon(n);
        let inside = Vec2::new(10.0, 10.0);
        c.bench_function(&format!("point_in_polygon {n} vertices"), |b| b.iter(|| point_in_polygon(black_box(&poly), black_box(inside))));
        c.bench_function(&format!("shoelace {n} vertices"), |b| b.iter(|| shoelace(black_box(&poly))));
        let pl = Polyline {
            vertices: poly.iter().enumerate().map(|(i, p)| PolyVertex::with_bulge(*p, if i % 10 == 0 { 0.3 } else { 0.0 })).collect(),
            closed: true,
        };
        c.bench_function(&format!("Polyline::area with bulges {n} vertices"), |b| b.iter(|| black_box(&pl).area()));
        c.bench_function(&format!("Polyline::tessellate {n} vertices"), |b| b.iter(|| black_box(&pl).tessellate(0.1).len()));
    }

    let arc = Arc::new(Vec2::new(0.0, 0.0), 1000.0, 0.2, 5.0);
    c.bench_function("Arc::tessellate r=1000 tol 0.01", |b| {
        b.iter(|| {
            let mut out = Vec::new();
            black_box(&arc).tessellate(0.01, &mut out);
            out.len()
        })
    });
    let m = Mat3::translate(Vec2::new(5.0, 6.0)).then(Mat3::rotate(0.7)).then(Mat3::scale(2.0, 2.0));
    let pts: Vec<Vec2> = polygon(1000);
    c.bench_function("Mat3::apply x1000 points", |b| b.iter(|| pts.iter().map(|p| black_box(&m).apply(*p).x).sum::<f64>()));
}

criterion_group!(benches, bench);
criterion_main!(benches);
