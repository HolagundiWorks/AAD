//! `library.publish` and `library.list`: the component library in its own ShilpiDB database
//! (`docs/component-library-proposal.md`).

use aad_engine::Session;
use serde_json::{Value, json};

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aad-library-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("library.vdb")
}

fn cleanup(path: &std::path::Path) {
    if let Some(dir) = path.parent() {
        std::fs::remove_dir_all(dir).ok();
    }
}

fn wall(revision: u64) -> Value {
    json!({ "kind": "component_type", "id": "osas.wall", "schema_version": revision, "spec_version": 1, "label": "Wall",
            "measurement_rule": "hcw-aqc-bridge/1#walls", "params": [ { "id": "height", "type": "length", "unit": "m", "default": 3.0 } ] })
}

fn material() -> Value {
    json!({ "kind": "material", "id": "brick.full", "revision": 1, "label": "Brick, full", "applies_to": ["osas.wall"] })
}

fn publish(s: &mut Session, path: &std::path::Path, defs: Vec<Value>) -> Value {
    s.execute("library.publish", &json!({ "path": path.to_string_lossy(), "definitions": defs })).unwrap()
}

#[test]
fn definitions_are_published_listed_and_immutable() {
    let mut s = Session::new();
    let path = temp("basic");
    let first = publish(&mut s, &path, vec![wall(1), material()]);
    assert_eq!(first["published"].as_array().unwrap().len(), 2, "{first}");
    assert_eq!(first["counts"]["component_type"], 1);

    let list = s.execute("library.list", &json!({ "path": path.to_string_lossy() })).unwrap();
    assert_eq!(list["state"], "complete");
    let defs = list["definitions"].as_array().unwrap();
    assert_eq!(defs.len(), 2);
    assert!(defs.iter().any(|d| d["kind"] == "component_type" && d["id"] == "osas.wall" && d["revision"] == 1));
    assert!(defs.iter().any(|d| d["kind"] == "material" && d["id"] == "brick.full" && d["label"] == "Brick, full"));
    let only = s.execute("library.list", &json!({ "path": path.to_string_lossy(), "kind": "material" })).unwrap();
    assert_eq!(only["definitions"].as_array().unwrap().len(), 1);

    // The same again: nothing changes. A different wall 1: refused, the original stays. Wall 2: a new record.
    let again = publish(&mut s, &path, vec![wall(1), material()]);
    assert_eq!((again["published"].as_array().unwrap().len(), again["unchanged"].as_array().unwrap().len()), (0, 2), "{again}");
    let mut changed = wall(1);
    changed["label"] = json!("Changed");
    let refused = publish(&mut s, &path, vec![changed, wall(2)]);
    assert_eq!(refused["conflicts"].as_array().unwrap().len(), 1, "{refused}");
    assert_eq!(refused["published"], json!(["osas.wall@2"]));
    let list = s.execute("library.list", &json!({ "path": path.to_string_lossy(), "kind": "component_type" })).unwrap();
    let revisions: Vec<u64> = list["definitions"].as_array().unwrap().iter().map(|d| d["revision"].as_u64().unwrap()).collect();
    assert_eq!(revisions, vec![1, 2]);
    assert!(list["definitions"].as_array().unwrap().iter().all(|d| d["label"] == "Wall"), "the original revision 1 is intact");
    cleanup(&path);
}

#[test]
fn other_databases_are_never_overwritten_and_hostile_input_is_an_error() {
    let mut s = Session::new();
    let dir = std::env::temp_dir().join(format!("aad-library-{}-other", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // An entity store and a take-off database are both refused, untouched.
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let entities = dir.join("entities.vdb");
    s.execute("vdb.export", &json!({ "path": entities.to_string_lossy() })).unwrap();
    let takeoff = dir.join("takeoff.vdb");
    s.execute("takeoff.record", &json!({ "path": takeoff.to_string_lossy(), "annotate": false })).unwrap();
    for other in [&entities, &takeoff] {
        let before = std::fs::read(other).unwrap();
        let err = s.execute("library.publish", &json!({ "path": other.to_string_lossy(), "definitions": [material()] })).unwrap_err().to_string();
        assert!(err.contains("not a component library database"), "{err}");
        assert_eq!(std::fs::read(other).unwrap(), before);
    }
    let path = dir.join("library.vdb");
    let p = path.to_string_lossy().to_string();
    for bad in [
        json!({ "path": p }),
        json!({ "path": p, "definitions": "x" }),
        json!({ "path": p, "definitions": [5] }),
        json!({ "path": p, "definitions": [{ "kind": "wall", "id": "a", "revision": 1 }] }),
        json!({ "path": p, "definitions": [{ "kind": "material", "id": "../x", "revision": 1 }] }),
        json!({ "path": p, "definitions": [{ "kind": "material", "id": "a", "revision": -1 }] }),
        json!({ "path": p, "definitions": [{ "kind": "material", "id": "a", "revision": 1e30 }] }),
    ] {
        assert!(s.execute("library.publish", &bad).is_err(), "{bad}");
    }
    assert!(!path.exists(), "nothing was written for a bad request");
    assert!(s.execute("library.list", &json!({ "path": p, "kind": "nope" })).is_err());
    // A server that is not there is an error.
    assert!(s.execute("library.publish", &json!({ "server": "127.0.0.1:1", "definitions": [material()] })).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

/// Against a real `shilpid`: `AAD_SHILPID_ADDR=127.0.0.1:7432 cargo test -p aad-engine --test library -- --ignored`
/// (start it with `shilpid --bind 127.0.0.1:7432 --data x.vdb`).
#[test]
#[ignore = "needs a running shilpid; see the doc comment"]
fn a_running_shilpid_holds_the_library() {
    let Ok(addr) = std::env::var("AAD_SHILPID_ADDR") else { return };
    let mut s = Session::new();
    let first = s.execute("library.publish", &json!({ "server": addr, "definitions": [wall(1), material()] })).unwrap();
    assert_eq!(first["published"].as_array().unwrap().len() + first["unchanged"].as_array().unwrap().len(), 2, "{first}");
    let again = s.execute("library.publish", &json!({ "server": addr, "definitions": [wall(1), material()] })).unwrap();
    assert_eq!(again["unchanged"].as_array().unwrap().len(), 2, "{again}");
    let list = s.execute("library.list", &json!({ "server": addr })).unwrap();
    assert_eq!(list["state"], "complete");
    assert_eq!(list["definitions"].as_array().unwrap().len(), 2);
}
