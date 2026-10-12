//! Active Drawing Space: saved layer-state configurations (docs/ACTIVE_DRAWING_SPACE.md).

use aad_doc::Drawing;
use aad_engine::Session;
use serde_json::{Value, json};

fn session() -> Session {
    let mut s = Session::new();
    for l in ["A-WALL", "A-DOOR", "S-COLUMN", "E-LIGHT", "A-FURN"] {
        s.execute("layer.new", &json!({"name": l})).unwrap();
        s.execute("layer.current", &json!({"name": l})).unwrap();
        s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
        s.execute("circle", &json!({"center": [5, 5], "radius": 1})).unwrap();
    }
    s.execute("layer.current", &json!({"name": "A-WALL"})).unwrap();
    s
}

fn flags(s: &Session, layer: &str) -> (bool, bool, bool) {
    let l = s.doc().unwrap().layer(layer).unwrap();
    (l.on, l.frozen, l.locked)
}

fn working() -> Value {
    json!({"name": "Working Drawings", "current": "A-WALL", "layers": [
        {"layer": "A-WALL"}, {"layer": "A-DOOR"},
        {"layer": "S-COLUMN", "locked": true}, {"layer": "E-LIGHT", "locked": true},
        {"layer": "A-FURN", "visible": false, "locked": true},
    ]})
}

fn electrical() -> Value {
    json!({"name": "Electrical Layout", "current": "E-LIGHT", "layers": [
        {"layer": "A-WALL", "locked": true}, {"layer": "A-DOOR", "visible": false},
        {"layer": "S-COLUMN", "visible": false, "frozen": true}, {"layer": "E-LIGHT"},
        {"layer": "A-FURN", "locked": true},
    ]})
}

fn geometry(d: &Drawing) -> String {
    d.model.iter().map(|e| format!("{:?}", e)).collect::<Vec<_>>().join("|")
}

#[test]
fn activating_applies_visible_frozen_and_locked_independently_and_leaves_geometry_alone() {
    let mut s = session();
    let geo = geometry(s.doc().unwrap());
    s.execute("space.save", &working()).unwrap();
    s.execute("space.save", &electrical()).unwrap();
    let r = s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    assert_eq!(r["current"], "A-WALL");
    assert_eq!(flags(&s, "A-WALL"), (true, false, false));
    assert_eq!(flags(&s, "S-COLUMN"), (true, false, true), "visible but locked");
    assert_eq!(flags(&s, "A-FURN"), (false, false, true), "hidden and locked");
    assert_eq!(geometry(s.doc().unwrap()), geo, "no object was touched");
    s.execute("space.activate", &json!({"name": "Electrical Layout"})).unwrap();
    assert_eq!(flags(&s, "S-COLUMN"), (false, true, false), "hidden and frozen: separate from locked");
    assert_eq!(flags(&s, "A-DOOR"), (false, false, false), "hidden but thawed and unlocked");
    assert_eq!(s.doc().unwrap().header.str("CLAYER", ""), "E-LIGHT");
    assert_eq!(geometry(s.doc().unwrap()), geo);
}

#[test]
fn switching_away_and_back_restores_the_saved_states_exactly() {
    let mut s = session();
    s.execute("space.save", &working()).unwrap();
    s.execute("space.save", &electrical()).unwrap();
    s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    let a: Vec<_> = s.doc().unwrap().layers.iter().map(|l| (l.name.clone(), l.on, l.frozen, l.locked)).collect();
    for _ in 0..3 {
        s.execute("space.activate", &json!({"name": "Electrical Layout"})).unwrap();
        s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    }
    let b: Vec<_> = s.doc().unwrap().layers.iter().map(|l| (l.name.clone(), l.on, l.frozen, l.locked)).collect();
    assert_eq!(a, b);
}

#[test]
fn the_engine_protects_what_the_active_space_locks() {
    let mut s = session();
    s.execute("space.save", &working()).unwrap();
    s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    let d = s.doc().unwrap();
    let col = d.model.iter().find(|e| e.common.layer == "S-COLUMN").unwrap().handle.hex();
    let wall = d.model.iter().find(|e| e.common.layer == "A-WALL").unwrap().handle.hex();
    let before = geometry(d);
    let _ = s.execute("erase", &json!({"handles": [col]}));
    let _ = s.execute("move", &json!({"handles": [col], "delta": [5, 5]}));
    let _ = s.execute("properties.set", &json!({"handles": [col], "color": 2}));
    assert_eq!(geometry(s.doc().unwrap()), before, "locked in the space: unchanged");
    s.execute("move", &json!({"handles": [wall], "delta": [5, 5]})).expect("editable in the space: moves");
    assert_ne!(geometry(s.doc().unwrap()), before);
}

#[test]
fn status_shows_when_the_layers_differ_from_the_saved_space_and_saving_resets_it() {
    let mut s = session();
    s.execute("space.save", &working()).unwrap();
    s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    assert_eq!(s.execute("space.status", &json!({})).unwrap()["dirty"], false);
    s.execute("layer.set", &json!({"name": "S-COLUMN", "locked": false})).unwrap();
    let st = s.execute("space.status", &json!({})).unwrap();
    assert_eq!(st["dirty"], true);
    assert_eq!(st["differences"][0]["layer"], "S-COLUMN");
    assert_eq!(st["differences"][0]["field"], "locked");
    assert_eq!(st["differences"][0]["saved"], true);
    s.execute("space.capture", &json!({"name": "Working Drawings"})).unwrap();
    assert_eq!(s.execute("space.status", &json!({})).unwrap()["dirty"], false);
}

#[test]
fn editing_a_saved_space_does_not_change_the_drawing_until_it_is_activated() {
    let mut s = session();
    s.execute("space.save", &working()).unwrap();
    s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    let live = flags(&s, "A-DOOR");
    s.execute("space.update", &json!({"name": "Working Drawings", "layers": [{"layer": "A-DOOR", "locked": true}]})).unwrap();
    assert_eq!(flags(&s, "A-DOOR"), live, "the drawing is as it was");
    assert_eq!(s.execute("space.status", &json!({})).unwrap()["dirty"], true, "saved differs from applied");
    s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    assert_eq!(flags(&s, "A-DOOR"), (true, false, true));
}

#[test]
fn renamed_deleted_and_new_layers_are_handled() {
    let mut s = session();
    s.execute("space.save", &working()).unwrap();
    // Rename a layer the space lists: the space follows (a locked layer too).
    s.execute("layer.set", &json!({"name": "S-COLUMN", "locked": true})).unwrap();
    s.execute("layer.set", &json!({"name": "S-COLUMN", "newName": "S-COL"})).expect("renaming a locked layer works");
    assert!(s.doc().unwrap().model.iter().any(|e| e.common.layer == "S-COL"), "its objects follow");
    s.execute("layer.set", &json!({"name": "S-COL", "locked": false})).unwrap();
    s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    assert_eq!(flags(&s, "S-COL"), (true, false, true), "the renamed layer got its saved state");
    // Delete a layer the space lists: reported, no crash.
    s.execute("layer.set", &json!({"name": "E-LIGHT", "locked": false})).unwrap();
    let hs: Vec<String> = s.doc().unwrap().model.iter().filter(|e| e.common.layer == "E-LIGHT").map(|e| e.handle.hex()).collect();
    s.execute("erase", &json!({"handles": hs})).unwrap();
    s.execute("layer.delete", &json!({"name": "E-LIGHT"})).unwrap();
    let r = s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    assert_eq!(r["missing"], json!(["E-LIGHT"]));
    // A layer created afterwards is left alone (default policy)...
    s.execute("layer.new", &json!({"name": "NEW1"})).unwrap();
    s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    assert_eq!(flags(&s, "NEW1"), (true, false, false));
    // ...or hidden / locked by the space's policy.
    s.execute("space.update", &json!({"name": "Working Drawings", "unlisted": "lock"})).unwrap();
    s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    assert_eq!(flags(&s, "NEW1"), (true, false, true));
    s.execute("space.update", &json!({"name": "Working Drawings", "unlisted": "hide"})).unwrap();
    s.execute("layer.set", &json!({"name": "NEW1", "locked": false})).unwrap();
    s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    assert!(!flags(&s, "NEW1").0);
}

#[test]
fn a_frozen_current_layer_is_replaced_not_left_broken_and_unusable_current_layers_are_rejected() {
    let mut s = session();
    // The current layer (A-WALL) would be frozen by this space; it names no current layer.
    s.execute("space.save", &json!({"name": "Freeze wall", "layers": [{"layer": "A-WALL", "frozen": true}, {"layer": "A-DOOR"}]})).unwrap();
    let r = s.execute("space.activate", &json!({"name": "Freeze wall"})).unwrap();
    assert_eq!(r["current"], "A-DOOR");
    assert!(r["warnings"][0].as_str().unwrap().contains("frozen"));
    // A configuration whose own current layer is locked or hidden is refused when saved.
    assert!(s.execute("space.save", &json!({"name": "bad", "current": "A-WALL", "layers": [{"layer": "A-WALL", "locked": true}]})).is_err());
    assert!(s.execute("space.save", &json!({"name": "bad", "current": "A-WALL", "layers": [{"layer": "A-WALL", "visible": false}]})).is_err());
    // layer.current refuses frozen and locked layers and does not thaw or unlock them.
    s.execute("layer.set", &json!({"name": "A-FURN", "locked": true})).unwrap();
    assert!(s.execute("layer.current", &json!({"name": "A-FURN"})).is_err());
    assert_eq!(flags(&s, "A-FURN"), (true, false, true));
    assert!(s.execute("layer.current", &json!({"name": "A-WALL"})).is_err(), "A-WALL is frozen by the space");
    assert_eq!(flags(&s, "A-WALL"), (true, true, false));
    // A space that leaves nothing to draw on is refused whole.
    let mut all = json!({"name": "nothing", "layers": []});
    all["layers"] =
        Value::Array(["0", "A-WALL", "A-DOOR", "S-COLUMN", "E-LIGHT", "A-FURN"].iter().map(|l| json!({"layer": l, "frozen": true})).collect());
    s.execute("space.save", &all).unwrap();
    let before = s.doc().unwrap().layers.clone();
    assert!(s.execute("space.activate", &json!({"name": "nothing"})).is_err());
    assert_eq!(s.doc().unwrap().layers, before, "refused as a whole");
}

#[test]
fn activation_is_one_undo_step_and_the_space_data_is_undone_with_it() {
    let mut s = session();
    s.execute("space.save", &json!({"name": "Working Drawings", "current": "A-WALL", "activate": false, "layers": [{"layer": "A-WALL"}, {"layer": "A-FURN", "visible": false, "locked": true}]})).unwrap();
    let before: Vec<_> = s.doc().unwrap().layers.iter().map(|l| (l.on, l.frozen, l.locked)).collect();
    s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    s.execute("u", &json!({})).unwrap();
    let after: Vec<_> = s.doc().unwrap().layers.iter().map(|l| (l.on, l.frozen, l.locked)).collect();
    assert_eq!(before, after);
    assert_eq!(s.execute("space.status", &json!({})).unwrap()["active"], Value::Null);
    s.execute("u", &json!({})).unwrap();
    assert!(s.execute("space.list", &json!({})).unwrap()["spaces"].as_array().unwrap().is_empty(), "the save is undone too");
}

#[test]
fn list_duplicate_rename_delete_filter_by_floor_and_validate() {
    let mut s = session();
    s.execute("space.save", &json!({"name": "All floors", "activate": false})).unwrap();
    s.execute("space.save", &json!({"name": "Ground only", "floor": "Ground", "activate": false})).unwrap();
    s.execute("space.save", &json!({"name": "First only", "floor": "First", "activate": false})).unwrap();
    let names = |r: Value| r["spaces"].as_array().unwrap().iter().map(|x| x["name"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(names(s.execute("space.list", &json!({"floor": "Ground"})).unwrap()), ["All floors", "Ground only"]);
    assert_eq!(names(s.execute("space.list", &json!({})).unwrap()).len(), 3);
    assert!(s.execute("space.save", &json!({"name": "all FLOORS"})).is_err(), "names are unique");
    assert!(s.execute("space.save", &json!({"name": "  "})).is_err());
    assert!(s.execute("space.save", &json!({"name": "x", "layers": [{"layer": "NOPE"}]})).is_err());
    assert!(s.execute("space.save", &json!({"name": "x", "layers": [{"layer": "A-WALL"}, {"layer": "a-wall"}]})).is_err());
    assert!(s.execute("space.save", &json!({"name": "x", "unlisted": "weird"})).is_err());
    let d = s.execute("space.duplicate", &json!({"name": "Ground only"})).unwrap();
    assert_eq!(d["space"]["name"], "Ground only copy");
    s.execute("space.update", &json!({"name": "Ground only copy", "newName": "Ground v2"})).unwrap();
    assert!(s.execute("space.update", &json!({"name": "Ground v2", "newName": "ALL FLOORS"})).is_err());
    s.execute("space.activate", &json!({"name": "Ground v2"})).unwrap();
    s.execute("space.delete", &json!({"name": "Ground v2"})).unwrap();
    assert_eq!(s.execute("space.list", &json!({})).unwrap()["active"], Value::Null, "deleting the active space clears it");
}

#[test]
fn recover_shows_thaws_and_unlocks_everything() {
    let mut s = session();
    s.execute("space.save", &electrical()).unwrap();
    s.execute("space.activate", &json!({"name": "Electrical Layout"})).unwrap();
    let r = s.execute("space.recover", &json!({})).unwrap();
    assert!(r["changed"].as_u64().unwrap() >= 3);
    assert!(s.doc().unwrap().layers.iter().all(|l| l.on && !l.frozen && !l.locked));
}

#[test]
fn a_big_change_carries_a_warning_and_preview_changes_nothing() {
    let mut s = Session::new();
    for i in 0..40 {
        s.execute("layer.new", &json!({"name": format!("L{i}")})).unwrap();
    }
    let layers: Vec<Value> = (0..40).map(|i| json!({"layer": format!("L{i}"), "visible": false})).collect();
    s.execute("space.save", &json!({"name": "Hide many", "layers": layers, "activate": false})).unwrap();
    let before = s.doc().unwrap().layers.clone();
    let p = s.execute("space.preview", &json!({"name": "Hide many"})).unwrap();
    assert_eq!(p["many"], true);
    assert_eq!(s.doc().unwrap().layers, before, "preview changes nothing");
    let r = s.execute("space.activate", &json!({"name": "Hide many"})).unwrap();
    assert!(r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("40")));
}

#[test]
fn hostile_parameters_are_errors_not_panics() {
    let mut s = session();
    let bad: Vec<Value> = vec![
        json!({}),
        json!({"name": null}),
        json!({"name": 5}),
        json!({"name": "x", "layers": "no"}),
        json!({"name": "x", "layers": [1, 2]}),
        json!({"name": "x", "layers": [{"layer": null}]}),
        json!({"name": "x", "current": 7}),
        json!({"id": {"a": 1}}),
        json!({"name": "\u{0}\u{1}"}),
        json!({"name": "é".repeat(100)}),
    ];
    for id in ["space.save", "space.update", "space.activate", "space.preview", "space.duplicate", "space.delete", "space.capture", "space.get"] {
        for p in &bad {
            let r = s.execute(id, p);
            if let Err(e) = r {
                assert!(!e.to_string().contains("internal"), "{id} {p}: {e}");
            }
        }
    }
}

#[test]
fn choosing_a_floor_moves_nothing_keeps_the_active_space_and_rooms_follow_it() {
    let mut s = session();
    s.execute("floors", &json!({"floors": [{"name": "Ground"}, {"name": "First"}, {"name": "Second"}]})).unwrap();
    s.execute("space.save", &working()).unwrap();
    s.execute("space.activate", &json!({"name": "Working Drawings"})).unwrap();
    let geo = geometry(s.doc().unwrap());
    let layers: Vec<_> = s.doc().unwrap().layers.iter().map(|l| (l.on, l.frozen, l.locked)).collect();
    let r = s.execute("floor.current", &json!({})).unwrap();
    assert_eq!(r["mode"], "drawing");
    assert_eq!(r["floor"], "Ground", "the first floor until another is chosen");
    let r = s.execute("floor.current", &json!({"name": "first"})).unwrap();
    assert_eq!(r["floor"], "First");
    assert_eq!(geometry(s.doc().unwrap()), geo, "no object changed floor or anything else");
    assert_eq!(s.doc().unwrap().layers.iter().map(|l| (l.on, l.frozen, l.locked)).collect::<Vec<_>>(), layers, "layer states untouched");
    assert_eq!(s.execute("space.status", &json!({})).unwrap()["active"]["name"], "Working Drawings", "the drawing space is untouched");
    assert!(s.execute("floor.current", &json!({"name": "Roof"})).is_err());
    // A room made now carries the floor's code without being told the floor.
    s.execute("layer.set", &json!({"name": "A-WALL", "locked": false})).unwrap();
    let room = s.execute("room", &json!({"p1": [0, 0], "p2": [4, 3], "name": "Kitchen"})).unwrap();
    assert!(room["name"].as_str().unwrap().starts_with("FF"), "{room}");
    s.execute("floor.current", &json!({"name": "Second"})).unwrap();
    let room = s.execute("room", &json!({"p1": [10, 0], "p2": [14, 3], "name": "Study"})).unwrap();
    assert!(room["name"].as_str().unwrap().starts_with("SF"), "{room}");
}
