//! Objects on a locked or frozen layer are never modified, by any editing command, whatever way it
//! is reached: handles given explicitly, the current selection, or a layer change. The engine
//! enforces this (`crates/engine/src/protect.rs`); the interface only shows it.

use aad_doc::{Drawing, EntityKind};
use aad_engine::Session;
use serde_json::{Value, json};

fn handle_of(d: &Drawing, f: impl Fn(&EntityKind) -> bool) -> String {
    d.model.iter().find(|e| f(&e.kind)).map(|e| e.handle.hex()).expect("entity")
}

/// A session with a protected layer `PROT` holding a line, a circle, a polyline and some text, and an
/// ordinary layer `FREE` with a line that crosses the protected objects.
fn session() -> Session {
    let mut s = Session::new();
    s.execute("layer.new", &json!({"name": "PROT"})).unwrap();
    s.execute("layer.new", &json!({"name": "FREE"})).unwrap();
    s.execute("layer.current", &json!({"name": "PROT"})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [20, 0]]})).unwrap();
    s.execute("line", &json!({"points": [[0, 50], [3, 50]]})).unwrap();
    s.execute("circle", &json!({"center": [10, 10], "radius": 3})).unwrap();
    s.execute("pline", &json!({"points": [[0, 20], [10, 25], [20, 20]]})).unwrap();
    s.execute("text", &json!({"at": [0, 30], "text": "NOTE", "height": 2})).unwrap();
    s.execute("layer.current", &json!({"name": "FREE"})).unwrap();
    s.execute("line", &json!({"points": [[5, -5], [5, 30]]})).unwrap();
    s.execute("line", &json!({"points": [[8, 40], [8, 60]]})).unwrap();
    s
}

fn lock(s: &mut Session, frozen: bool) {
    let key = if frozen { "frozen" } else { "locked" };
    s.execute("layer.set", &json!({"name": "PROT", key: true})).unwrap();
}

/// Every object on PROT as (handle, debug form), in order.
fn protected(s: &Session) -> Vec<(String, String)> {
    let d = s.doc().unwrap();
    d.model.iter().filter(|e| e.common.layer == "PROT").map(|e| (e.handle.hex(), format!("{e:?}"))).collect()
}

fn commands(d: &Drawing) -> Vec<(&'static str, Value)> {
    let lines: Vec<String> = d.model.iter().filter(|e| matches!(e.kind, EntityKind::Line(_))).map(|e| e.handle.hex()).collect();
    let (line, short, cut, bound) = (lines[0].clone(), lines[1].clone(), lines[2].clone(), lines[3].clone());
    let circle = handle_of(d, |k| matches!(k, EntityKind::Circle(_)));
    let pl = handle_of(d, |k| matches!(k, EntityKind::LwPolyline(_)));
    let text = handle_of(d, |k| matches!(k, EntityKind::Text(_)));
    let all = json!([line, circle, pl, text]);
    vec![
        ("erase", json!({"handles": all})),
        ("move", json!({"handles": all, "delta": [3, 4]})),
        ("copy", json!({"handles": all, "delta": [3, 4]})),
        ("rotate", json!({"handles": all, "base": [0, 0], "angle": 30})),
        ("scale", json!({"handles": all, "base": [0, 0], "factor": 2})),
        ("mirror", json!({"handles": all, "p1": [0, 0], "p2": [0, 10], "erase": true})),
        ("stretch", json!({"window": [[-1, -1], [25, 35]], "delta": [2, 2]})),
        ("trim", json!({"handle": line, "pick": [15, 0], "edges": [cut]})),
        ("extend", json!({"handle": short, "pick": [2.9, 50], "edges": [bound]})),
        ("break", json!({"handle": line, "p1": [5, 0], "p2": [8, 0]})),
        ("explode", json!({"handles": [pl]})),
        ("offset", json!({"handle": circle, "distance": 1, "side": [10, 10], "erase": true})),
        ("properties.set", json!({"handles": all, "color": 1})),
        ("properties.set", json!({"handles": all, "layer": "FREE"})),
        ("properties.set", json!({"handles": [circle], "radius": 5})),
        ("textedit", json!({"handle": text, "text": "CHANGED"})),
        ("pedit", json!({"handle": pl, "option": "close"})),
        ("lengthen", json!({"handle": line, "delta": 4, "pick": [19, 0]})),
        ("array", json!({"type": "rectangular", "handles": all, "rows": 2, "cols": 2, "rowDist": 5, "colDist": 5})),
        ("flatten", json!({"handles": all})),
        ("cutclip", json!({"handles": all})),
        ("overkill", json!({"handles": all})),
    ]
}

#[test]
fn no_editing_command_changes_an_object_on_a_locked_or_frozen_layer() {
    for frozen in [false, true] {
        let mut s = session();
        let list = commands(s.doc().unwrap());
        lock(&mut s, frozen);
        let before = protected(&s);
        assert!(before.len() >= 4);
        let kind = if frozen { "frozen" } else { "locked" };
        for (id, params) in list {
            let _ = s.execute(id, &params); // refused, skipped or a no-op: all are fine
            assert_eq!(protected(&s), before, "`{id}` {params} changed an object on a {kind} layer");
            // And selection-based forms: select the protected objects, run with no handles.
            let hs: Vec<_> = s.doc().unwrap().model.iter().filter(|e| e.common.layer == "PROT").map(|e| e.handle).collect();
            s.set_selection(hs);
            let mut p = params.clone();
            if let Some(o) = p.as_object_mut() {
                o.remove("handles");
            }
            let _ = s.execute(id, &p);
            assert_eq!(protected(&s), before, "`{id}` on the selection changed an object on a {kind} layer");
            s.set_selection(Vec::new());
        }
    }
}

#[test]
fn editable_objects_on_other_layers_are_still_edited() {
    let mut s = session();
    lock(&mut s, false);
    let free = s.doc().unwrap().model.iter().find(|e| e.common.layer == "FREE").map(|e| e.handle.hex()).unwrap();
    s.execute("move", &json!({"handles": [free], "delta": [1, 0]})).expect("the unprotected line moves");
    s.execute("properties.set", &json!({"handles": [free], "color": 3})).expect("and recolours");
    s.execute("erase", &json!({"handles": [free]})).expect("and erases");
}

#[test]
fn a_protected_object_does_not_move_when_it_is_edited_together_with_an_editable_one() {
    let mut s = session();
    lock(&mut s, false);
    let free = s.doc().unwrap().model.iter().find(|e| e.common.layer == "FREE").map(|e| e.handle.hex()).unwrap();
    let prot = s.doc().unwrap().model.iter().find(|e| e.common.layer == "PROT").map(|e| e.handle.hex()).unwrap();
    let before = s.doc().unwrap().model.iter().find(|e| e.handle.hex() == prot).unwrap().clone();
    let r = s.execute("move", &json!({"handles": [prot, free], "delta": [9, 9]}));
    assert_eq!(s.doc().unwrap().model.iter().find(|e| e.handle.hex() == prot).unwrap(), &before, "did not move ({r:?})");
    let r = s.execute("properties.set", &json!({"handles": [prot], "layer": "FREE"}));
    if let Err(e) = r {
        assert!(e.to_string().contains("PROT") || e.to_string().contains("locked"), "{e}");
    }
    assert_eq!(s.doc().unwrap().model.iter().find(|e| e.handle.hex() == prot).unwrap(), &before);
}

#[test]
fn nothing_is_drawn_on_a_locked_layer_and_unlocking_allows_it_again() {
    let mut s = session();
    s.execute("layer.current", &json!({"name": "PROT"})).unwrap();
    let n = s.doc().unwrap().model.len();
    lock(&mut s, false);
    let r = s.execute("line", &json!({"points": [[0, 40], [10, 40]]}));
    assert!(r.is_err(), "drawing on the locked current layer is refused");
    assert!(r.unwrap_err().to_string().contains("PROT"));
    assert_eq!(s.doc().unwrap().model.len(), n);
    s.execute("layer.set", &json!({"name": "PROT", "locked": false})).unwrap();
    s.execute("line", &json!({"points": [[0, 40], [10, 40]]})).expect("unlocked again");
    assert_eq!(s.doc().unwrap().model.len(), n + 1);
}

#[test]
fn locking_unlocking_and_undo_are_not_object_edits() {
    let mut s = session();
    let before = protected(&s);
    s.execute("layer.set", &json!({"name": "PROT", "locked": true})).unwrap();
    s.execute("layer.set", &json!({"name": "PROT", "frozen": true})).unwrap();
    s.execute("layer.set", &json!({"name": "PROT", "frozen": false, "locked": false})).unwrap();
    assert_eq!(protected(&s), before);
    s.execute("u", &json!({})).unwrap();
}

/// The commands above must be valid calls, otherwise "refused" proves nothing: on an unlocked
/// drawing nearly all of them succeed.
#[test]
fn the_command_list_is_made_of_working_calls() {
    let mut ok = Vec::new();
    let mut failed = Vec::new();
    for (id, params) in commands(session().doc().unwrap()) {
        let mut s = session();
        match s.execute(id, &params) {
            // handles are the same in every fresh session (same creation order)
            Ok(_) => ok.push(id),
            Err(e) => failed.push(format!("{id}: {e}")),
        }
    }
    eprintln!("working calls: {}, failing on an unlocked drawing: {failed:#?}", ok.len());
    assert!(ok.len() >= 17, "only {} of the calls work: {failed:#?}", ok.len());
}
