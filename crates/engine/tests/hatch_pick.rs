//! A pattern hatch is picked anywhere inside its area, and its boundary stays separately selectable.

use cadcraft_engine::Session;
use cadcraft_engine::doc::EntityKind;
use serde_json::json;

fn selected_kinds(s: &Session) -> Vec<&'static str> {
    let d = s.doc().unwrap();
    s.selection().iter().filter_map(|h| d.entity(*h)).map(|e| if matches!(e.kind, EntityKind::Hatch(_)) { "hatch" } else { "other" }).collect()
}

#[test]
fn hatch_is_selected_between_its_pattern_lines_and_apart_from_its_boundary() {
    let mut s = Session::new();
    s.execute("rectang", &json!({"p1": [0, 0], "p2": [100, 100]})).unwrap();
    s.execute("hatch", &json!({"points": [[50, 50]], "pattern": "ANSI31", "scale": 20})).unwrap();
    s.execute("zoom", &json!({"mode": "window", "p1": [-10, -10], "p2": [110, 110]})).unwrap();
    // Many interior points, most of them far from any pattern line at this scale.
    let mut hits = 0;
    let mut total = 0;
    for x in [13.0, 27.0, 41.0, 58.0, 73.0, 88.0] {
        for y in [12.0, 31.0, 47.0, 63.0, 79.0, 91.0] {
            s.execute("select", &json!({"clear": true})).unwrap();
            s.execute("select", &json!({"at": [x, y]})).unwrap();
            total += 1;
            if selected_kinds(&s) == ["hatch"] {
                hits += 1;
            }
        }
    }
    assert_eq!(hits, total, "every interior click selects the hatch");
    // On the boundary the rectangle wins over the hatch.
    s.execute("select", &json!({"clear": true})).unwrap();
    s.execute("select", &json!({"at": [50, 0]})).unwrap();
    assert_eq!(selected_kinds(&s), ["other"]);
    // Outside the hatch nothing is selected.
    s.execute("select", &json!({"clear": true})).unwrap();
    s.execute("select", &json!({"at": [150, 50]})).unwrap();
    assert!(s.selection().is_empty());
}
