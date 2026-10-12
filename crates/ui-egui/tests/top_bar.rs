//! The top bar: a customizable first line (tools you choose) and a second line with the layer,
//! floor and drawing space selectors. The old property row (layer, colour, linetype, lineweight
//! dropdowns) is gone.

use cadcraft_engine::Session;
use cadcraft_ui_egui::topbar;
use cadcraft_ui_egui::{CadApp, Services, WorkspaceMode};
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

fn harness(mode: WorkspaceMode) -> Harness<'static, CadApp> {
    let mut a = CadApp::new(Session::default(), Services::default());
    a.ui.in_window_menu = false;
    a.integrated_titlebar = false;
    a.run("line", json!({"points": [[0, 0], [10, 0]]})).unwrap();
    a.run(if mode == WorkspaceMode::Pro { "ui.workspace.pro" } else { "ui.workspace.classic" }, json!({})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 800.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app_holder(a),
    );
    h.run_steps(5);
    h
}

fn app_holder(a: CadApp) -> CadApp {
    a
}

fn has(h: &Harness<'_, CadApp>, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

fn y_of(h: &Harness<'_, CadApp>, label: &str) -> f32 {
    h.query_all_by_label(label).next().unwrap_or_else(|| panic!("nothing labelled {label}")).rect().center().y
}

#[test]
fn the_first_line_holds_the_chosen_tools_and_the_second_line_the_selectors() {
    let h = harness(WorkspaceMode::Classic);
    for tool in ["Room Label", "Room Area", "Architectural Layer Set"] {
        assert!(has(&h, tool), "default first-line tool {tool}");
    }
    let first = y_of(&h, "Room Label");
    let second = y_of(&h, "Drawing space");
    assert!(first < second, "the tools line is above the selectors line ({first} vs {second})");
    assert!(
        (y_of(&h, "Layer") - second).abs() < 6.0 && (y_of(&h, "Floor") - second).abs() < 6.0,
        "layer, floor and drawing space share the second line"
    );
    // The old property row is gone: no dropdown sits in the top bar area above the selectors.
    let combos_in_top_bar = h.query_all_by_role(Role::ComboBox).filter(|n| n.rect().center().y < second - 8.0).count();
    assert_eq!(combos_in_top_bar, 0, "no layer, colour, linetype or lineweight dropdowns in the first line");
}

#[test]
fn pro_has_no_first_line_but_keeps_the_second() {
    let h = harness(WorkspaceMode::Pro);
    assert!(!has(&h, "Room Label"), "Pro has no toolbars, so no first line");
    assert!(has(&h, "Drawing space") && has(&h, "Floor"));
}

#[test]
fn the_first_line_can_be_set_by_command_and_bad_entries_are_refused_or_cleaned() {
    let mut h = harness(WorkspaceMode::Classic);
    h.state_mut().run("ui.topbar.set", json!({"items": ["line", "-", "circle"]})).unwrap();
    h.run_steps(4);
    assert!(has(&h, "Line") && has(&h, "Circle") && !has(&h, "Room Label"));
    assert!(h.state_mut().run("ui.topbar.set", json!({"items": ["line", "no-such-tool"]})).is_err(), "an unknown tool is refused");
    assert!(h.state_mut().run("ui.topbar.set", json!({"items": "line"})).is_err());
    let r = h.state_mut().run("ui.topbar.set", json!({"items": ["-", "circle", "circle", "-", "-", "line", "-"]})).unwrap();
    assert_eq!(r["items"], json!(["circle", "-", "line"]), "stray separators and repeats are removed");
    let g = h.state_mut().run("ui.topbar.get", json!({})).unwrap();
    assert_eq!(g["items"], json!(["circle", "-", "line"]));
}

#[test]
fn the_customize_dialog_adds_removes_and_reorders_tools_and_reset_restores_the_default() {
    let mut h = harness(WorkspaceMode::Classic);
    h.state_mut().run("ui.topbar.set", json!({"items": ["line", "circle"]})).unwrap();
    h.state_mut().run("ui.dialog.customizetop", json!({})).unwrap();
    h.run_steps(5);
    assert!(has(&h, "Customize Top Bar") && has(&h, "Available tools") && has(&h, "On the bar, left to right"));
    // Tick a tool that is not on the bar.
    h.query_all_by_role_and_label(Role::CheckBox, "Polyline").next().expect("Polyline in the catalog").click();
    h.run_steps(4);
    assert_eq!(h.state().ui.top_bar, ["line", "circle", "pline"], "ticked tools are appended");
    // Move the last one left, then remove the first.
    let ups: Vec<_> = h.query_all_by_label("▲").collect();
    ups.last().unwrap().click();
    h.run_steps(4);
    assert_eq!(h.state().ui.top_bar, ["line", "pline", "circle"]);
    h.query_all_by_label("✕").next().unwrap().click();
    h.run_steps(4);
    assert_eq!(h.state().ui.top_bar, ["pline", "circle"]);
    // Untick removes it too.
    h.query_all_by_role_and_label(Role::CheckBox, "Circle").next().expect("Circle checkbox").click();
    h.run_steps(4);
    assert_eq!(h.state().ui.top_bar, ["pline"]);
    h.query_all_by_label("Reset").next().unwrap().click();
    h.run_steps(4);
    assert_eq!(h.state().ui.top_bar, topbar::default_items());
}

#[test]
fn each_mode_keeps_its_own_first_line_and_resetui_restores_the_default() {
    let mut a = CadApp::new(Session::empty(), Services::default());
    a.run("ui.topbar.set", json!({"items": ["line", "-", "circle"]})).unwrap();
    a.run("ui.workspace.pro", json!({})).unwrap();
    assert_eq!(a.ui.top_bar, topbar::default_items(), "Pro starts from the default, not Classic's list");
    a.run("ui.topbar.set", json!({"items": ["arc"]})).unwrap();
    a.run("ui.workspace.classic", json!({})).unwrap();
    assert_eq!(a.ui.top_bar, ["line", "-", "circle"], "Classic's list came back");
    a.run("ui.workspace.pro", json!({})).unwrap();
    assert_eq!(a.ui.top_bar, ["arc"]);
    a.run("ui.workspace.classic", json!({})).unwrap();
    assert_eq!(a.cmdline("resetui"), None);
    assert_eq!(a.ui.top_bar, topbar::default_items(), "reset puts the bar back");
}
