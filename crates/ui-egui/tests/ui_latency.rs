//! How long the real application takes to answer input, on the synthetic drawing of
//! `tools/quality/synthetic.rs`. Hardware dependent, so it is `#[ignore]`d in the normal run and
//! asserts nothing about speed (only that the work happened); it prints median / p95 / max.
//!
//! Run: `cargo test --release -p cadcraft-ui-egui --test ui_latency -- --ignored --nocapture`
//! Sizes: `AAD_LATENCY_N=10000,100000` (default).
//!
//! What is timed is one `Harness::run_steps(1)` after the event: the whole `logic` + `ui` pass the
//! application runs per frame (layout, hit-testing, display-list work), without GPU rasterisation
//! (the harness does not present frames). It is a CPU-side lower bound for input-to-response time.

#[path = "../../../tools/quality/synthetic.rs"]
mod synthetic;

use std::time::Instant;

use cadcraft_engine::Session;
use cadcraft_engine::geom::Vec2;
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::Harness;
use serde_json::json;

fn stats(name: &str, n: usize, mut ms: Vec<f64>) {
    ms.sort_by(|a, b| a.total_cmp(b));
    let q = |p: f64| ms[((ms.len() as f64 - 1.0) * p).round() as usize];
    eprintln!(
        "{n:>7} entities | {name:<34} | median {:8.3} ms | p95 {:8.3} ms | max {:8.3} ms | samples {}",
        q(0.5),
        q(0.95),
        ms[ms.len() - 1],
        ms.len()
    );
}

fn harness(n: usize) -> Harness<'static, CadApp> {
    let mut s = Session::empty();
    s.open_drawing(synthetic::drawing(n), "latency", None);
    let mut app = CadApp::new(s, Services::default());
    app.ui.in_window_menu = false;
    app.integrated_titlebar = false;
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 800.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.state_mut().canvas.zoom_pending = true;
    h.run_steps(6);
    h
}

fn timed(h: &mut Harness<'_, CadApp>) -> f64 {
    let t = Instant::now();
    h.run_steps(1);
    t.elapsed().as_secs_f64() * 1000.0
}

#[test]
#[ignore = "hardware dependent; run with --ignored --nocapture in release"]
fn input_to_frame_latency() {
    let sizes: Vec<usize> = std::env::var("AAD_LATENCY_N")
        .ok()
        .map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![10_000, 100_000]);
    for n in sizes {
        let t = Instant::now();
        let mut h = harness(n);
        eprintln!("{n:>7} entities | open + first frames (6 steps)        | {:8.1} ms total", t.elapsed().as_secs_f64() * 1000.0);
        let rect = h.state().canvas.rect.expect("canvas");
        let xf = h.state().canvas.xf.expect("transform");
        assert!(h.state().session.doc().unwrap().model.len() >= n / 2);

        // Idle frame (nothing changed).
        let v: Vec<f64> = (0..60).map(|_| timed(&mut h)).collect();
        stats("idle frame", n, v);
        eprintln!(
            "{n:>7} entities | canvas build_ms {:.3}, draw_ms {:.3}, gpu target: {}",
            h.state().canvas.build_ms,
            h.state().canvas.draw_ms,
            h.state().canvas.gpu.is_some()
        );

        // Pointer moving over the canvas (hover pick, snap, crosshair).
        let c = rect.center();
        let v: Vec<f64> = (0..120)
            .map(|i| {
                h.event(Event::PointerMoved(c + egui::vec2((i % 40) as f32 * 3.0, (i / 3 % 30) as f32 * 2.0)));
                timed(&mut h)
            })
            .collect();
        stats("mouse move over canvas", n, v);

        // Tool activation (LINE prompt), then cancelled.
        let v: Vec<f64> = (0..30)
            .map(|_| {
                let t = Instant::now();
                h.state_mut().start("line");
                h.run_steps(1);
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                h.state_mut().session.cancel();
                h.run_steps(1);
                ms
            })
            .collect();
        stats("tool activation (LINE)", n, v);

        // Click select: press and release on the canvas centre-ish points.
        let v: Vec<f64> = (0..40)
            .map(|i| {
                let p = xf.to_screen(Vec2::new(xf.center.x + i as f64, xf.center.y));
                h.event(Event::PointerMoved(p));
                h.run_steps(1);
                let mut ms = 0.0;
                for pressed in [true, false] {
                    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
                    ms += timed(&mut h);
                }
                h.state_mut().session.set_selection(Vec::new());
                h.state_mut().session.pending_window = None;
                ms
            })
            .collect();
        stats("click (press + release frames)", n, v);

        // Property update then the frame that shows it.
        let sel: Vec<String> = h.state().session.doc().unwrap().model.iter().take(1000).map(|e| e.handle.hex()).collect();
        let v: Vec<f64> = (0..10)
            .map(|i| {
                let t = Instant::now();
                h.state_mut().run("properties.set", json!({"handles": sel, "color": (i % 6) + 1})).unwrap();
                h.run_steps(1);
                t.elapsed().as_secs_f64() * 1000.0
            })
            .collect();
        stats("recolour 1000 + next frame", n, v);

        // Scroll-wheel zoom steps (rebuilds the display list when the zoom band changes).
        let v: Vec<f64> = (0..30)
            .map(|i| {
                h.event(Event::PointerMoved(c));
                h.event(Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, if i % 2 == 0 { 1.0 } else { -1.0 }),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                });
                timed(&mut h)
            })
            .collect();
        stats("wheel zoom step", n, v);
    }
}

/// Writes the synthetic drawing as DXF so the real application (GPU canvas) can open the same
/// content: `AAD_LATENCY_WRITE=path.dxf AAD_LATENCY_COUNT=100000 cargo test -p cadcraft-ui-egui
/// --test ui_latency write_synthetic_dxf -- --ignored`.
#[test]
#[ignore = "writes a file when AAD_LATENCY_WRITE is set"]
fn write_synthetic_dxf() {
    let Ok(path) = std::env::var("AAD_LATENCY_WRITE") else { return };
    let n: usize = std::env::var("AAD_LATENCY_COUNT").ok().and_then(|v| v.parse().ok()).unwrap_or(100_000);
    let bytes = cadcraft_io::write(&synthetic::drawing(n), "synthetic.dxf").expect("write");
    std::fs::write(path, bytes).expect("save");
}
