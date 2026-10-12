//! Level marks, the level schedule, section and elevation markers, north and slope arrows.

use aad_engine::Session;
use aad_engine::doc::EntityKind;
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    s
}

fn texts(s: &Session) -> Vec<String> {
    s.doc().unwrap().model.iter().filter_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.value.clone()) } else { None }).collect()
}

#[test]
fn level_marks_are_remembered_and_listed_in_ascending_order() {
    let mut s = session();
    let r = s.execute("level.mark", &json!({"at": [0, 0], "value": 3.15})).unwrap();
    assert_eq!(r["label"], "+3.150");
    assert_eq!(s.execute("level.mark", &json!({"at": [0, 1000], "value": 0})).unwrap()["label"], "±0.000");
    assert_eq!(s.execute("level.mark", &json!({"at": [0, 2000], "value": -0.45})).unwrap()["label"], "-0.450");
    let sched = s.execute("level.schedule", &json!({})).unwrap();
    assert_eq!(sched["levels"][0], json!(["L1", "-0.450"]));
    assert_eq!(sched["levels"][2], json!(["L3", "+3.150"]));
    assert!(texts(&s).contains(&"LEVEL SCHEDULE".to_string()));
    assert!(s.execute("level.mark", &json!({"at": [0, 0]})).is_err());
    assert!(s.execute("level.mark", &json!({"at": [0, 0], "floor": "Nowhere"})).is_err());
    assert!(session().execute("level.schedule", &json!({})).is_err());
}

#[test]
fn markers_and_arrows_draw_and_refuse_bad_input() {
    let mut s = session();
    s.execute("section.marker", &json!({"p1": [0, 0], "p2": [5000, 0], "label": "B"})).unwrap();
    assert_eq!(texts(&s).iter().filter(|t| *t == "B").count(), 2);
    assert!(s.execute("section.marker", &json!({"p1": [0, 0], "p2": [0, 0]})).is_err());
    assert!(s.execute("section.marker", &json!({"p1": [0, 0], "p2": [1, 0], "label": "TOOLONG"})).is_err());
    assert!(s.execute("section.marker", &json!({"p1": [0, 0], "p2": [1, 0], "side": "up"})).is_err());
    s.execute("elevation.marker", &json!({"at": [0, 3000], "number": "1", "sheet": "A-201"})).unwrap();
    assert!(texts(&s).contains(&"A-201".to_string()));
    assert!(s.execute("elevation.marker", &json!({"at": [0, 0]})).is_err());
    s.execute("north.arrow", &json!({"at": [9000, 0]})).unwrap();
    assert!(texts(&s).contains(&"N".to_string()));
    assert!(s.execute("north.arrow", &json!({"at": [0, 0], "size": -1})).is_err());
    s.execute("slope.arrow", &json!({"p1": [0, 0], "p2": [3000, 0], "text": "1:100"})).unwrap();
    assert!(s.execute("slope.arrow", &json!({"p1": [0, 0], "p2": [3000, 0]})).is_err());
}
