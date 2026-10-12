//! Text inside a block definition is edited through BLOCKTEXTEDIT and shows in every reference.

use aad_engine::Session;
use aad_engine::doc::EntityKind;
use serde_json::json;

#[test]
fn block_definition_text_can_be_listed_and_edited_and_undone() {
    let mut s = Session::new();
    s.execute("text", &json!({"at": [0, 0], "text": "DOOR", "height": 2})).unwrap();
    let t = s.doc().unwrap().model.iter().next().unwrap().handle.hex();
    s.execute("block", &json!({"name": "TAG", "base": [0, 0], "handles": [t]})).unwrap();
    s.execute("insert", &json!({"name": "TAG", "at": [10, 0]})).unwrap();
    s.execute("insert", &json!({"name": "TAG", "at": [20, 0]})).unwrap();

    let list = s.execute("blocktext", &json!({"block": "TAG"})).unwrap();
    let h = list["text"][0]["handle"].as_str().unwrap().to_string();
    assert_eq!(list["text"][0]["text"], "DOOR");

    let r = s.execute("blocktextedit", &json!({"block": "tag", "handle": h, "text": "WINDOW"})).unwrap();
    assert_eq!(r["instances"], 3, "the converted original and both inserts use the edited definition");
    let after = s.execute("blocktext", &json!({"block": "TAG"})).unwrap();
    assert_eq!(after["text"][0]["text"], "WINDOW");
    let b = s.doc().unwrap().block("TAG").unwrap().clone();
    assert!(b.entities.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "WINDOW" && t.height == 2.0)), "other properties kept");

    s.execute("u", &json!({})).unwrap();
    assert_eq!(s.execute("blocktext", &json!({"block": "TAG"})).unwrap()["text"][0]["text"], "DOOR");

    // Refusals: unknown block, unknown object, non-text object, locked layer.
    assert!(s.execute("blocktextedit", &json!({"block": "NOPE", "handle": h, "text": "x"})).is_err());
    assert!(s.execute("blocktextedit", &json!({"block": "TAG", "handle": "FFFF", "text": "x"})).is_err());
    s.execute("layer.set", &json!({"name": "0", "locked": true})).unwrap();
    assert!(s.execute("blocktextedit", &json!({"block": "TAG", "handle": h, "text": "x"})).is_err());
}
