//! Door and window schedule, the deduction map, the opening table, rates and the bill.

use cadcraft_engine::Session;
use cadcraft_engine::doc::EntityKind;
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    s.execute("takeoff.start", &json!({})).unwrap();
    s
}

fn on(s: &mut Session, layer: &str) {
    s.execute("layer.current", &json!({"name": layer})).unwrap();
}

#[test]
fn standard_openings_take_their_heights_from_the_first_floor() {
    let mut s = session();
    s.execute("floors", &json!({"floors": [{"name": "Ground", "floorToFloor": 3.2, "ceiling": 3.0, "lintel": 2.4}]})).unwrap();
    assert_eq!(s.execute("openings.std", &json!({})).unwrap()["added"], 9);
    assert_eq!(s.execute("openings.std", &json!({})).unwrap()["added"], 0, "adding twice adds nothing");
    let r = s.execute("schedule", &json!({})).unwrap();
    let d2 = r["openings"].as_array().unwrap().iter().find(|o| o["name"] == "D2").unwrap();
    assert_eq!((d2["length"].as_f64(), d2["height"].as_f64()), (Some(0.9), Some(2.4)));
    let w3 = r["openings"].as_array().unwrap().iter().find(|o| o["name"] == "W3").unwrap();
    assert_eq!((w3["sill"].as_f64(), w3["height"].as_f64()), (Some(1.2), Some(1.2)), "sill + height reaches the lintel");
    assert!(r["warnings"].as_array().unwrap().is_empty(), "{r}");
}

#[test]
fn schedule_edits_are_validated() {
    let mut s = session();
    let bad = |s: &mut Session, list: serde_json::Value| s.execute("schedule", &json!({"openings": list})).unwrap_err().to_string();
    assert!(bad(&mut s, json!([{"name": "A", "length": 1, "height": 2}, {"name": "a", "length": 1, "height": 2}])).contains("twice"));
    assert!(bad(&mut s, json!([{"name": "", "length": 1, "height": 2}])).contains("name"));
    assert!(bad(&mut s, json!([{"name": "X", "length": 0, "height": 2}])).contains("positive"));
    assert!(bad(&mut s, json!([{"name": "X", "kind": "gate", "length": 1, "height": 2}])).contains("kind"));
    let r = s.execute("schedule", &json!({"openings": [{"name": "W9", "kind": "window", "length": 1.2, "height": 1.2, "sill": 0.5}, {"name": "D9", "length": 0.9, "height": 2.6}]})).unwrap();
    let w: Vec<&str> = r["warnings"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert!(w.iter().any(|m| m.contains("W9") && m.contains("does not reach")), "{w:?}");
    assert!(w.iter().any(|m| m.contains("D9") && m.contains("taller than the lintel")), "{w:?}");
    let e = s.execute("schedule", &json!({"map": {"FB01-D1": "NOPE"}})).unwrap_err().to_string();
    assert!(e.contains("not in the schedule"), "{e}");
}

#[test]
fn measured_openings_map_to_schedule_entries_and_paint_uses_their_size() {
    let mut s = session();
    s.execute("schedule", &json!({"openings": [{"name": "D2", "length": 0.9, "height": 2.1}, {"name": "D3", "length": 1.2, "height": 2.1}]}))
        .unwrap();
    on(&mut s, "MEASURE-FULLBRICK");
    s.execute("line", &json!({"points": [[0, 0], [6000, 0]]})).unwrap();
    on(&mut s, "MEASURE-DEDUCT");
    s.execute("line", &json!({"points": [[500, 0], [1400, 0]]})).unwrap();
    s.execute("line", &json!({"points": [[3000, 0], [4200, 0]]})).unwrap();
    s.execute("line", &json!({"points": [[5000, 0], [5300, 0]]})).unwrap();
    let r = s.execute("takeoff.brick", &json!({"table": false})).unwrap();
    assert_eq!(r["items"][0]["openings"].as_array().unwrap().len(), 3);

    let r = s.execute("openings.sync", &json!({})).unwrap();
    assert_eq!(r["mapped"], 2);
    assert_eq!(r["unmatched"], json!(["FB01-D3"]), "0.30 m is within 50 mm of no entry");
    let map = s.execute("schedule", &json!({})).unwrap()["map"].clone();
    assert_eq!(map, json!({"FB01-D1": "D2", "FB01-D2": "D3"}));

    // Wall paint: 6.00 m wall, 2.85 m ceiling; mapped openings use length x door height 2.10,
    // the unmapped one the measured length x wall height.
    let r = s.execute("takeoff.paint", &json!({"table": false})).unwrap();
    let expect_open: f64 = 0.9 * 2.1 + 1.2 * 2.1 + 0.3 * 2.85;
    assert!((r["totals"]["openings"].as_f64().unwrap() - (expect_open * 100.0).round() / 100.0).abs() < 1e-9, "{r}");
}

#[test]
fn block_references_are_counted_and_the_table_and_bill_are_produced() {
    let mut s = session();
    s.execute("openings.std", &json!({})).unwrap();
    let mut list = s.execute("schedule", &json!({})).unwrap()["openings"].clone();
    list[1]["block"] = json!("DOOR;DOOR-ALT");
    s.execute("schedule", &json!({"openings": list})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [900, 0]]})).unwrap();
    let h = s.doc().unwrap().model.iter().last().unwrap().handle.hex();
    s.execute("block", &json!({"name": "DOOR", "base": [0, 0], "handles": [h], "keep": "delete"})).unwrap();
    for x in [0, 2000, 4000] {
        s.execute("insert", &json!({"name": "DOOR", "at": [x, 0]})).unwrap();
    }
    let counts = s.execute("openings.sync", &json!({})).unwrap()["counts"].clone();
    assert_eq!(counts["D2"], 3);
    s.execute("openings.table", &json!({"at": [0, -5000]})).unwrap();
    let d = s.doc().unwrap();
    let texts: Vec<String> = d
        .model
        .iter()
        .filter(|e| e.common.layer == "MEASURE-TABLE")
        .filter_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.value.clone()) } else { None })
        .collect();
    assert!(
        texts.contains(&"DOOR AND WINDOW SCHEDULE".to_string())
            && texts.contains(&"TOTAL DOOR".to_string())
            && texts.contains(&"FLOOR HEIGHTS".to_string()),
        "{texts:?}"
    );

    // Rates and the bill.
    assert!(s.execute("takeoff.bill", &json!({})).is_err(), "no rates yet");
    assert!(s.execute("takeoff.rates", &json!({"rates": {"Bricks": {"unit": "m", "rate": -1}}})).is_err());
    s.execute("takeoff.rates", &json!({"rates": {"Bricks": {"unit": "m", "rate": 120.5}, "Floor": {"unit": "sqm", "rate": 10}}})).unwrap();
    assert!(s.execute("takeoff.bill", &json!({})).is_err(), "no rated take-off has been run");
    on(&mut s, "MEASURE-FULLBRICK");
    s.execute("line", &json!({"points": [[0, 3000], [4000, 3000]]})).unwrap();
    s.execute("takeoff.brick", &json!({"table": false})).unwrap();
    let r = s.execute("takeoff.bill", &json!({})).unwrap();
    assert_eq!(r["lines"].as_array().unwrap().len(), 1, "only rated take-offs that were run");
    assert_eq!(r["total"], 482.0);
    assert!(r["csv"].as_str().unwrap().contains("Bricks,4,m,120.5,482"));
}
