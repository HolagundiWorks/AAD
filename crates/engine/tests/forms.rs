//! Input forms: commands that take JSON are asked for on the command line and the drawing, and a
//! dialog can answer the settings first.

use aad_engine::doc::EntityKind;
use aad_engine::geom::Vec2;
use aad_engine::{Input, Session, forms};
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    s
}

fn count(s: &Session, layer: &str) -> usize {
    s.doc().unwrap().model.iter().filter(|e| e.common.layer == layer).count()
}

#[test]
fn a_dialog_answers_the_settings_and_the_points_are_picked() {
    let mut s = session();
    s.start_form("wall.draw", json!({"thickness": "200"})).unwrap();
    let p = s.current_prompt().unwrap();
    assert!(p.message.contains("pick points"), "{}", p.message);
    s.input(Input::Point(Vec2::new(0.0, 0.0))).unwrap();
    s.input(Input::Point(Vec2::new(4000.0, 0.0))).unwrap();
    assert_eq!(count(&s, "A-WALL"), 0, "nothing is drawn until the points are finished");
    s.input(Input::Enter).unwrap();
    assert!(s.running.is_none(), "the command finished");
    assert_eq!(count(&s, "A-WALL"), 4, "two faces and two end caps");
    // One undo removes the whole wall.
    s.undo().unwrap();
    assert_eq!(count(&s, "A-WALL"), 0);

    s.start_form("wall.draw", json!({"thickness": "200"})).unwrap();
    s.input(Input::Enter).unwrap();
    assert!(s.running.is_none() && count(&s, "A-WALL") == 0, "Enter with no points cancels");
}

#[test]
fn the_command_line_asks_for_what_is_missing() {
    let mut s = session();
    s.start("stair.plan").unwrap();
    assert!(s.current_prompt().unwrap().message.contains("rise"), "asks the required fields in order after the choice default");
    // `kind` is optional and has a default, so the first question is the rise.
    s.input(Input::Text("3".into())).unwrap();
    s.input(Input::Text("1100".into())).unwrap();
    assert!(s.current_prompt().unwrap().message.contains("Start of the stair"));
    s.input(Input::Point(Vec2::new(0.0, 0.0))).unwrap();
    assert!(s.running.is_none());
    assert!(count(&s, "A-STAIR") > 10);

    // A bad answer is refused and the question stays.
    s.start("stair.plan").unwrap();
    s.input(Input::Text("tall".into())).ok();
    assert!(s.current_prompt().unwrap().message.contains("rise"), "a bad answer keeps the question");
    s.input(Input::Cancel).unwrap();
    assert!(s.running.is_none());
}

#[test]
fn optional_points_can_be_skipped_with_enter() {
    let mut s = session();
    s.start("grid.axis").unwrap();
    s.input(Input::Text("4000 5000".into())).unwrap();
    assert!(s.current_prompt().unwrap().message.contains("origin"), "{:?}", s.current_prompt());
    s.input(Input::Enter).unwrap();
    assert!(s.running.is_none());
    assert!(count(&s, "AN-GRID") > 0);
    s.execute("grid.axis", &json!({"bays": [4000, 5000], "rows": [3000, 3000]})).unwrap();
    // Columns: size first, then positions; Enter takes every grid intersection.
    s.start_form("column.place", json!({"size": "300"})).unwrap();
    s.input(Input::Enter).unwrap();
    assert!(s.running.is_none());
    let cols = s.doc().unwrap().model.iter().filter(|e| matches!(e.kind, EntityKind::LwPolyline(_)) && e.common.layer == "A-COL").count();
    assert!(cols >= 3, "{cols}");
}

#[test]
fn forms_exist_for_the_commands_people_open_from_menus() {
    for id in [
        "project",
        "floors",
        "beams",
        "schedule",
        "room",
        "wall.draw",
        "wall.door",
        "wall.window",
        "grid.axis",
        "column.place",
        "stair.plan",
        "level.mark",
        "takeoff.brick",
        "bridge.export",
    ] {
        assert!(forms::get(id).is_some(), "{id}");
    }
    let s = session();
    let f = forms::get("floors").unwrap();
    let pre = forms::prefill(&s, f);
    assert_eq!(pre["floors"][0]["name"], "Ground", "the dialog starts from the drawing's floors");
    let room = forms::get("room").unwrap();
    // The room form asks for the room (from a list) and picks the corners: the floor comes from the top bar and the
    // units, box, text height and font from the Room Label Settings, so none of those are asked here.
    assert!(room.has_picks() && room.dialog_fields().count() == 1);
    assert!(forms::room_types().iter().any(|(c, _)| *c == "br"));
}

#[test]
fn project_and_floors_forms_round_trip_through_their_commands() {
    let mut s = session();
    let f = forms::get("project").unwrap();
    let params = forms::parse_input(f, &json!({"title": "Rao House", "owner": "S. Rao", "siteArea": "186.5"})).unwrap();
    s.execute("project", &params).unwrap();
    assert_eq!(forms::prefill(&s, f)["title"], "Rao House");
    let f = forms::get("floors").unwrap();
    let params =
        forms::parse_input(f, &json!({"floors": [["Ground", "3.15", "3.0", "2.1", "150", "450"], ["First", "3.0", "2.85", "2.1", "150", "450"]]}))
            .unwrap();
    s.execute("floors", &params).unwrap();
    assert_eq!(forms::prefill(&s, f)["floors"].as_array().unwrap().len(), 2);
    let f = forms::get("takeoff.rates").unwrap();
    let params = forms::parse_input(f, &json!({"rates": [["Bricks", "m3", "5200"]]})).unwrap();
    s.execute("takeoff.rates", &params).unwrap();
    let rows = forms::prefill(&s, forms::get("takeoff.rates").unwrap());
    assert_eq!(rows["rates"][0]["takeoff"], "Bricks");
}
