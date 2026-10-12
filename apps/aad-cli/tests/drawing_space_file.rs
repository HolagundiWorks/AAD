//! Active Drawing Space configurations are saved in the DXF and come back after reopening, and the
//! locks they set protect objects in the reopened drawing.

use std::sync::Once;

use aad_engine::Session;
use serde_json::{Value, json};

fn install_io() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        aad_engine::cmd::file::set_io(aad_engine::cmd::file::IoHooks {
            read: |b, name| aad_io::read(b, name).map_err(|e| e.to_string()),
            write: |d, name| aad_io::write(d, name).map_err(|e| e.to_string()),
            plot: None,
        });
    });
}

fn temp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("aad-space-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn flags(s: &Session, layer: &str) -> (bool, bool, bool) {
    let l = s.doc().unwrap().layer(layer).unwrap();
    (l.on, l.frozen, l.locked)
}

#[test]
fn spaces_survive_save_and_reopen_and_still_protect_locked_layers() {
    install_io();
    let dir = temp("roundtrip");
    let path = dir.join("plan.dxf");
    let mut s = Session::new();
    for l in ["A-WALL", "S-COLUMN", "A-FURN"] {
        s.execute("layer.new", &json!({"name": l})).unwrap();
        s.execute("layer.current", &json!({"name": l})).unwrap();
        s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    }
    s.execute("layer.current", &json!({"name": "A-WALL"})).unwrap();
    s.execute(
        "space.save",
        &json!({"name": "Working Drawings", "description": "walls editable", "floor": "First", "current": "A-WALL", "unlisted": "lock", "layers": [
            {"layer": "A-WALL"}, {"layer": "S-COLUMN", "locked": true}, {"layer": "A-FURN", "visible": false, "locked": true}]}),
    )
    .unwrap();
    s.execute("space.save", &json!({"name": "Furniture", "activate": false, "layers": [{"layer": "A-WALL", "locked": true}, {"layer": "A-FURN"}]}))
        .unwrap();
    s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    let saved_flags: Vec<_> = ["A-WALL", "S-COLUMN", "A-FURN"].iter().map(|l| flags(&s, l)).collect();
    s.execute("saveas", &json!({"path": path.to_str().unwrap()})).unwrap();

    // A new session reads the file.
    let mut r = Session::new();
    r.execute("open", &json!({"path": path.to_str().unwrap()})).unwrap();
    let list = r.execute("space.list", &json!({})).unwrap();
    let names: Vec<&str> = list["spaces"].as_array().unwrap().iter().map(|x| x["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Working Drawings", "Furniture"]);
    assert_eq!(list["spaces"][0]["description"], "walls editable");
    assert_eq!(list["spaces"][0]["floor"], "First");
    assert_eq!(list["spaces"][0]["unlisted"], "lock");
    assert_eq!(list["spaces"][0]["active"], true, "the active space is remembered");
    assert_eq!(list["dirty"], false, "the saved layer states are the layer states of the file");
    let got: Vec<_> = ["A-WALL", "S-COLUMN", "A-FURN"].iter().map(|l| flags(&r, l)).collect();
    assert_eq!(got, saved_flags, "layer flags came back with the file");

    // The reopened space still switches, and its locks protect.
    r.execute("space.activate", &json!({"name": "Furniture"})).unwrap();
    assert_eq!(flags(&r, "A-WALL"), (true, false, true));
    let wall = r.doc().unwrap().model.iter().find(|e| e.common.layer == "A-WALL").unwrap().handle.hex();
    let before = format!("{:?}", r.doc().unwrap().model.iter().find(|e| e.common.layer == "A-WALL"));
    let _ = r.execute("move", &json!({"handles": [wall], "delta": [5, 5]}));
    assert_eq!(format!("{:?}", r.doc().unwrap().model.iter().find(|e| e.common.layer == "A-WALL")), before, "locked by the space: not moved");
    r.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    assert_eq!(flags(&r, "A-WALL"), (true, false, false));
    let e: Value = r.execute("space.get", &json!({"name": "Working Drawings"})).unwrap();
    assert_eq!(e["entries"][1]["editable"], false, "S-COLUMN is locked, so not editable");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_drawing_without_spaces_opens_and_works_unchanged() {
    install_io();
    let dir = temp("plain");
    let path = dir.join("plain.dxf");
    let mut s = Session::new();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    s.execute("saveas", &json!({"path": path.to_str().unwrap()})).unwrap();
    let mut r = Session::new();
    r.execute("open", &json!({"path": path.to_str().unwrap()})).unwrap();
    assert_eq!(r.execute("space.list", &json!({})).unwrap()["spaces"], json!([]));
    assert_eq!(r.execute("space.status", &json!({})).unwrap()["active"], Value::Null);
    let _ = std::fs::remove_dir_all(&dir);
}
