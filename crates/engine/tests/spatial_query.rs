//! Where things are: the overview grid, objects in a window, the nearest objects, details by handle,
//! and the `/view` reference. All read-only and bounded.

use aad_engine::Session;
use aad_engine::cmd::context;
use serde_json::{Value, json};

/// A 100 x 60 sheet: a wall line along the bottom, a circle in the middle, text top right, and a
/// second layer holding a short line far from the rest.
fn session() -> Session {
    let mut s = Session::default();
    s.execute("layer.new", &json!({"name": "A-WALL"})).unwrap();
    s.execute("layer.new", &json!({"name": "A-ANNO"})).unwrap();
    s.execute("layer.current", &json!({"name": "A-WALL"})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [100, 0]]})).unwrap();
    s.execute("circle", &json!({"center": [50, 30], "radius": 5})).unwrap();
    s.execute("layer.current", &json!({"name": "A-ANNO"})).unwrap();
    s.execute("text", &json!({"at": [90, 55], "height": 2, "text": "KITCHEN"})).unwrap();
    s.execute("line", &json!({"points": [[10, 50], [20, 50]]})).unwrap();
    s
}

fn handles(r: &Value) -> Vec<String> {
    r["objects"].as_array().unwrap().iter().map(|o| o["handle"].as_str().unwrap().to_string()).collect()
}

#[test]
fn the_overview_says_which_part_of_the_drawing_holds_what() {
    let mut s = session();
    let r = s.execute("spatial.overview", &json!({"cols": 2, "rows": 2})).unwrap();
    assert_eq!(r["space"], "Model");
    assert_eq!(r["extents"]["min"], json!([0.0, 0.0]));
    let cells = r["cells"].as_array().unwrap();
    let total: u64 = cells.iter().map(|c| c["count"].as_u64().unwrap()).sum();
    assert_eq!(total, 4, "every object is counted once");
    let cell = |col: u64, row: u64| cells.iter().find(|c| c["col"] == col && c["row"] == row);
    assert!(cell(0, 1).is_some_and(|c| c["layers"]["A-ANNO"] == 1), "the short annotation line is top left");
    assert!(cell(1, 1).is_some_and(|c| c["types"]["Text"] == 1), "the text is top right");
    assert!(r["note"].as_str().unwrap().contains("row 0 is the bottom"));
    assert!(r["view"]["width"].as_f64().unwrap() > 0.0);
    // Hostile sizes are clamped, an empty drawing says so.
    let big = s.execute("spatial.overview", &json!({"cols": 100000, "rows": 0})).unwrap();
    assert_eq!(big["grid"], json!([12, 1]));
    let empty = Session::default().execute("spatial.overview", &json!({})).unwrap();
    assert_eq!(empty["cells"], json!([]));
}

#[test]
fn a_window_lists_what_touches_it_or_lies_wholly_inside() {
    let mut s = session();
    let around_circle = s.execute("spatial.query", &json!({"window": [40, 20, 60, 40]})).unwrap();
    assert_eq!(around_circle["matched"], 1);
    let c = &around_circle["objects"][0];
    assert_eq!(c["type"], "Circle");
    assert_eq!(c["radius"], 5.0);
    assert_eq!(c["center"], json!([50.0, 30.0]));
    // Crossing includes the wall that only touches the window; inside does not.
    let low = json!({"window": [20, -5, 80, 5]});
    assert_eq!(s.execute("spatial.query", &low).unwrap()["matched"], 1);
    let mut inside = low.clone();
    inside["mode"] = json!("inside");
    assert_eq!(s.execute("spatial.query", &inside).unwrap()["matched"], 0, "the wall runs out of the window");
    // Corners in any order, filters, limit and truncation.
    let r = s.execute("spatial.query", &json!({"window": [100, 60, 0, 0], "layers": ["a-anno"]})).unwrap();
    assert_eq!(r["matched"], 2);
    let r = s.execute("spatial.query", &json!({"window": [0, 0, 100, 60], "types": ["Line"], "limit": 1})).unwrap();
    assert_eq!((r["matched"].as_u64(), r["shown"].as_u64(), r["truncated"].as_bool()), (Some(2), Some(1), Some(true)));
    let text = s.execute("spatial.query", &json!({"window": [85, 50, 100, 60], "types": ["Text"]})).unwrap();
    assert_eq!(text["objects"][0]["text"], "KITCHEN");
    assert_eq!(text["objects"][0]["at"], json!([90.0, 55.0]));
    // Malformed windows are errors, not silent empty answers.
    for bad in [
        json!({}),
        json!({"window": [1, 2, 3]}),
        json!({"window": [0, 0, 0, 5]}),
        json!({"window": ["a", 0, 1, 1]}),
        json!({"window": [f64::MAX * 10.0, 0, 1, 1]}),
    ] {
        assert!(s.execute("spatial.query", &bad).is_err(), "{bad}");
    }
}

#[test]
fn the_nearest_objects_come_closest_first_with_their_distance() {
    let mut s = session();
    let r = s.execute("spatial.nearest", &json!({"point": [52, 30], "count": 2})).unwrap();
    assert_eq!(r["objects"][0]["type"], "Circle");
    assert_eq!(r["objects"][0]["distance"], 0.0, "the point is inside the circle's bounds");
    assert_eq!(r["objects"].as_array().unwrap().len(), 2);
    assert!(r["objects"][1]["distance"].as_f64().unwrap() > 0.0);
    let far = s.execute("spatial.nearest", &json!({"point": [0, 100], "count": 1, "types": ["Text"]})).unwrap();
    assert_eq!(far["objects"][0]["text"], "KITCHEN");
    assert!(s.execute("spatial.nearest", &json!({"point": [1]})).is_err());
    assert_eq!(
        s.execute("spatial.nearest", &json!({"point": [0, 0], "count": 1000})).unwrap()["objects"].as_array().unwrap().len(),
        4,
        "clamped to the drawing"
    );
}

#[test]
fn objects_are_described_by_handle_and_unknown_handles_are_reported() {
    let mut s = session();
    let all = s.execute("spatial.query", &json!({"window": [0, 0, 100, 60]})).unwrap();
    let hs = handles(&all);
    let r = s.execute("spatial.describe", &json!({"handles": [hs[0], "FFFFF", "zz"]})).unwrap();
    assert_eq!(r["objects"].as_array().unwrap().len(), 1);
    assert_eq!(r["missing"], json!(["FFFFF", "zz"]));
    assert_eq!(r["objects"][0]["from"], json!([0.0, 0.0]));
    assert_eq!(r["objects"][0]["length"], 100.0);
    assert!(s.execute("spatial.describe", &json!({"handles": (0..21).map(|i| format!("{i:X}")).collect::<Vec<_>>()})).is_err(), "at most 20");
    assert!(s.execute("spatial.describe", &json!({})).is_err());
}

#[test]
fn the_view_reference_gives_the_visible_window_with_positions_and_changes_nothing() {
    let mut s = session();
    s.execute("view.set", &json!({"center": [50, 30], "height": 20})).unwrap();
    let before = s.state().unwrap().revision;
    let r = context::resolve(&s, "view").unwrap();
    assert_eq!(r["view"]["center"], json!([50.0, 30.0]));
    assert!(r["objectsInView"].as_u64().unwrap() >= 1);
    assert!(r["objects"].as_array().unwrap().iter().any(|o| o["type"] == "Circle" && o["center"] == json!([50.0, 30.0])));
    assert_eq!(s.state().unwrap().revision, before);
    // Zoomed far away from everything: nothing in view.
    s.execute("view.set", &json!({"center": [5000, 5000], "height": 10})).unwrap();
    assert_eq!(context::resolve(&s, "view").unwrap()["objectsInView"], 0);
    assert!(context::PROVIDERS.iter().any(|p| p.name == "view"));
    // /drawing carries the view too.
    assert!(context::resolve(&s, "drawing").unwrap()["view"]["width"].as_f64().is_some());
}
