//! The Calculator and Pomodoro widgets: opened from the Tools menu commands, driven by the same
//! commands the buttons send, and the timer keeps counting while its window is closed.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

fn harness() -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.in_window_menu = false;
    app.integrated_titlebar = false;
    app.run("line", json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 800.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    h
}

fn has(h: &Harness<'static, CadApp>, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

#[test]
fn the_calculator_opens_from_its_command_and_its_buttons_type() {
    let mut h = harness();
    assert!(!has(&h, "Length"), "closed until asked for");
    h.state_mut().run("ui.toggle.calculator", json!({})).unwrap();
    h.run_steps(5);
    assert!(has(&h, "Length"));

    let r = h.state_mut().run("ui.calculator", json!({"keys": "10' 6\" x 20'="})).unwrap();
    assert_eq!(r["display"], "210.00 ft²");
    assert_eq!(r["results"]["m2"], "19.51");
    assert_eq!(r["kind"], "area");
    h.run_steps(3);
    assert!(has(&h, "Area"), "the kind label follows the result");

    h.get_by_label("C").click();
    h.run_steps(3);
    assert_eq!(h.state_mut().run("ui.calculator", json!({})).unwrap()["display"], "0");
    h.get_by_label("7").click();
    h.run_steps(2);
    h.get_by_label("×").click();
    h.run_steps(2);
    h.get_by_label("6").click();
    h.run_steps(2);
    assert_eq!(h.state_mut().run("ui.calculator", json!({"action": "equals"})).unwrap()["display"], "42.00 m²");

    // The drawing was never touched.
    assert_eq!(h.state().session.doc().unwrap().model.iter().count(), 1);
    h.state_mut().run("ui.toggle.calculator", json!({"on": false})).unwrap();
    h.run_steps(3);
    assert!(!has(&h, "Length"));
}

#[test]
fn the_pomodoro_runs_pauses_resets_and_is_set_by_command() {
    let mut h = harness();
    let r = h.state_mut().run("ui.pomodoro", json!({})).unwrap();
    assert_eq!((r["label"].as_str(), r["running"].as_bool(), r["minutes"].as_u64()), (Some("25:00"), Some(false), Some(25)));
    h.state_mut().run("ui.toggle.pomodoro", json!({})).unwrap();
    h.run_steps(5);
    h.query_all_by_label("Start").last().unwrap().click();
    h.run_steps(3);
    assert_eq!(h.state_mut().run("ui.pomodoro", json!({"action": "status"})).unwrap()["running"], true);
    assert!(has(&h, "Pause"));
    h.get_by_label("Pause").click();
    h.run_steps(3);
    assert_eq!(h.state_mut().run("ui.pomodoro", json!({})).unwrap()["running"], false);
    h.get_by_label("Reset").click();
    h.run_steps(3);
    assert_eq!(h.state_mut().run("ui.pomodoro", json!({})).unwrap()["label"], "25:00");

    let r = h.state_mut().run("ui.pomodoro", json!({"action": "set", "minutes": 47})).unwrap();
    assert_eq!((r["minutes"].as_u64(), r["label"].as_str()), (Some(45), Some("45:00")));
    assert!(h.state_mut().run("ui.pomodoro", json!({"action": "set"})).is_err());
    assert!(h.state_mut().run("ui.pomodoro", json!({"action": "explode"})).is_err());

    // Closing the window leaves the timer running.
    h.state_mut().run("ui.pomodoro", json!({"action": "start"})).unwrap();
    h.state_mut().run("ui.toggle.pomodoro", json!({"on": false})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state_mut().run("ui.pomodoro", json!({})).unwrap()["running"], true);
}

#[test]
fn importing_an_aqc_file_previews_then_applies_on_the_button() {
    let mut h = harness();
    let path = std::env::temp_dir().join(format!("aad-aqc-ui-{}.json", std::process::id()));
    std::fs::write(
        &path,
        r#"{"format":"hcw-aqc-bridge","version":1,"source":{"app":"AQC","drawing_id":"ui-1"},"units":"mm","project":{"name":"Rao House"}}"#,
    )
    .unwrap();
    cadcraft_ui_egui::widgets::start_bridge_import(h.state_mut(), path.to_str().unwrap());
    h.run_steps(5);
    assert!(has(&h, "Overwrite fields already filled in AAD"), "the preview window is open");
    assert_eq!(h.state_mut().run("project", json!({})).unwrap()["details"]["title"], serde_json::Value::Null, "nothing applied yet");
    h.query_all_by_label("Apply").last().unwrap().click();
    h.run_steps(5);
    std::fs::remove_file(&path).ok();
    assert_eq!(h.state_mut().run("project", json!({})).unwrap()["details"]["title"], "Rao House");
    assert!(!has(&h, "Overwrite fields already filled in AAD"), "the window closes");
}

#[test]
fn menu_commands_that_need_input_open_their_dialog() {
    use egui::accesskit::Role;
    let mut h = harness();

    // Project details: type a title, press OK, and the drawing remembers it.
    h.state_mut().start("project");
    h.run_steps(5);
    assert!(has(&h, "Project title"), "the Project Details dialog is open");
    let n = h.query_all_by_role(Role::TextInput).count();
    h.query_all_by_role(Role::TextInput).nth(n - 11).unwrap().click();
    h.run_steps(2);
    h.query_all_by_role(Role::TextInput).nth(n - 11).unwrap().type_text("Rao House");
    h.run_steps(3);
    h.query_all_by_label("OK").last().unwrap().click();
    h.run_steps(5);
    assert!(!has(&h, "Project title"), "the dialog closes after OK");
    assert_eq!(h.state_mut().run("project", json!({})).unwrap()["details"]["title"], "Rao House");

    // Floors: a table, prefilled from the drawing.
    h.state_mut().start("floors");
    h.run_steps(5);
    assert!(has(&h, "Add row") && has(&h, "Floor to floor (m)"));
    h.query_all_by_label("Cancel").last().unwrap().click();
    h.run_steps(3);
    assert!(!has(&h, "Add row"));

    // Room: the room type is required; OK without it says so and stays open.
    h.state_mut().start("room");
    h.run_steps(5);
    assert!(has(&h, "Room type"));
    h.query_all_by_label("OK").last().unwrap().click();
    h.run_steps(3);
    assert!(has(&h, "Room type"), "still open");
    assert!(h.query_all_by_label_contains("is required").next().is_some(), "the reason is shown");
}
