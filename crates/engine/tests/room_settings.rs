//! The room label tool: it lists the rooms to pick from, takes the floor from the top bar, and
//! follows the Room Label Settings (units, box, text height, font).

use aad_engine::Session;
use aad_engine::doc::EntityKind;
use serde_json::json;

fn session() -> Session {
    let mut s = Session::default();
    s.execute("floors", &json!({"floors": [{"name": "Ground", "floorToFloor": 3.0}, {"name": "First", "floorToFloor": 3.0}]})).unwrap();
    s
}

fn texts(s: &Session) -> Vec<(String, f64, String)> {
    s.doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| if let EntityKind::Text(t) = &e.kind { Some((t.value.clone(), t.height, t.style.clone())) } else { None })
        .collect()
}

fn has_box(s: &Session) -> bool {
    s.doc().unwrap().model.iter().any(|e| e.common.layer == "ROOM-RECT")
}

#[test]
fn the_settings_start_with_a_box_and_defaults_and_validate_what_they_are_given() {
    let mut s = session();
    let r = s.execute("room.settings", &json!({})).unwrap();
    assert_eq!(r, json!({"units": "", "box": true, "textHeight": null, "font": ""}));
    let r = s.execute("room.settings", &json!({"units": "FT", "box": false, "textHeight": 150.0, "font": "Arial"})).unwrap();
    assert_eq!(r, json!({"units": "ft", "box": false, "textHeight": 150.0, "font": "Arial"}));
    assert_eq!(s.execute("room.settings", &json!({})).unwrap(), r, "they are kept");
    for bad in [json!({"units": "yards"}), json!({"box": "yes"}), json!({"textHeight": -3}), json!({"textHeight": "tall"}), json!({"font": 5})] {
        assert!(s.execute("room.settings", &bad).is_err(), "{bad}");
    }
    let r = s.execute("room.settings", &json!({"textHeight": null, "units": "", "font": ""})).unwrap();
    assert_eq!((r["textHeight"].is_null(), r["units"].as_str(), r["font"].as_str()), (true, Some(""), Some("")), "back to the defaults");
    // They are part of the drawing: one undo step each.
    s.execute("room.settings", &json!({"box": true})).unwrap();
    s.execute("room.settings", &json!({"box": false})).unwrap();
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.execute("room.settings", &json!({})).unwrap()["box"], true);
}

#[test]
fn a_room_follows_the_settings_and_a_parameter_still_wins() {
    let mut s = session();
    s.execute("room", &json!({"p1": [0, 0], "p2": [3400, 2500], "name": "br"})).unwrap();
    assert!(has_box(&s), "a box by default");
    let default_h = texts(&s)[0].1;
    s.execute("room.settings", &json!({"box": false, "textHeight": 200.0, "font": "IBM Plex Sans", "units": "mm"})).unwrap();
    s.execute("room", &json!({"p1": [5000, 0], "p2": [8400, 2500], "name": "ki"})).unwrap();
    let all = texts(&s);
    let new = &all[3..];
    assert!(new.iter().any(|t| t.0.contains("KITCHEN")));
    assert!(new[0].1 == 200.0 && 200.0 != default_h, "the text height setting: {new:?}");
    assert!(new.iter().all(|t| t.2 == "ROOM-LABELS"), "labels use the label text style");
    let style = s.doc().unwrap().text_styles.iter().find(|t| t.name == "ROOM-LABELS").unwrap().font.clone();
    assert_eq!(style, "IBM Plex Sans");
    assert!(new[1].0.contains("3.40 m x 2.50 m"), "the units setting (coordinates in mm, sizes in m): {:?}", new[1]);
    let boxes = s.doc().unwrap().model.iter().filter(|e| e.common.layer == "ROOM-RECT").count();
    assert_eq!(boxes, 1, "no box for the second room");
    // The call's own parameters win over the settings.
    s.execute("room", &json!({"p1": [0, 4000], "p2": [3000, 6000], "name": "li", "rectangle": true, "textHeight": 300.0})).unwrap();
    assert_eq!(s.doc().unwrap().model.iter().filter(|e| e.common.layer == "ROOM-RECT").count(), 2);
    assert_eq!(texts(&s).last().unwrap().1, 300.0 * 0.7);
}

#[test]
fn the_room_command_lists_the_rooms_and_numbers_them_from_the_floor_in_the_top_bar() {
    let mut s = session();
    s.execute("floor.current", &json!({"name": "First"})).unwrap();
    s.start("room").unwrap();
    let p = s.current_prompt().unwrap();
    assert!(p.options.len() >= 20, "the rooms to choose from");
    assert!(p.options.iter().any(|(n, c)| n == "Kitchen" && c == "ki"));
    s.cmdline("Kitchen").unwrap();
    s.cmdline("0,0").unwrap();
    s.cmdline("3000,2500").unwrap();
    let label = texts(&s).into_iter().find(|t| t.0.contains("KITCHEN")).unwrap().0;
    let prefix = label.trim_end_matches("KITCHEN").trim();
    assert!(!prefix.is_empty(), "the floor gives the number prefix: {label}");
    s.execute("floor.current", &json!({"name": "Ground"})).unwrap();
    s.start("room").unwrap();
    s.cmdline("ba").unwrap();
    s.cmdline("4000,0").unwrap();
    s.cmdline("6000,2000").unwrap();
    let ground = texts(&s).into_iter().find(|t| t.0.contains("BATHROOM")).unwrap().0;
    assert_ne!(ground.trim_end_matches("BATHROOM").trim(), prefix, "another floor, another prefix");
}
