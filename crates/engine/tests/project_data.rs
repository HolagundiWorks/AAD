//! Project data kept in the drawing: details, floors and heights, beam depths and levels.

use cadcraft_engine::Session;
use serde_json::json;

#[test]
fn details_floors_beams_and_levels_are_validated_stored_and_undoable() {
    let mut s = Session::new();
    let r = s.execute("project", &json!({"details": {"title": "Villa", "owner": "A. Rao", "siteArea": 240.5}})).unwrap();
    assert_eq!(r["details"]["siteArea"], "240.5");
    assert_eq!(s.execute("project", &json!({"details": {"title": "Villa 2"}})).unwrap()["details"]["owner"], "A. Rao", "set merges");
    assert!(s.execute("project", &json!({"details": {"colour": "red"}})).is_err(), "unknown detail");

    // Raising the count copies the last floor and names the new ones.
    let r = s.execute("floors", &json!({"count": 3})).unwrap();
    let names: Vec<&str> = r["floors"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Ground", "First", "Second"]);
    let lv = s.execute("levels", &json!({})).unwrap();
    assert_eq!(lv["levels"][2]["level"], 6.0);

    // Impossible heights are refused with the reason.
    let e = s.execute("floors", &json!({"floors": [{"name": "G", "floorToFloor": 3.0, "ceiling": 3.4}]})).unwrap_err().to_string();
    assert!(e.contains("taller than the floor-to-floor"), "{e}");
    let e = s.execute("floors", &json!({"floors": [{"name": "G", "floorToFloor": 3.0, "ceiling": 2.8, "lintel": 2.9}]})).unwrap_err().to_string();
    assert!(e.contains("above the ceiling"), "{e}");
    assert!(s.execute("floors", &json!({"count": 0})).is_err());
    assert_eq!(s.execute("floors", &json!({})).unwrap()["floors"].as_array().unwrap().len(), 3, "a refused call changes nothing");

    assert_eq!(s.execute("beams", &json!({})).unwrap()["depths"].as_array().unwrap().len(), 5);
    assert!(s.execute("beams", &json!({"depths": [-5]})).is_err());
    s.execute("beams", &json!({"depths": [200, 250]})).unwrap();

    s.execute("u", &json!({})).unwrap();
    assert_eq!(s.execute("beams", &json!({})).unwrap()["depths"].as_array().unwrap().len(), 5, "undo restores the beam depths");
}
