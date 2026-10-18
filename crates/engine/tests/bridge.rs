//! The AQC bridge: export of what the drawing knows, and a previewed, never-geometric import.

use aad_engine::Session;
use serde_json::{Value, json};

fn session() -> Session {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    s
}

fn export(s: &mut Session) -> (Value, Value) {
    let r = s.execute("bridge.export", &json!({})).unwrap();
    let file: Value = serde_json::from_str(r["json"].as_str().unwrap()).unwrap();
    (r, file)
}

#[test]
fn export_carries_project_floors_schedule_and_measured_items() {
    let mut s = session();
    s.execute(
        "project",
        &json!({"details": {"title": "Small house", "owner": "A. Kumar", "location": "Hospet", "siteArea": "120", "plotUse": "Residential"}}),
    )
    .unwrap();
    s.execute("floors", &json!({"floors": [{"name": "Ground", "floorToFloor": 3.15, "ceiling": 3.0, "lintel": 2.1, "slab": 150, "beam": 450}]}))
        .unwrap();
    s.execute("beams", &json!({"depths": [300, 450]})).unwrap();
    s.execute("openings.std", &json!({})).unwrap();
    s.execute("wall.draw", &json!({"points": [[0, 0], [9000, 0], [9000, 6000], [0, 6000], [0, 0]], "thickness": 230})).unwrap();
    s.execute("wall.door", &json!({"at": [2000, 0], "name": "D2", "width": 900})).unwrap();
    s.execute("wall.window", &json!({"at": [5000, 0], "name": "W3", "width": 1200})).unwrap();
    s.execute("takeoff.start", &json!({})).unwrap();
    s.execute("layer.current", &json!({"name": "MEASURE-FULLBRICK"})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [9000, 0]]})).unwrap();
    s.execute("line", &json!({"points": [[0, 6000], [9000, 6000]]})).unwrap();
    s.execute("takeoff.brick", &json!({"table": false})).unwrap();

    let (r, f) = export(&mut s);
    assert_eq!((f["format"].as_str(), f["version"].as_u64(), f["units"].as_str()), (Some("hcw-aqc-bridge"), Some(1), Some("mm")));
    assert_eq!(f["source"]["app"], "AAD");
    assert_eq!(f["source"]["drawing_id"], r["drawingId"]);
    assert_eq!(f["project"]["name"], "Small house");
    assert_eq!(f["project"]["client_name"], "A. Kumar");
    assert_eq!(f["project"]["site_area_m2"], 120.0);
    assert_eq!(f["levels"][0]["height_mm"], 3150.0);
    assert_eq!(f["levels"][0]["ceiling_mm"], 3000.0);
    assert_eq!(f["beam_depths_mm"], json!([300, 450]));
    assert_eq!(f["walls"].as_array().unwrap().len(), 2);
    assert_eq!(f["walls"][0]["length_mm"], 9000.0);
    assert_eq!(f["walls"][0]["height_mm"], 3000.0);
    let sched = f["opening_schedule"].as_array().unwrap();
    let d2 = sched.iter().find(|o| o["mark"] == "D2").unwrap();
    assert_eq!((d2["kind"].as_str(), d2["width_mm"].as_f64(), d2["nos"].as_u64()), (Some("Door"), Some(900.0), Some(1)));
    assert_eq!(f["openings"].as_array().unwrap().len(), 2, "one door and one window block");
    let warnings: Vec<&str> = r["warnings"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(warnings.iter().any(|w| w.contains("rooms")), "{warnings:?}");
    assert!(warnings.iter().any(|w| w.contains("slab")), "the slab take-off was not run: {warnings:?}");

    // The id is kept, so a second export is recognisable.
    let (r2, _) = export(&mut s);
    assert_eq!(r["drawingId"], r2["drawingId"]);
}

#[test]
fn export_to_a_file_and_empty_drawings_still_export() {
    let mut s = session();
    let path = std::env::temp_dir().join(format!("aad-bridge-{}.json", std::process::id()));
    let r = s.execute("bridge.export", &json!({"path": path.to_str().unwrap()})).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(text, r["json"].as_str().unwrap());
    let f: Value = serde_json::from_str(&text).unwrap();
    assert!(f["walls"].as_array().unwrap().is_empty());
    assert!(s.execute("bridge.export", &json!({"bearing": -5})).is_err());
    assert!(s.execute("bridge.export", &json!({"path": "Z:/no/such/dir/x.json"})).is_err());
}

const FROM_AQC: &str = r#"{
  "format": "hcw-aqc-bridge", "version": 1,
  "source": { "app": "AQC", "drawing_id": "aqc-1111", "exported": "2026-10-07T10:30:00Z" },
  "units": "mm",
  "project": { "name": "Residence for Mr Rao", "client_name": "S. Rao", "company_name": "Holgundi Consulting Works", "location": "Hospet", "address": "12, 4th Cross, Hospet" },
  "levels": [
    { "name": "First", "height_mm": 3150, "slab_thickness_mm": 150, "beam_depth_mm": 450 },
    { "name": "Second", "height_mm": 3300, "slab_thickness_mm": 125, "beam_depth_mm": 375 }
  ],
  "beam_depths_mm": [ 300, 375, 450 ],
  "walls": [ { "ref": "W1", "length_mm": 9000 } ]
}"#;

#[test]
fn import_previews_first_and_applies_only_on_request_without_touching_geometry() {
    let mut s = session();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    s.execute("project", &json!({"details": {"owner": "Someone Else"}})).unwrap();
    s.execute("floors", &json!({"floors": [{"name": "Ground"}, {"name": "First", "floorToFloor": 3.0, "ceiling": 2.85}]})).unwrap();
    let before = s.doc().unwrap().clone();

    let r = s.execute("bridge.import", &json!({"json": FROM_AQC})).unwrap();
    assert_eq!(r["applied"], false);
    assert_eq!(s.doc().unwrap().project, before.project, "a preview changes nothing");
    let items = r["items"].as_array().unwrap();
    let find = |sec: &str, field: &str| items.iter().find(|i| i["section"] == sec && i["field"] == field).unwrap().clone();
    assert_eq!(find("project", "title")["action"], "add");
    assert_eq!(find("project", "owner")["action"], "flagged", "a filled field is not overwritten silently");
    assert_eq!(find("floors", "First")["action"], "update");
    assert_eq!(find("floors", "Second")["action"], "add");
    assert_eq!(find("beams", "beam_depths_mm")["action"], "update");
    assert_eq!(find("geometry", "walls")["action"], "skipped");

    let r = s.execute("bridge.import", &json!({"json": FROM_AQC, "apply": true})).unwrap();
    assert_eq!(r["applied"], true);
    let details = s.execute("project", &json!({})).unwrap()["details"].clone();
    assert_eq!(
        (details["title"].as_str(), details["owner"].as_str(), details["location"].as_str()),
        (Some("Residence for Mr Rao"), Some("Someone Else"), Some("Hospet"))
    );
    let fl = s.execute("floors", &json!({})).unwrap()["floors"].clone();
    assert_eq!(fl[1]["floorToFloor"], 3.15);
    assert_eq!((fl[2]["name"].as_str(), fl[2]["floorToFloor"].as_f64(), fl[2]["slab"].as_f64()), (Some("Second"), Some(3.3), Some(125.0)));
    assert_eq!(s.execute("beams", &json!({})).unwrap()["depths"], json!([300.0, 375.0, 450.0]));
    assert_eq!(s.doc().unwrap().model.len(), before.model.len(), "geometry untouched");
    // The file's id was adopted, so the next export carries it.
    assert_eq!(s.execute("bridge.export", &json!({})).unwrap()["drawingId"], "aqc-1111");
    // Importing the same file again changes nothing.
    let again = s.execute("bridge.import", &json!({"json": FROM_AQC, "apply": true})).unwrap();
    assert!(again["summary"].as_str().unwrap().starts_with("0 to add, 0 to update"), "{again}");
    // `overwrite` replaces a filled field.
    s.execute("bridge.import", &json!({"json": FROM_AQC, "apply": true, "overwrite": true})).unwrap();
    assert_eq!(s.execute("project", &json!({})).unwrap()["details"]["owner"], "S. Rao");
}

#[test]
fn a_file_from_another_drawing_needs_confirmation_and_bad_files_are_refused() {
    let mut s = session();
    s.execute("bridge.export", &json!({})).unwrap();
    let r = s.execute("bridge.import", &json!({"json": FROM_AQC})).unwrap();
    assert_eq!(r["otherDrawing"], true);
    assert!(s.execute("bridge.import", &json!({"json": FROM_AQC, "apply": true})).is_err());
    assert_eq!(s.execute("bridge.import", &json!({"json": FROM_AQC, "apply": true, "acceptOtherDrawing": true})).unwrap()["applied"], true);

    for bad in [
        "not json",
        "[]",
        r#"{"format":"other","version":1,"units":"mm","source":{"drawing_id":"x"}}"#,
        r#"{"format":"hcw-aqc-bridge","version":2,"units":"mm","source":{"drawing_id":"x"}}"#,
        r#"{"format":"hcw-aqc-bridge","version":1,"units":"in","source":{"drawing_id":"x"}}"#,
        r#"{"format":"hcw-aqc-bridge","version":1,"units":"mm","source":{}}"#,
    ] {
        assert!(s.execute("bridge.import", &json!({"json": bad})).is_err(), "{bad}");
    }
    assert!(s.execute("bridge.import", &json!({})).is_err());
    assert!(s.execute("bridge.import", &json!({"path": "Z:/no/such/file.json"})).is_err());
}

#[test]
fn hostile_values_are_flagged_not_applied() {
    let mut s = session();
    let hostile = r#"{
      "format": "hcw-aqc-bridge", "version": 1, "source": { "drawing_id": "h1" }, "units": "mm",
      "levels": [ {"name": "", "height_mm": 3000}, {"name": "Tall", "height_mm": 1e12}, {"name": "Neg", "height_mm": -5},
                  {"name": "Thick", "height_mm": 3000, "slab_thickness_mm": 99999}, {"name": "Ok", "height_mm": 3000} ],
      "beam_depths_mm": [ -1, 0, 1e99 ]
    }"#;
    let r = s.execute("bridge.import", &json!({"json": hostile, "apply": true})).unwrap();
    assert_eq!(r["items"].as_array().unwrap().iter().filter(|i| i["action"] == "flagged").count(), 5, "{r}");
    let fl = s.execute("floors", &json!({})).unwrap()["floors"].clone();
    assert_eq!(fl.as_array().unwrap().len(), 2, "the default Ground floor and Ok");
    let big =
        format!(r#"{{"format":"hcw-aqc-bridge","version":1,"source":{{"drawing_id":"h"}},"units":"mm","project":{{"name":"{}"}}}}"#, "x".repeat(600));
    assert_eq!(s.execute("bridge.import", &json!({"json": big})).unwrap()["items"][0]["action"], "flagged");
}

#[test]
fn export_goes_beside_the_saved_drawing_by_default() {
    let mut s = session();
    let dir = std::env::temp_dir().join(format!("aad-bridge-dir-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dxf = dir.join("house.dxf");
    s.state_mut().unwrap().path = Some(dxf.to_str().unwrap().to_string());
    let r = s.execute("bridge.export", &json!({})).unwrap();
    let expected = dir.join("house.aqc.json");
    assert!(expected.exists(), "{r}");
    assert!(r["message"].as_str().unwrap().contains("house.aqc.json"));
    std::fs::remove_file(&expected).ok();
    let r = s.execute("bridge.export", &json!({"write": false})).unwrap();
    assert!(!expected.exists() && r.get("path").is_none());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn export_names_the_saved_drawing_revision_and_lists_rooms() {
    let mut s = session();
    let dir = std::env::temp_dir().join(format!("aad-bridge-rev-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dxf = dir.join("house.dxf");
    std::fs::write(&dxf, b"saved bytes").unwrap();
    let path = dxf.to_str().unwrap().to_string();
    s.execute("room", &json!({"p1": [0, 0], "p2": [4000, 3000], "name": "br", "units": "mm", "textHeight": 150})).unwrap();
    s.state_mut().unwrap().path = Some(path.clone());

    // No revision recorded yet: the export says so and carries none.
    let r = s.execute("bridge.export", &json!({"write": false})).unwrap();
    let warnings: Vec<&str> = r["warnings"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(warnings.iter().any(|w| w.contains("no matching saved revision")), "{warnings:?}");
    let file: Value = serde_json::from_str(r["json"].as_str().unwrap()).unwrap();
    assert!(file["source"].get("revision").is_none());

    // With a recorded revision that still matches the file, it is named, and the room is listed.
    let rev = aad_engine::revision::record(&path, b"saved bytes", 4).unwrap();
    let r = s.execute("bridge.export", &json!({"write": false})).unwrap();
    let file: Value = serde_json::from_str(r["json"].as_str().unwrap()).unwrap();
    assert_eq!(file["source"]["revision"]["sha256"], json!(rev.sha256));
    assert_eq!(file["rooms"][0]["name"], json!("BEDROOM"));
    assert_eq!(file["rooms"][0]["area_m2"], json!(12.0));
    let warnings: Vec<&str> = r["warnings"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(!warnings.iter().any(|w| w.contains("no room labels") || w.contains("no matching saved revision")), "{warnings:?}");
    // The room was drawn after the session opened, so the drawing differs from its saved state.
    assert!(warnings.iter().any(|w| w.contains("unsaved changes")), "{warnings:?}");

    // The file changed after the save: the revision is no longer offered.
    std::fs::write(&dxf, b"changed elsewhere").unwrap();
    let r = s.execute("bridge.export", &json!({"write": false})).unwrap();
    let file: Value = serde_json::from_str(r["json"].as_str().unwrap()).unwrap();
    assert!(file["source"].get("revision").is_none());
    std::fs::remove_dir_all(&dir).ok();
}
