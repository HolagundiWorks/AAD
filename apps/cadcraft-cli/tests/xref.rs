//! External references with real DXF files: attach, reload, move the project, bind, detach.

use std::path::{Path, PathBuf};
use std::sync::Once;

use cadcraft_engine::Session;
use serde_json::{Value, json};

fn install_io() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
            read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
            write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
            plot: None,
        });
    });
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("aad-xref-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn session(units: &str) -> Session {
    install_io();
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": units, "mode": "metadata"})).unwrap();
    s
}

/// A floor drawing with a wall line and a circle, saved at `path`.
fn floor_file(path: &Path, units: &str, wall: f64) {
    let mut s = session(units);
    s.execute("hcwlayers", &json!({"set": "hcw"})).unwrap();
    s.execute("layer.current", &json!({"name": "A-WALL"})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [wall, 0]]})).unwrap();
    s.execute("saveas", &json!({"path": path.to_str().unwrap()})).unwrap();
}

fn count(s: &Session, name: &str) -> usize {
    s.doc().unwrap().block(name).map_or(0, |b| b.entities.len())
}

fn inserts(s: &Session, name: &str) -> usize {
    s.doc().unwrap().model.iter().filter(|e| matches!(&e.kind, cadcraft_engine::doc::EntityKind::Insert(i) if i.block == name)).count()
}

#[test]
fn attach_reload_and_the_layers_of_a_reference() {
    let dir = temp("attach");
    let ground = dir.join("Floors").join("Ground.dxf");
    std::fs::create_dir_all(ground.parent().unwrap()).unwrap();
    floor_file(&ground, "mm", 9000.0);

    let mut host = session("mm");
    host.execute("saveas", &json!({"path": dir.join("Main.dxf").to_str().unwrap()})).unwrap();
    let r = host.execute("xref.attach", &json!({"path": ground.to_str().unwrap(), "name": "Ground"})).unwrap();
    assert_eq!(
        (r["name"].as_str(), r["entities"].as_u64(), r["path"].as_str()),
        (Some("Ground"), Some(1), Some("Floors/Ground.dxf")),
        "the path is stored relative to the drawing"
    );
    assert_eq!(inserts(&host, "Ground"), 1);
    assert!(host.doc().unwrap().layer("Ground|A-WALL").is_some(), "the layers come in prefixed");
    assert_eq!(host.doc().unwrap().block("Ground").unwrap().xref_path.as_deref(), Some("Floors/Ground.dxf"));

    // Layer flags set here survive a reload; new content from the source arrives.
    host.execute("layer.freeze", &json!({"names": ["Ground|A-WALL"]})).ok();
    host.doc_mut().unwrap().layer_mut("Ground|A-WALL").unwrap().locked = true;
    let mut src = session("mm");
    src.execute("open", &json!({"path": ground.to_str().unwrap()})).unwrap();
    src.execute("circle", &json!({"center": [500, 500], "radius": 100})).unwrap();
    src.execute("qsave", &json!({})).unwrap();
    let r = host.execute("xref.reload", &json!({})).unwrap();
    assert_eq!(r["reloaded"].as_array().unwrap().len(), 1);
    assert_eq!(count(&host, "Ground"), 2, "the circle drawn in the source file shows up");
    assert!(host.doc().unwrap().layer("Ground|A-WALL").unwrap().locked, "the lock was kept");

    // The list.
    let l = host.execute("xref.list", &json!({})).unwrap();
    assert_eq!((l["xrefs"][0]["status"].as_str(), l["xrefs"][0]["references"].as_u64()), (Some("found"), Some(1)));

    // Refusals.
    assert!(host.execute("xref.attach", &json!({"path": ground.to_str().unwrap(), "name": "ground"})).is_err(), "the name is taken");
    assert!(host.execute("xref.attach", &json!({"path": dir.join("nope.dxf").to_str().unwrap()})).is_err());
    assert!(host.execute("xref.attach", &json!({"path": ground.to_str().unwrap(), "name": "G2", "scale": -1})).is_err());
    assert!(host.execute("xref.reload", &json!({"name": "Nothing"})).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_moved_project_keeps_its_references_and_a_missing_file_keeps_the_last_content() {
    let dir = temp("move");
    let floors = dir.join("Floors");
    std::fs::create_dir_all(&floors).unwrap();
    floor_file(&floors.join("Ground.dxf"), "mm", 9000.0);
    let mut host = session("mm");
    host.execute("saveas", &json!({"path": dir.join("Main.dxf").to_str().unwrap()})).unwrap();
    host.execute("xref.attach", &json!({"path": floors.join("Ground.dxf").to_str().unwrap(), "name": "Ground"})).unwrap();
    host.execute("qsave", &json!({})).unwrap();

    // Move the whole project folder.
    let moved = dir.with_file_name(format!("{}-moved", dir.file_name().unwrap().to_string_lossy()));
    std::fs::remove_dir_all(&moved).ok();
    std::fs::rename(&dir, &moved).unwrap();
    let mut again = session("mm");
    again.execute("open", &json!({"path": moved.join("Main.dxf").to_str().unwrap()})).unwrap();
    let l = again.execute("xref.list", &json!({})).unwrap();
    assert_eq!(l["xrefs"][0]["status"], "found", "the relative path still resolves: {l}");
    let r = again.execute("xref.reload", &json!({})).unwrap();
    assert_eq!(r["reloaded"].as_array().unwrap().len(), 1);
    assert_eq!(count(&again, "Ground"), 1);

    // The source goes missing: reload reports it and keeps what was loaded.
    std::fs::remove_file(moved.join("Floors").join("Ground.dxf")).unwrap();
    let r = again.execute("xref.reload", &json!({})).unwrap();
    assert_eq!((r["missing"].as_array().unwrap().len(), r["reloaded"].as_array().unwrap().len()), (1, 0));
    assert_eq!(count(&again, "Ground"), 1, "the last content stays");
    assert_eq!(again.execute("xref.list", &json!({})).unwrap()["xrefs"][0]["status"], "missing");
    std::fs::remove_dir_all(&moved).ok();
}

#[test]
fn units_are_reconciled_by_the_insert_scale_and_overlays_are_never_saved() {
    let dir = temp("units");
    let metres = dir.join("InMetres.dxf");
    floor_file(&metres, "m", 9.0);
    let mut host = session("mm");
    let r = host.execute("xref.attach", &json!({"path": metres.to_str().unwrap(), "name": "M"})).unwrap();
    assert_eq!(r["scale"], 1000.0, "a metre drawing in a millimetre drawing");
    let e = host
        .doc()
        .unwrap()
        .model
        .iter()
        .find_map(|e| if let cadcraft_engine::doc::EntityKind::Insert(i) = &e.kind { Some(i.scale.x) } else { None })
        .unwrap();
    assert_eq!(e, 1000.0);

    // An overlay is there to look at, and is not part of the saved file.
    let r = host.execute("xref.attach", &json!({"path": metres.to_str().unwrap(), "name": "Other", "overlay": true})).unwrap();
    assert_eq!(r["name"], "OVL|Other");
    let out = dir.join("Host.dxf");
    host.execute("saveas", &json!({"path": out.to_str().unwrap()})).unwrap();
    let mut back = session("mm");
    back.execute("open", &json!({"path": out.to_str().unwrap()})).unwrap();
    assert!(back.doc().unwrap().block("M").is_some(), "the real xref was saved");
    assert!(back.doc().unwrap().blocks.keys().all(|k| !k.starts_with("OVL|")), "{:?}", back.doc().unwrap().blocks.keys().collect::<Vec<_>>());
    assert!(back.doc().unwrap().layers.iter().all(|l| !l.name.starts_with("OVL|")));
    assert!(host.doc().unwrap().block("OVL|Other").is_some(), "the session still has its overlay");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn bind_makes_it_a_block_and_detach_removes_it() {
    let dir = temp("bind");
    let f = dir.join("Ground.dxf");
    floor_file(&f, "mm", 5000.0);
    let mut host = session("mm");
    host.execute("xref.attach", &json!({"path": f.to_str().unwrap(), "name": "Ground"})).unwrap();
    let r = host.execute("xref.bind", &json!({"name": "Ground"})).unwrap();
    assert!(r["layers"].as_u64().unwrap() >= 1);
    let b = host.doc().unwrap().block("Ground").unwrap().clone();
    assert!(b.xref_path.is_none(), "no longer linked");
    assert!(host.doc().unwrap().layer("Ground$0$A-WALL").is_some() && host.doc().unwrap().layer("Ground|A-WALL").is_none());
    assert!(b.entities.iter().all(|e| e.common.layer.starts_with("Ground$0$")));
    assert_eq!(host.execute("xref.list", &json!({})).unwrap()["xrefs"].as_array().unwrap().len(), 0);
    assert!(host.execute("xref.bind", &json!({"name": "Ground"})).is_err(), "it is not an xref any more");

    host.execute("xref.attach", &json!({"path": f.to_str().unwrap(), "name": "Second"})).unwrap();
    let r = host.execute("xref.detach", &json!({"name": "Second"})).unwrap();
    assert_eq!(r["removed"], 1);
    assert!(host.doc().unwrap().block("Second").is_none() && host.doc().unwrap().layer("Second|A-WALL").is_none());
    assert_eq!(inserts(&host, "Second"), 0);
    assert!(host.execute("xref.detach", &json!({"name": "Second"})).is_err());
    let o: Value = host.execute("xref.attach", &json!({"path": f.to_str().unwrap(), "name": "Ov", "overlay": true})).unwrap();
    assert!(host.execute("xref.bind", &json!({"name": o["name"].as_str().unwrap()})).is_err(), "an overlay cannot be bound");
    std::fs::remove_dir_all(&dir).ok();
}
