//! Walls, doors and windows: faces with mitred corners, openings cut in the wall lines, blocks that
//! carry a deduction line for the take-off, and line clean-up.

use cadcraft_engine::Session;
use cadcraft_engine::doc::EntityKind;
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    s
}

fn lines_on(s: &Session, layer: &str) -> Vec<(f64, f64, f64, f64)> {
    let mut v: Vec<(f64, f64, f64, f64)> = s
        .doc()
        .unwrap()
        .model
        .iter()
        .filter(|e| e.common.layer == layer)
        .filter_map(|e| {
            if let EntityKind::Line(l) = &e.kind { Some((l.a.x.min(l.b.x), l.a.y.min(l.b.y), l.a.x.max(l.b.x), l.a.y.max(l.b.y))) } else { None }
        })
        .collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

#[test]
fn wall_faces_have_end_caps_and_mitred_corners() {
    let mut s = session();
    let r = s.execute("wall.draw", &json!({"points": [[0, 0], [4000, 0]], "thickness": 230})).unwrap();
    assert_eq!(r["lines"].as_array().unwrap().len(), 4, "two faces and two end caps");
    assert_eq!(
        lines_on(&s, "A-WALL"),
        vec![(0.0, -115.0, 0.0, 115.0), (0.0, -115.0, 4000.0, -115.0), (0.0, 115.0, 4000.0, 115.0), (4000.0, -115.0, 4000.0, 115.0)]
    );

    let mut c = session();
    c.execute("wall.draw", &json!({"points": [[0, 0], [4000, 0], [4000, 3000]], "thickness": 200})).unwrap();
    let l = lines_on(&c, "A-WALL");
    // The outer corner is mitred at (4100, -100) and the inner one at (3900, 100).
    assert!(l.contains(&(0.0, -100.0, 4100.0, -100.0)), "{l:?}");
    assert!(l.contains(&(0.0, 100.0, 3900.0, 100.0)), "{l:?}");
    assert!(l.contains(&(4100.0, -100.0, 4100.0, 3000.0)), "{l:?}");

    // From selected lines, a closed path, and refusals.
    let mut f = session();
    f.execute("rectang", &json!({"p1": [0, 0], "p2": [4000, 3000]})).unwrap();
    let h = f.doc().unwrap().model.iter().next().unwrap().handle.hex();
    let r = f.execute("wall.draw", &json!({"handles": [h], "thickness": 200})).unwrap();
    assert_eq!(r["lines"].as_array().unwrap().len(), 8, "a closed path has no end caps");
    assert!(f.execute("wall.draw", &json!({"points": [[0, 0]]})).is_err());
    assert!(f.execute("wall.draw", &json!({"points": [[0, 0], [1, 1]], "thickness": 0})).is_err());
    assert!(f.execute("wall.draw", &json!({"handles": []})).is_err());
}

#[test]
fn a_door_cuts_both_faces_and_places_a_block_the_takeoff_deducts() {
    let mut s = session();
    s.execute("wall.draw", &json!({"points": [[0, 0], [4000, 0]], "thickness": 230})).unwrap();
    let r = s.execute("wall.door", &json!({"at": [2000, 0], "width": 900})).unwrap();
    assert_eq!(
        (r["block"].as_str(), r["cut"].as_u64(), r["thickness"].as_f64()),
        (Some("DOOR-900-230"), Some(2), Some(230.0)),
        "thickness read from the wall faces"
    );
    let w = lines_on(&s, "A-WALL");
    assert!(w.contains(&(0.0, 115.0, 1550.0, 115.0)) && w.contains(&(2450.0, 115.0, 4000.0, 115.0)), "{w:?}");
    assert!(w.contains(&(0.0, -115.0, 1550.0, -115.0)) && w.contains(&(2450.0, -115.0, 4000.0, -115.0)), "{w:?}");
    assert!(!w.contains(&(0.0, 115.0, 4000.0, 115.0)), "the face line was cut");
    let b = s.doc().unwrap().block("DOOR-900-230").unwrap().clone();
    assert!(b.entities.iter().any(|e| matches!(e.kind, EntityKind::Arc(_))), "swing arc");
    assert!(b.entities.iter().any(|e| e.common.layer == "MEASURE-DEDUCT"));

    // The take-off finds the door's deduction line inside the block.
    s.execute("takeoff.start", &json!({})).unwrap();
    s.execute("layer.current", &json!({"name": "MEASURE-FULLBRICK"})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [4000, 0]]})).unwrap();
    let t = s.execute("takeoff.brick", &json!({"table": false})).unwrap();
    assert_eq!(t["totals"]["openings"], 0.9);
    assert_eq!(t["items"][0]["net"], 3.1);
}

#[test]
fn doors_and_windows_follow_the_schedule_and_report_missing_walls() {
    let mut s = session();
    s.execute("openings.std", &json!({})).unwrap();
    s.execute("wall.draw", &json!({"points": [[0, 0], [0, 5000]], "thickness": 230})).unwrap();
    // The wall runs along y: the angle is found from the nearest wall line.
    let r = s.execute("wall.door", &json!({"at": [0, 1000], "name": "D2", "swing": "right"})).unwrap();
    assert_eq!(r["block"], "DOOR-D2-230");
    let r = s.execute("wall.window", &json!({"at": [0, 3000], "name": "W3"})).unwrap();
    assert_eq!(r["cut"], 2);
    let sched = s.execute("schedule", &json!({})).unwrap();
    let d2 = sched["openings"].as_array().unwrap().iter().find(|o| o["name"] == "D2").unwrap();
    assert_eq!(d2["block"], "DOOR-D2-230", "the entry is linked to the block");
    let counts = s.execute("openings.sync", &json!({})).unwrap()["counts"].clone();
    assert_eq!((counts["D2"].as_u64(), counts["W3"].as_u64()), (Some(1), Some(1)), "placing links the entry to its block, so sync counts it");
    assert!(s.execute("wall.door", &json!({"at": [9000, 9000]})).is_err(), "no wall near, no angle given");
    assert!(s.execute("wall.door", &json!({"at": [0, 1000], "name": "ZZ"})).is_err());
    assert!(s.execute("wall.door", &json!({"at": [0, 1000], "width": -5})).is_err());
    s.execute("u", &json!({})).unwrap();
}

#[test]
fn clean_erases_zero_length_and_duplicate_lines_and_joins_overlaps() {
    let mut s = session();
    s.execute("line", &json!({"points": [[0, 0], [1000, 0]]})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [1000, 0]]})).unwrap();
    s.execute("line", &json!({"points": [[1000, 0], [0, 0]]})).unwrap();
    s.execute("line", &json!({"points": [[500, 0], [2500, 0]]})).unwrap();
    s.execute("line", &json!({"points": [[2500, 0], [3000, 0]]})).unwrap();
    s.execute("line", &json!({"points": [[7, 7], [7, 7]]})).ok();
    s.execute("line", &json!({"points": [[0, 500], [1000, 500]]})).unwrap();
    let r = s.execute("wall.clean", &json!({})).unwrap();
    assert!(r["erased"].as_u64().unwrap() >= 4, "{r}");
    let l = lines_on(&s, "0");
    assert_eq!(l, vec![(0.0, 0.0, 3000.0, 0.0), (0.0, 500.0, 1000.0, 500.0)], "{l:?}");
}
