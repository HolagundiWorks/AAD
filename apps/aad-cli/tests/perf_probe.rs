//! A timing probe, not a pass/fail test: set AAD_PERF_FILE to a drawing and run
//! `cargo test --release -p aad-cli --test perf_probe -- --nocapture --ignored` to see where the
//! time goes when it is opened and edited. Ignored by default (it needs a file and a release build).

use std::time::Instant;

use aad_engine::Session;
use serde_json::json;

fn t<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let t0 = Instant::now();
    let r = f();
    println!("{label:<34} {:>8.1} ms", t0.elapsed().as_secs_f64() * 1000.0);
    r
}

#[test]
#[ignore]
fn where_the_time_goes() {
    let Ok(path) = std::env::var("AAD_PERF_FILE") else { return };
    aad_engine::cmd::file::set_io(aad_engine::cmd::file::IoHooks {
        read: |b, name| aad_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| aad_io::write(d, name).map_err(|e| e.to_string()),
        plot: Some(|d, space, opts| aad_io::plot(d, space, opts).map_err(|e| e.to_string())),
    });
    let bytes = t("read file bytes", || std::fs::read(&path).unwrap());
    let name = std::path::Path::new(&path).file_name().and_then(|n| n.to_str()).unwrap_or("x.dwg").to_string();
    let drawing = t("parse (aad_io::read)", || aad_io::read(&bytes, &name).unwrap());
    let mut s = Session::new();
    t("open_drawing", || s.open_drawing(drawing, &name, None));
    let n = s.doc().unwrap().model.len();
    println!("entities: {n}");
    let doc = s.state().unwrap().doc.clone();
    let space = s.space();
    let opts = aad_engine::render::Options {
        tolerance: 0.5,
        min_dash: 1.0,
        text: true,
        fill: true,
        lineweights: true,
        view_height: 0.0,
        plot_style_table: None,
    };
    let list = t("display list (parallel)", || aad_engine::render::build(&doc, &space, &opts));
    println!("primitives: {} verts: {} tris: {}", list.prims.len(), list.verts.len(), list.tris.len());
    t("spatial index (first pick)", || aad_engine::select::pick(&doc, &space, aad_engine::geom::Vec2::new(1.0, 1.0), 0.5));
    t("pick x100 (warm)", || {
        for i in 0..100 {
            let _ = aad_engine::select::pick(&doc, &space, aad_engine::geom::Vec2::new(f64::from(i), f64::from(i)), 0.5);
        }
    });
    t("select all", || s.execute("selectall", &json!({})).ok());
    t("erase selection", || s.execute("erase.selection", &json!({})).ok());
    t("undo", || s.execute("undo", &json!({})).ok());
    t("audit", || s.execute("audit", &json!({})).ok());
    t("zoom extents", || s.execute("zoom.extents", &json!({})).ok());
    let bytes = t("write DXF", || aad_io::write(s.doc().unwrap(), "x.dxf").unwrap());
    println!("dxf bytes: {}", bytes.len());
    t("re-read DXF", || aad_io::read(&bytes, "x.dxf").unwrap());
}

#[test]
#[ignore]
fn which_objects_make_the_triangles() {
    let Ok(path) = std::env::var("AAD_PERF_FILE") else { return };
    let bytes = std::fs::read(&path).unwrap();
    let drawing = aad_io::read(&bytes, "x.dwg").unwrap();
    let opts = aad_engine::render::Options {
        tolerance: 0.5,
        min_dash: 1.0,
        text: true,
        fill: true,
        lineweights: true,
        view_height: 0.0,
        plot_style_table: None,
    };
    let list = aad_engine::render::build(&drawing, &aad_engine::doc::Space::Model, &opts);
    let mut by: std::collections::BTreeMap<String, (usize, usize, usize)> = Default::default();
    for p in &list.prims {
        let kind = drawing.entity(p.handle).map_or("(block content)".to_string(), |e| e.kind.type_name().to_string());
        let e = by.entry(kind).or_default();
        e.0 += 1;
        match p.kind {
            aad_engine::render::Kind::Tris | aad_engine::render::Kind::Mask => e.2 += p.len as usize,
            _ => e.1 += p.len as usize,
        }
    }
    println!("{:<22} {:>9} {:>12} {:>12}", "entity", "prims", "line verts", "tri verts");
    for (k, (n, v, t)) in by {
        println!("{k:<22} {n:>9} {v:>12} {t:>12}");
    }
}

#[test]
#[ignore]
fn display_list_cost_by_feature() {
    let Ok(path) = std::env::var("AAD_PERF_FILE") else { return };
    let drawing = aad_io::read(&std::fs::read(&path).unwrap(), "x.dwg").unwrap();
    for (label, text, fill) in [("all", true, true), ("no text", false, true), ("no fill", true, false), ("neither", false, false)] {
        let opts =
            aad_engine::render::Options { tolerance: 0.5, min_dash: 1.0, text, fill, lineweights: true, view_height: 0.0, plot_style_table: None };
        let t0 = Instant::now();
        let list = aad_engine::render::build(&drawing, &aad_engine::doc::Space::Model, &opts);
        println!(
            "{label:<10} {:>8.0} ms  prims {:>8} lineverts {:>9} triverts {:>10}",
            t0.elapsed().as_secs_f64() * 1000.0,
            list.prims.len(),
            list.verts.len(),
            list.tris.len()
        );
    }
}
