//! Grid, columns, lintels, stairs, lifts and handrails.

use cadcraft_engine::Session;
use cadcraft_engine::doc::EntityKind;
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    s
}

fn count(s: &Session, layer: &str) -> usize {
    s.doc().unwrap().model.iter().filter(|e| e.common.layer == layer).count()
}

#[test]
fn a_grid_is_drawn_remembered_and_columns_go_on_its_intersections() {
    let mut s = session();
    let r = s.execute("grid.axis", &json!({"bays": [4000, 5000], "rows": [3000, 3500, 3000]})).unwrap();
    assert_eq!(r["xs"], json!([0.0, 4000.0, 9000.0]));
    assert_eq!(r["ys"], json!([0.0, 3000.0, 6500.0, 9500.0]));
    assert_eq!(count(&s, "AN-GRID"), 3 + 4 + 7 + 7, "lines, bubbles and labels: 7 lines, 7 circles, 7 labels");
    let texts: Vec<String> =
        s.doc().unwrap().model.iter().filter_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.value.clone()) } else { None }).collect();
    assert!(texts.contains(&"3".to_string()) && texts.contains(&"D".to_string()), "{texts:?}");

    let r = s.execute("column.place", &json!({"size": [230, 450]})).unwrap();
    assert_eq!(r["placed"], 12, "3 x 4 intersections");
    let r = s.execute("column.place", &json!({"size": 300, "window": [[-100, -100], [4100, 3100]]})).unwrap();
    assert_eq!(r["placed"], 4);
    let sched = s.execute("column.schedule", &json!({"table": false})).unwrap();
    assert_eq!(sched["rows"][0], json!(["C1", "0.45 x 0.23", "12"]));
    assert_eq!(sched["rows"][1], json!(["C2", "0.30 x 0.30", "4"]));

    let mut bare = session();
    assert!(bare.execute("column.place", &json!({"size": 300})).is_err(), "no grid");
    assert!(bare.execute("column.place", &json!({"size": -1, "points": [[0, 0]]})).is_err());
    assert_eq!(bare.execute("column.place", &json!({"size": 300, "points": [[0, 0], [1000, 0]]})).unwrap()["placed"], 2);
    assert!(bare.execute("grid.axis", &json!({})).is_err());
    assert!(bare.execute("grid.axis", &json!({"bays": [0]})).is_err());
}

#[test]
fn lintels_cover_each_opening_with_a_bearing_at_both_ends() {
    let mut s = session();
    s.execute("wall.draw", &json!({"points": [[0, 0], [6000, 0]], "thickness": 230})).unwrap();
    s.execute("wall.door", &json!({"at": [1000, 0], "width": 900})).unwrap();
    s.execute("wall.window", &json!({"at": [4000, 0], "width": 1200})).unwrap();
    let r = s.execute("lintel.draw", &json!({})).unwrap();
    assert_eq!(r["lintels"], 2);
    let mut lens: Vec<f64> = s
        .doc()
        .unwrap()
        .model
        .iter()
        .filter(|e| e.common.layer == "MEASURE-LINTEL")
        .filter_map(|e| if let EntityKind::Line(l) = &e.kind { Some(l.a.xy().dist(l.b.xy())) } else { None })
        .collect();
    lens.sort_by(f64::total_cmp);
    assert_eq!(lens, vec![1200.0, 1500.0], "opening plus 150 mm either side");
    let t = s.execute("takeoff.beams", &json!({"table": false})).unwrap();
    assert_eq!(t["totals"]["length"], 2.7);
    let mut empty = session();
    assert!(empty.execute("lintel.draw", &json!({})).is_err());
}

#[test]
fn stairs_report_risers_goings_and_concrete_from_their_inputs() {
    let mut s = session();
    let r = s.execute("stair.plan", &json!({"at": [0, 0], "rise": 3.0, "width": 1100})).unwrap();
    assert_eq!((r["risers"].as_u64(), r["treads"].as_u64()), (Some(18), Some(17)), "3000 / 175 = 17.1, so 18 risers");
    assert!((r["riser"].as_f64().unwrap() - 166.7).abs() < 0.05);
    assert_eq!(r["run"], 4250.0);
    // Tread lines, the outline, the arrow and the notes were drawn on A-STAIR.
    assert!(count(&s, "A-STAIR") >= 16 + 1 + 1 + 2);
    let c = r["concrete"].as_f64().unwrap();
    assert!(c > 1.0 && c < 4.0, "{c}");
    let d = s.execute("stair.plan", &json!({"at": [0, 5000], "rise": 3.0, "width": 1100, "kind": "dogleg"})).unwrap();
    assert_eq!(d["flights"], 2.0);
    assert!(d["run"].as_f64().unwrap() < r["run"].as_f64().unwrap(), "two flights are shorter");
    assert!(s.execute("stair.plan", &json!({"at": [0, 0], "rise": 0, "width": 1100})).is_err());
    assert!(s.execute("stair.plan", &json!({"at": [0, 0], "rise": 3, "width": 1100, "kind": "spiral"})).is_err());
}

#[test]
fn lift_shafts_and_handrails() {
    let mut s = session();
    let r = s.execute("lift.shaft", &json!({"at": [0, 0], "width": 2000, "depth": 2200})).unwrap();
    assert_eq!(r["outer"], json!([2460.0, 2660.0]));
    assert_eq!(r["car"], json!([1800.0, 2000.0]));
    assert!(s.execute("lift.shaft", &json!({"at": [0, 0], "width": 2000, "depth": 2200, "car": [2500, 2000]})).is_err());
    let r = s.execute("rail.draw", &json!({"points": [[0, 5000], [3000, 5000], [3000, 8000]], "post": 1000})).unwrap();
    assert_eq!(r["posts"], 7, "the first post plus three per run");
    assert!(s.execute("rail.draw", &json!({"points": [[0, 0]]})).is_err());
}
