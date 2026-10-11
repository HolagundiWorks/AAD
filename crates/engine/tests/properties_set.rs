//! Property changes reach the entities, respect locked layers and loaded linetypes, and undo.

use cadcraft_engine::Session;
use cadcraft_engine::doc::EntityKind;
use cadcraft_engine::doc::color::Color;
use serde_json::json;

fn color_of(s: &Session, i: usize) -> Color {
    s.doc().unwrap().model.iter().nth(i).unwrap().common.color
}

#[test]
fn properties_set_changes_selected_entities_skips_locked_layers_and_undoes() {
    let mut s = Session::new();
    s.execute("layer.new", &json!({"name": "WALLS"})).unwrap();
    s.execute("layer.new", &json!({"name": "FROZEN-IN-PLACE"})).unwrap();
    s.execute("layer.current", &json!({"name": "WALLS"})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [5, 0]]})).unwrap();
    s.execute("layer.current", &json!({"name": "FROZEN-IN-PLACE"})).unwrap();
    s.execute("line", &json!({"points": [[0, 1], [5, 1]]})).unwrap();
    let hs: Vec<String> = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    let (ha, hb) = (hs[0].clone(), hs[1].clone());
    s.execute("layer.set", &json!({"name": "FROZEN-IN-PLACE", "locked": true})).unwrap();

    let r = s.execute("properties.set", &json!({"handles": [ha, hb], "color": "red", "lineweight": 0.5})).unwrap();
    assert_eq!((r["changed"].as_u64(), r["skippedLocked"].as_u64()), (Some(1), Some(1)));
    assert_eq!(color_of(&s, 0), Color::Index(1));
    assert_eq!(color_of(&s, 1), Color::ByLayer, "the locked layer's object is untouched");

    // Only locked objects: refused with the reason.
    let e = s.execute("properties.set", &json!({"handles": [hb], "color": "blue"})).unwrap_err().to_string();
    assert!(e.contains("locked"), "{e}");
    // A linetype that isn't loaded is refused; a loaded one is applied.
    assert!(s.execute("properties.set", &json!({"handles": [ha], "linetype": "NOSUCH"})).is_err());
    s.execute("linetype", &json!({"load": "DASHED"})).unwrap();
    s.execute("properties.set", &json!({"handles": [ha], "linetype": "DASHED"})).unwrap();
    let kinds = s.doc().unwrap().model.iter().next().unwrap();
    assert_eq!(kinds.common.linetype, "DASHED");
    assert!(matches!(kinds.kind, EntityKind::Line(_)));

    s.execute("u", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().model.iter().next().unwrap().common.linetype, "ByLayer", "undo restores the linetype");
    s.execute("u", &json!({})).ok();
}
