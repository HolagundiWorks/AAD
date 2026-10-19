//! The parametric wall slice: graph → preview → commit (one undo step) → stale detection →
//! take-off facts, and the graph surviving a DXF save.

use aad_doc::{EntityKind, Handle, Line};
use aad_engine::Session;
use aad_geom::Vec3;
use serde_json::{Value, json};

fn metric() -> Session {
    let mut s = Session::empty();
    s.new_drawing(true);
    s
}

fn centre_line(s: &mut Session, x1: f64) -> Handle {
    s.add_entity(EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(x1, 0.0, 0.0) })).unwrap()
}

fn wall_lines(s: &Session) -> Vec<(Handle, [f64; 4])> {
    s.doc()
        .unwrap()
        .model
        .iter()
        .filter(|e| e.common.layer == "A-WALL")
        .filter_map(|e| match &e.kind {
            EntityKind::Line(l) => Some((e.handle, [l.a.x, l.a.y, l.b.x, l.b.y])),
            _ => None,
        })
        .collect()
}

fn state_of(s: &mut Session, id: &str) -> Value {
    let st = s.execute("param.status", &json!({})).unwrap();
    st["walls"].as_array().unwrap().iter().find(|w| w["instanceId"] == id).cloned().unwrap_or(Value::Null)
}

#[test]
fn wall_preview_commit_undo_and_takeoff() {
    let mut s = metric();
    let c = centre_line(&mut s, 5000.0);
    let r = s.execute("param.wall.new", &json!({ "handle": c.hex(), "height": "3 m", "thickness": 230 })).unwrap();
    assert_eq!(r["instanceId"], "wall-1");
    assert_eq!(r["unitsAssumed"], false);

    // Preview changes nothing in the drawing.
    let undo_before = s.state().unwrap().undo.len();
    let ev = s.execute("param.evaluate", &json!({})).unwrap();
    assert_eq!(ev["evaluation"]["status"], "ok", "{ev}");
    let staged = ev["staged"][0]["lines"].as_array().unwrap();
    assert_eq!(staged.len(), 4, "two faces and two end caps");
    assert!(wall_lines(&s).is_empty());
    assert_eq!(s.state().unwrap().undo.len(), undo_before);
    let m = &ev["evaluation"]["nodes"]["wall-1.measure"]["outputs"];
    assert!((m["grossFaceArea"]["value"]["value"].as_f64().unwrap() - 15.0).abs() < 1e-9, "{m}");

    // Commit: one undo step, faces 115 mm either side of the centre line.
    let rev = ev["revision"].as_u64().unwrap();
    let r = s.execute("param.commit", &json!({ "graphRevision": rev, "drawingRevision": ev["drawingRevision"] })).unwrap();
    assert_eq!(r["committed"], json!(["wall-1"]));
    assert_eq!(r["created"], 4);
    let lines = wall_lines(&s);
    assert_eq!(lines.len(), 4);
    assert!(lines.iter().any(|(_, l)| (l[1] - 115.0).abs() < 1e-9 && (l[3] - 115.0).abs() < 1e-9));
    assert!(lines.iter().any(|(_, l)| (l[1] + 115.0).abs() < 1e-9));
    assert_eq!(s.state().unwrap().undo.len(), undo_before + 1);
    assert_eq!(state_of(&mut s, "wall-1")["state"], "linked");

    // Take-off facts: the contract's wall row, with provenance.
    let t = s.execute("param.takeoff", &json!({})).unwrap();
    let row = &t["walls"][0]["row"];
    assert_eq!(row["ref"], "PW-wall-1");
    assert_eq!(row["length_mm"], 5000.0);
    assert_eq!(row["height_mm"], 3000.0);
    assert_eq!(row["thickness_mm"], 230.0);
    assert_eq!(row["layer"], "A-WALL");
    assert_eq!(t["walls"][0]["provenance"]["componentType"], "osas.wall");
    assert!(row.get("plaster").is_none() && row.get("cost").is_none());

    // Undo removes the lines and the link together; redo brings both back.
    s.undo().unwrap();
    assert!(wall_lines(&s).is_empty());
    assert!(s.execute("param.status", &json!({})).unwrap()["walls"].as_array().unwrap().is_empty());
    s.redo().unwrap();
    assert_eq!(wall_lines(&s).len(), 4);
    assert_eq!(state_of(&mut s, "wall-1")["state"], "linked");

    // The AQC bridge (and so `takeoff.record`) carries the same row.
    let b = s.execute("bridge.export", &json!({})).unwrap();
    let file: Value = serde_json::from_str(b["json"].as_str().unwrap()).unwrap();
    let w = file["walls"].as_array().unwrap().iter().find(|w| w["ref"] == "PW-wall-1").cloned().unwrap();
    assert_eq!(w["thickness_mm"], 230.0);
}

#[test]
fn changing_a_value_makes_the_wall_stale_until_committed_again() {
    let mut s = metric();
    let c = centre_line(&mut s, 4000.0);
    s.execute("param.wall.new", &json!({ "handle": c.hex() })).unwrap();
    s.execute("param.commit", &json!({})).unwrap();
    let first: Vec<Handle> = wall_lines(&s).iter().map(|(h, _)| *h).collect();

    // A preview does not store the change.
    let p = s.execute("param.set", &json!({ "node": "wall-1.height", "value": 3.5, "preview": true })).unwrap();
    assert_eq!(p["evaluation"]["nodes"]["wall-1.wall"]["outputs"]["height"]["value"]["value"], 3.5);
    assert_eq!(state_of(&mut s, "wall-1")["state"], "linked");

    s.execute("param.set", &json!({ "node": "wall-1.height", "value": 3.5 })).unwrap();
    let st = state_of(&mut s, "wall-1");
    assert_eq!(st["state"], "stale");
    let t = s.execute("param.takeoff", &json!({})).unwrap();
    assert!(t["walls"].as_array().unwrap().is_empty(), "a stale wall is not recorded");
    assert!(t["warnings"][0].as_str().unwrap().contains("stale"));

    // Height changes no geometry: the same lines are updated in place.
    let r = s.execute("param.commit", &json!({})).unwrap();
    assert_eq!(r["updated"], 4);
    let again: Vec<Handle> = wall_lines(&s).iter().map(|(h, _)| *h).collect();
    assert_eq!(first, again);
    let t = s.execute("param.takeoff", &json!({})).unwrap();
    assert_eq!(t["walls"][0]["row"]["height_mm"], 3500.0);

    // Moving the centre line it follows makes it stale; committing follows the line.
    s.doc_mut()
        .unwrap()
        .modify_entity(c, |e| {
            if let EntityKind::Line(l) = &mut e.kind {
                l.b = Vec3::new(6000.0, 0.0, 0.0);
            }
        })
        .unwrap();
    assert!(state_of(&mut s, "wall-1")["reasons"].to_string().contains("follows changed"));
    let ev = s.execute("param.evaluate", &json!({})).unwrap();
    assert_eq!(ev["sourcesChanged"], json!(["wall-1.path"]));
    s.execute("param.commit", &json!({})).unwrap();
    let t = s.execute("param.takeoff", &json!({})).unwrap();
    assert_eq!(t["walls"][0]["row"]["length_mm"], 6000.0);
    assert_eq!(state_of(&mut s, "wall-1")["state"], "linked");
}

#[test]
fn hand_edited_lines_are_not_overwritten_silently() {
    let mut s = metric();
    s.execute("param.wall.new", &json!({ "points": [[0, 0], [3000, 0], [3000, 2000]] })).unwrap();
    s.execute("param.commit", &json!({})).unwrap();
    let (h, _) = wall_lines(&s)[0];
    s.doc_mut()
        .unwrap()
        .modify_entity(h, |e| {
            if let EntityKind::Line(l) = &mut e.kind {
                l.a.x += 10.0;
            }
        })
        .unwrap();
    assert_eq!(state_of(&mut s, "wall-1")["state"], "stale");
    s.execute("param.set", &json!({ "node": "wall-1.thickness", "value": 300 })).unwrap();
    let before = wall_lines(&s);
    let err = s.execute("param.commit", &json!({})).unwrap_err().to_string();
    assert!(err.contains("edited"), "{err}");
    assert_eq!(wall_lines(&s), before, "a refused commit changes nothing");
    s.execute("param.commit", &json!({ "force": true })).unwrap();
    assert_eq!(s.execute("param.takeoff", &json!({})).unwrap()["walls"][0]["row"]["thickness_mm"], 300.0);

    // Detached: the lines stay as plain geometry and later commits leave them alone.
    s.execute("param.detach", &json!({ "instance": "wall-1" })).unwrap();
    s.execute("param.set", &json!({ "node": "wall-1.thickness", "value": 150 })).unwrap();
    let kept = wall_lines(&s);
    s.execute("param.commit", &json!({})).unwrap();
    assert_eq!(wall_lines(&s), kept);
    assert_eq!(state_of(&mut s, "wall-1")["state"], "detached");
    assert!(s.execute("param.takeoff", &json!({})).unwrap()["walls"].as_array().unwrap().is_empty());
}

#[test]
fn invalid_graphs_and_old_revisions_change_nothing() {
    let mut s = metric();
    s.execute("param.wall.new", &json!({ "points": [[0, 0], [2000, 0]] })).unwrap();
    let rev = s.execute("param.graph", &json!({})).unwrap()["revision"].as_u64().unwrap();
    let err = s.execute("param.commit", &json!({ "graphRevision": rev + 5 })).unwrap_err().to_string();
    assert!(err.contains("revision"), "{err}");
    let err = s.execute("param.commit", &json!({ "drawingRevision": 999 })).unwrap_err().to_string();
    assert!(err.contains("revision"), "{err}");
    assert!(wall_lines(&s).is_empty());

    // A cycle: two formulas feeding each other.
    let mut g = s.execute("param.graph", &json!({})).unwrap()["graph"].clone();
    let f = |id: &str| json!({ "nodeId": id, "nodeType": "math.formula", "nodeVersion": 1, "config": { "expr": "a", "inputs": [{ "name": "a", "dimension": "length" }] } });
    g["nodes"].as_array_mut().unwrap().extend([f("f1"), f("f2")]);
    let e = |id: &str, a: &str, b: &str| json!({ "edgeId": id, "sourceNodeId": a, "sourcePortId": "result", "targetNodeId": b, "targetPortId": "a" });
    g["edges"].as_array_mut().unwrap().extend([e("c1", "f1", "f2"), e("c2", "f2", "f1")]);
    let r = s.execute("param.graph", &json!({ "graph": g })).unwrap();
    assert!(r["diagnostics"].to_string().contains("cycle"), "{r}");
    let undo = s.state().unwrap().undo.len();
    let err = s.execute("param.commit", &json!({})).unwrap_err().to_string();
    assert!(err.contains("nothing was changed"), "{err}");
    assert!(wall_lines(&s).is_empty());
    assert_eq!(s.state().unwrap().undo.len(), undo);

    // Hostile parameters are refused, never a panic.
    for p in [
        json!({ "points": [[0, 0]] }),
        json!({ "points": [[0, 0], [1, 1]], "height": -3 }),
        json!({ "points": [[0, 0], [1, 1]], "thickness": "3 kg" }),
        json!({ "points": [[0, 0], [1, 1]], "id": "bad id!" }),
        json!({ "handle": "FFFFFF" }),
        json!({}),
    ] {
        assert!(s.execute("param.wall.new", &p).is_err(), "{p}");
    }
    assert!(s.execute("param.graph", &json!({ "graph": { "schemaVersion": 99 } })).is_err());
    assert!(s.execute("param.set", &json!({ "node": "nope", "value": 1 })).is_err());
    assert!(s.execute("param.deps", &json!({ "node": "nope" })).is_err());
}

#[test]
fn graph_survives_a_dxf_save_and_evaluates_after_reload() {
    let mut s = metric();
    let c = centre_line(&mut s, 5000.0);
    s.execute("param.wall.new", &json!({ "handle": c.hex(), "height": 2.8 })).unwrap();
    s.execute("param.commit", &json!({})).unwrap();
    let text = aad_io::write_dxf(s.doc().unwrap());
    let back = aad_io::read(text.as_bytes(), "wall.dxf").unwrap();

    let mut t = Session::empty();
    t.open_drawing(back, "wall", None);
    assert_eq!(state_of(&mut t, "wall-1")["state"], "linked");
    let ev = t.execute("param.evaluate", &json!({})).unwrap();
    assert_eq!(ev["evaluation"]["status"], "ok", "{ev}");
    assert_eq!(ev["evaluation"]["nodes"]["wall-1.wall"]["outputs"]["height"]["value"]["value"], 2.8);
    let deps = t.execute("param.deps", &json!({ "node": "wall-1.height" })).unwrap();
    assert_eq!(deps["downstream"], json!(["wall-1.measure", "wall-1.wall"]));
    let row = &t.execute("param.takeoff", &json!({})).unwrap()["walls"][0]["row"];
    assert_eq!(row["height_mm"], 2800.0);
}
