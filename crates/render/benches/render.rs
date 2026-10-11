//! Building the display list, which the canvas does whenever the drawing, the layout view or the
//! zoom band changes: model space at zoom extents, and a layout with a viewport (model space seen
//! through it, clipped), which is what every frame of panning inside a viewport costs.
//!
//! Run: `cargo bench -p cadcraft-render`.

#[path = "../../../tools/quality/synthetic.rs"]
mod synthetic;

use std::hint::black_box;

use cadcraft_doc::{Common, EntityKind, Space, Viewport};
use cadcraft_geom::{Vec2, Vec3};
use cadcraft_render::{Options, build};
use criterion::{Criterion, criterion_group, criterion_main};

fn options(side: f64) -> Options {
    let px = side / 1600.0;
    Options { tolerance: px * 0.5, min_dash: px * 2.0, ..Options::default() }
}

fn bench(c: &mut Criterion) {
    for n in [10_000usize, 100_000] {
        let side = synthetic::side(n);
        let mut d = synthetic::drawing(n);
        let mut g = c.benchmark_group(format!("render/{n}"));
        g.sample_size(15);
        let o = options(side);
        g.bench_function("model space, zoom extents", |b| b.iter(|| build(black_box(&d), &Space::Model, &o).prims.len()));

        // A layout whose one viewport shows a tenth of the drawing.
        let layout = d.layouts.first().map(|l| l.name.clone()).unwrap_or_else(|| "Layout1".into());
        let paper = Space::Paper(layout);
        let vp = |cx: f64| Viewport {
            center: Vec3::new(150.0, 100.0, 0.0),
            width: 280.0,
            height: 190.0,
            view_center: Vec2::new(cx, side / 2.0),
            view_height: side * 0.3,
            id: 2,
            locked: false,
            frozen_layers: Vec::new(),
            layer_colors: Vec::new(),
        };
        let h = d.add(&paper, Common::default(), EntityKind::Viewport(vp(side / 2.0))).ok();
        let po = Options { view_height: 200.0, ..options(side) };
        g.bench_function("layout with one viewport", |b| b.iter(|| build(black_box(&d), &paper, &po).prims.len()));
        // Panning inside the viewport: the view centre moves, so the content is built again.
        let mut step = 0.0;
        g.bench_function("one pan step inside the viewport", |b| {
            b.iter(|| {
                step += 1.0;
                if let Some(h) = h {
                    d.layouts.iter_mut().find(|l| Space::Paper(l.name.clone()) == paper).map(|l| {
                        l.entities.modify(h, |e| {
                            if let EntityKind::Viewport(v) = &mut e.kind {
                                v.view_center = Vec2::new(side / 2.0 + step, side / 2.0);
                            }
                        })
                    });
                }
                build(black_box(&d), &paper, &po).prims.len()
            })
        });
        g.finish();
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
