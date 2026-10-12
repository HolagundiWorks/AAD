//! `/reference` resolution for the assistant (`cmd::context`): live data, bounded, read-only,
//! missing data reported differently from empty data.

use aad_engine::Session;
use aad_engine::cmd::context::{self, PROVIDERS, RefError};
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    for l in ["A-WALL", "S-COLUMN"] {
        s.execute("layer.new", &json!({"name": l})).unwrap();
        s.execute("layer.current", &json!({"name": l})).unwrap();
        s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    }
    s.execute("layer.set", &json!({"name": "S-COLUMN", "locked": true})).unwrap();
    s
}

#[test]
fn references_are_found_in_text_but_paths_and_fractions_are_not() {
    assert_eq!(context::references_in("Using /rooms and /layers, identify rooms without labels"), ["rooms", "layers"]);
    assert_eq!(context::references_in("/Drawing /drawing /DRAWING"), ["drawing"], "any case, once");
    assert!(context::references_in("open c:/dir/file.dxf and scale 1/100").is_empty(), "a slash inside a word is not a reference");
    assert_eq!(context::references_in("/selection."), ["selection"]);
    assert!(context::references_in("/ alone").is_empty());
}

#[test]
fn every_advertised_reference_returns_live_data_from_the_open_drawing() {
    let mut s = session();
    s.execute("floors", &json!({"floors": [{"name": "Ground"}, {"name": "First"}]})).unwrap();
    s.execute("floor.current", &json!({"name": "First"})).unwrap();
    s.execute("project", &json!({"details": {"title": "Rao House", "owner": "A. Rao"}})).unwrap();
    s.execute("room", &json!({"p1": [0, 0], "p2": [4, 3], "name": "Kitchen"})).unwrap();
    s.execute("schedule", &json!({"openings": [{"name": "D1", "kind": "door", "length": 0.9, "height": 2.1, "count": 3}]})).unwrap();
    s.execute("space.save", &json!({"name": "Working", "layers": [{"layer": "A-WALL"}, {"layer": "S-COLUMN", "locked": true}]})).unwrap();
    let h = s.doc().unwrap().model.iter().next().unwrap().handle;
    s.set_selection(vec![h]);

    let drawing = context::resolve(&s, "drawing").unwrap();
    assert_eq!(drawing["currentLayer"], "S-COLUMN");
    assert!(drawing["objectsInSpace"].as_u64().unwrap() >= 5);
    assert!(drawing["objectsByType"]["Line"].as_u64().unwrap() >= 2);

    let sel = context::resolve(&s, "selection").unwrap();
    assert_eq!(sel["count"], 1);
    assert_eq!(sel["objects"][0]["handle"], h.hex());

    let layers = context::resolve(&s, "layers").unwrap();
    let col = layers["layers"].as_array().unwrap().iter().find(|l| l["name"] == "S-COLUMN").unwrap();
    assert_eq!(col["locked"], true, "lock state is live");
    assert_eq!(layers["drawingSpace"]["name"], "Working");

    let rooms = context::resolve(&s, "rooms").unwrap();
    assert_eq!(rooms["count"], 1);
    assert!(rooms["rooms"][0]["name"].as_str().unwrap().contains("KITCHEN"));
    assert!(rooms["rooms"][0]["area"].as_str().unwrap().len() > 2);

    let floor = context::resolve(&s, "floor").unwrap();
    assert_eq!(floor["current"], "First");

    let schedule = context::resolve(&s, "schedule").unwrap();
    assert_eq!(schedule["openings"][0]["name"], "D1");

    let project = context::resolve(&s, "project").unwrap();
    assert_eq!(project["details"]["title"], "Rao House");

    assert!(PROVIDERS.iter().all(|p| context::resolve(&s, p.name).is_ok()), "nothing is advertised that cannot answer here");
}

#[test]
fn missing_data_is_reported_with_a_reason_and_empty_data_is_an_answer() {
    let mut s = Session::new();
    // A plain drawing: no room labels, floors, schedule or project details to read.
    for name in ["rooms", "floor", "schedule", "project"] {
        match context::resolve(&s, name) {
            Err(RefError::Unavailable { reason, .. }) => assert!(!reason.is_empty()),
            other => panic!("/{name} should be unavailable, got {other:?}"),
        }
    }
    // Nothing selected is not an error: it is an empty selection.
    let sel = context::resolve(&s, "selection").unwrap();
    assert_eq!(sel["count"], 0);
    assert_eq!(sel["objects"], json!([]));
    // Room labels that exist and were then erased: the layer is there, the list is empty.
    s.execute("room", &json!({"p1": [0, 0], "p2": [4, 3], "name": "Hall"})).unwrap();
    let hs: Vec<String> = s.doc().unwrap().model.iter().filter(|e| e.common.layer == "ROOM-LABELS").map(|e| e.handle.hex()).collect();
    s.execute("erase", &json!({"handles": hs})).unwrap();
    let rooms = context::resolve(&s, "rooms").unwrap();
    assert_eq!(rooms["count"], 0);
    assert_eq!(rooms["rooms"], json!([]));
    // Unknown references and a missing document.
    assert!(matches!(context::resolve(&s, "walls"), Err(RefError::Unknown(_))));
    assert!(context::resolve(&s, "walls").unwrap_err().to_string().contains("/drawing"), "the error lists what is available");
    let none = Session::empty();
    assert!(matches!(context::resolve(&none, "drawing"), Err(RefError::Unavailable { .. })));
}

#[test]
fn resolving_changes_nothing_and_never_switches_floor_or_drawing() {
    let mut s = session();
    s.execute("floors", &json!({"floors": [{"name": "Ground"}, {"name": "First"}]})).unwrap();
    s.execute("floor.current", &json!({"name": "Ground"})).unwrap();
    s.execute("space.save", &json!({"name": "Working"})).unwrap();
    let doc = s.doc().unwrap().clone();
    let (rev, undo, docs, active) = {
        let st = s.state().unwrap();
        (st.revision, st.undo.len(), s.docs.len(), s.active)
    };
    let r = context::resolve_text(&s, "/drawing /selection /layers /rooms /floor /schedule /project /walls");
    assert_eq!(
        r.used,
        ["drawing", "selection", "layers", "floor", "project"]
            .iter()
            .filter(|n| r.used.contains(&n.to_string()))
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
    );
    assert!(r.problems.iter().any(|p| p.contains("/walls")) && r.problems.iter().any(|p| p.contains("/rooms")));
    assert_eq!(s.doc().unwrap(), &doc, "the drawing is as it was");
    let st = s.state().unwrap();
    assert_eq!((st.revision, st.undo.len(), s.docs.len(), s.active), (rev, undo, docs, active), "no revision, undo step, or switch of drawing");
    assert_eq!(s.execute("floor.current", &json!({})).unwrap()["floor"], "Ground", "the floor did not change");
}

#[test]
fn big_drawings_give_bounded_context() {
    let mut s = Session::new();
    for i in 0..400 {
        s.execute("layer.new", &json!({"name": format!("LAYER-WITH-A-LONG-NAME-{i}")})).unwrap();
    }
    let v = context::resolve(&s, "layers").unwrap();
    assert!(v.to_string().chars().count() <= context::MAX_CHARS + 200, "{} characters", v.to_string().len());
    assert_eq!(v["truncated"], true);
    assert_eq!(v["count"], 402, "the count still tells the truth about what was left out");
}
