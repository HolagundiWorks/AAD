//! Quantity take-off: lengths, areas, openings, numbering, rounding, tables and export.

use aad_engine::Session;
use aad_engine::doc::EntityKind;
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

fn walls(s: &mut Session) {
    on(s, "MEASURE-FULLBRICK");
    s.execute("line", &json!({"points": [[0, 0], [4000, 0]]})).unwrap();
    s.execute("line", &json!({"points": [[4000, 0], [4000, 3000]]})).unwrap();
    s.execute("line", &json!({"points": [[0, 3000], [4000, 3000]]})).unwrap();
}

#[test]
fn start_creates_the_measure_layers_once() {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    assert_eq!(s.execute("takeoff.start", &json!({})).unwrap()["created"], 15);
    assert_eq!(s.execute("takeoff.start", &json!({})).unwrap()["created"], 0);
    assert!(s.doc().unwrap().layer("MEASURE-DEDUCT").is_some());
}

#[test]
fn brickwork_numbers_walls_groups_equal_lengths_and_deducts_openings() {
    let mut s = session();
    walls(&mut s);
    on(&mut s, "MEASURE-DEDUCT");
    s.execute("line", &json!({"points": [[1000, 0], [2000, 0]]})).unwrap();
    let r = s.execute("takeoff.brick", &json!({})).unwrap();
    let marks: Vec<&str> = r["items"].as_array().unwrap().iter().map(|i| i["mark"].as_str().unwrap()).collect();
    assert_eq!(marks, ["FB01", "FB02", "FB03"], "left to right, then bottom to top");
    assert_eq!(r["items"][0]["openings"][0]["name"], "FB01-D1");
    assert_eq!(r["items"][0]["net"], 3.0);
    assert_eq!(r["totals"], json!({"count": 3, "length": 11.0, "openings": 1.0, "net": 10.0}));
    assert_eq!(r["rows"][0], json!(["FB-A", "2", "4.00", "8.00", "1.00", "7.00"]));
    assert_eq!(r["rows"][1], json!(["FB-B", "1", "3.00", "3.00", "0.00", "3.00"]));
    // Labels and a table were drawn on their layers, and the result was saved.
    let d = s.doc().unwrap();
    assert!(d.model.iter().any(|e| e.common.layer == "MEASURE-LABELS" && matches!(&e.kind, EntityKind::Text(t) if t.value == "FB01-D1")));
    assert!(d.model.iter().any(|e| e.common.layer == "MEASURE-TABLE"));
    assert!(s.execute("takeoff.list", &json!({})).unwrap()["takeoffs"].as_array().unwrap().iter().any(|n| n == "Bricks"));
    // Repeating gives identical numbers.
    s.execute("takeoff.clear", &json!({})).unwrap();
    assert_eq!(s.execute("takeoff.brick", &json!({})).unwrap()["totals"], r["totals"]);
}

#[test]
fn deductions_inside_blocks_and_orphans_are_handled() {
    let mut s = session();
    walls(&mut s);
    on(&mut s, "MEASURE-DEDUCT");
    s.execute("line", &json!({"points": [[0, 500], [1000, 500]]})).unwrap();
    let h = s.doc().unwrap().model.iter().last().unwrap().handle.hex();
    s.execute("block", &json!({"name": "DOOR", "base": [0, 500], "handles": [h], "keep": "delete"})).unwrap();
    s.execute("insert", &json!({"name": "DOOR", "at": [1000, 0]})).unwrap();
    // A block that touches no selected wall is ignored; a loose line on no wall is flagged.
    s.execute("insert", &json!({"name": "DOOR", "at": [9000, 9000]})).unwrap();
    s.execute("line", &json!({"points": [[7000, 7000], [7500, 7000]]})).unwrap();
    let r = s.execute("takeoff.brick", &json!({"table": false})).unwrap();
    assert_eq!(r["items"][0]["net"], 3.0, "the deduction line inside the block sits on the bottom wall");
    assert_eq!(r["totals"]["openings"], 1.0);
    let w = r["warnings"].as_array().unwrap();
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].as_str().unwrap().contains("lie on no wall"));
    assert!(s.doc().unwrap().model.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "D? NO LINE")));
}

#[test]
fn areas_columns_slab_and_rectangles() {
    let mut s = session();
    on(&mut s, "MEASURE-CEILING");
    s.execute("rectang", &json!({"p1": [0, 0], "p2": [4000, 3000]})).unwrap();
    s.execute("rectang", &json!({"p1": [5000, 0], "p2": [9000, 3000]})).unwrap();
    s.execute("rectang", &json!({"p1": [0, 4000], "p2": [2000, 5000]})).unwrap();
    let r = s.execute("takeoff.ceiling", &json!({"table": false})).unwrap();
    assert_eq!(r["rows"][0], json!(["CP-A", "1", "2.00", "2.00"]), "groups are named in left-to-right order of first appearance");
    assert_eq!(r["rows"][1], json!(["CP-B", "2", "12.00", "24.00"]));
    assert_eq!(r["totals"]["area"], 26.0);

    on(&mut s, "MEASURE-COLUMN");
    for x in [0, 3000, 6000] {
        s.execute("rectang", &json!({"p1": [x, 0], "p2": [x + 230, 450]})).unwrap();
    }
    s.execute("rectang", &json!({"p1": [0, 1000], "p2": [300, 1300]})).unwrap();
    let r = s.execute("takeoff.columns", &json!({"table": false})).unwrap();
    assert_eq!(r["rows"][0], json!(["C1", "0.45 x 0.23", "3"]));
    assert_eq!(r["rows"][1], json!(["C2", "0.30 x 0.30", "1"]));

    on(&mut s, "MEASURE-SLAB");
    s.execute("rectang", &json!({"p1": [0, 0], "p2": [5000, 4000]})).unwrap();
    on(&mut s, "MEASURE-SLAB-DEDUCT");
    s.execute("rectang", &json!({"p1": [1000, 1000], "p2": [2000, 2000]})).unwrap();
    s.execute("rectang", &json!({"p1": [9000, 9000], "p2": [9500, 9500]})).unwrap();
    let r = s.execute("takeoff.slab", &json!({"table": false})).unwrap();
    assert_eq!(
        (r["items"][0]["gross"].as_f64(), r["items"][0]["openings"].as_f64(), r["items"][0]["net"].as_f64()),
        (Some(20.0), Some(1.0), Some(19.0))
    );
    assert!(r["warnings"][0].as_str().unwrap().contains("lies in no slab"));

    on(&mut s, "MEASURE-COUNT");
    s.execute("rectang", &json!({"p1": [0, 0], "p2": [1000, 500]})).unwrap();
    s.execute("rectang", &json!({"p1": [2000, 0], "p2": [2500, 1000]})).unwrap();
    let r = s.execute("takeoff.rects", &json!({"table": false})).unwrap();
    assert_eq!(r["rows"][0], json!(["R-A", "1.00", "0.50", "2", "0.50", "3.00"]), "the same size in either orientation groups together");

    on(&mut s, "MEASURE-AREA");
    s.execute("circle", &json!({"center": [0, 0], "radius": 1000})).unwrap();
    let r = s.execute("takeoff.area", &json!({"table": false})).unwrap();
    assert!((r["items"][0]["area"].as_f64().unwrap() - std::f64::consts::PI).abs() < 0.01);
}

#[test]
fn wall_paint_uses_the_floor_ceiling_height_and_imperial_drawings_use_feet() {
    let mut s = session();
    walls(&mut s);
    on(&mut s, "MEASURE-DEDUCT");
    s.execute("line", &json!({"points": [[1000, 0], [2000, 0]]})).unwrap();
    let r = s.execute("takeoff.paint", &json!({"table": false})).unwrap();
    assert_eq!(r["totals"], json!({"wallArea": 31.35, "openings": 2.85, "net": 28.5, "height": 2.85}));
    s.execute("floors", &json!({"floors": [{"name": "Ground", "floorToFloor": 3.2, "ceiling": 3.0, "lintel": 2.1}]})).unwrap();
    assert_eq!(s.execute("takeoff.paint", &json!({"table": false})).unwrap()["totals"]["height"], 3.0);
    assert!(s.execute("takeoff.paint", &json!({"floor": "Roof"})).is_err());

    let mut f = Session::new();
    f.execute("drawing.units", &json!({"units": "ft", "mode": "metadata"})).unwrap();
    f.execute("takeoff.start", &json!({})).unwrap();
    on(&mut f, "MEASURE-LINEAR");
    f.execute("line", &json!({"points": [[0, 0], [12.5, 0]]})).unwrap();
    let r = f.execute("takeoff.linear", &json!({"table": false})).unwrap();
    assert_eq!((r["lengthUnit"].as_str(), r["totals"]["length"].as_f64()), (Some("ft"), Some(12.5)));
    let mut unknown = Session::new();
    unknown.execute("setvar", &json!({"name": "INSUNITS", "value": 0})).unwrap();
    assert!(unknown.execute("takeoff.linear", &json!({})).is_err());
    assert!(unknown.execute("takeoff.linear", &json!({"units": "m", "table": false})).is_ok());
}

#[test]
fn results_export_to_csv_and_clear_only_removes_labels_and_tables() {
    let mut s = session();
    walls(&mut s);
    let r = s.execute("takeoff.brick", &json!({})).unwrap();
    let before = s.doc().unwrap().model.len();
    let erased = s.execute("takeoff.clear", &json!({})).unwrap()["erased"].as_u64().unwrap() as usize;
    assert_eq!(s.doc().unwrap().model.len(), before - erased);
    assert_eq!(s.doc().unwrap().model.iter().filter(|e| e.common.layer == "MEASURE-FULLBRICK").count(), 3, "measured lines stay");
    let dir = std::env::temp_dir().join(format!("aad-takeoff-{}", std::process::id()));
    let out = s.execute("takeoff.export", &json!({"path": dir.to_str().unwrap()})).unwrap();
    let file = out["files"][0].as_str().unwrap().to_string();
    let csv = std::fs::read_to_string(&file).unwrap();
    assert!(csv.starts_with("BRICKWORK\n"), "{csv}");
    assert!(csv.contains("FB-A,2,4.00,8.00,0.00,8.00"), "{csv}");
    assert_eq!(r["rows"].as_array().unwrap().len() + 2, csv.lines().count());
    std::fs::remove_dir_all(&dir).unwrap();
    let mut empty = session();
    assert!(empty.execute("takeoff.export", &json!({})).is_err());
}
