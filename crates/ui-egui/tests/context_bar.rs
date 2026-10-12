//! The context bar (current layer, current floor, active drawing space) in both workspaces, driven
//! by clicks. What is asserted is the engine's state afterwards, not that something was drawn.

use aad_engine::Session;
use aad_ui_egui::{CadApp, Services, WorkspaceMode};
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

fn app(mode: WorkspaceMode) -> CadApp {
    let mut a = CadApp::new(Session::default(), Services::default());
    a.ui.in_window_menu = false;
    a.integrated_titlebar = false;
    for l in ["A-WALL", "S-COLUMN", "E-LIGHT"] {
        a.run("layer.new", json!({"name": l})).unwrap();
        a.run("layer.current", json!({"name": l})).unwrap();
        a.run("line", json!({"points": [[0, 0], [10, 0]]})).unwrap();
    }
    a.run("layer.current", json!({"name": "A-WALL"})).unwrap();
    a.run("view.set", json!({"center": [5, 3], "height": 30})).unwrap();
    a.run(if mode == WorkspaceMode::Pro { "ui.workspace.pro" } else { "ui.workspace.classic" }, json!({})).unwrap();
    a
}

fn harness(app: CadApp) -> Harness<'static, CadApp> {
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

fn has(h: &Harness<'_, CadApp>, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

fn current(h: &Harness<'_, CadApp>) -> String {
    h.state().session.doc().unwrap().header.str("CLAYER", "")
}

fn click(h: &mut Harness<'_, CadApp>, label: &str) {
    h.query_all_by_label(label).next().unwrap_or_else(|| panic!("nothing labelled `{label}`")).click();
    h.run_steps(4);
}

fn geometry(h: &Harness<'_, CadApp>) -> String {
    format!("{:?}", h.state().session.doc().unwrap().model.iter().collect::<Vec<_>>())
}

#[test]
fn both_workspaces_show_the_context_bar_and_pro_hides_the_toolbars() {
    for mode in [WorkspaceMode::Classic, WorkspaceMode::Pro] {
        let h = harness(app(mode));
        assert!(has(&h, "Layer") && has(&h, "Floor") && has(&h, "Drawing space"), "{mode:?}: the bar's three controls");
        assert!(has(&h, "A-WALL"), "{mode:?}: the current layer is named");
        assert_eq!(has(&h, "Line"), mode == WorkspaceMode::Classic, "{mode:?}: the left toolbars are Classic only");
    }
}

#[test]
fn the_layer_selector_switches_the_current_layer_filters_and_refuses_locked_layers() {
    let mut a = app(WorkspaceMode::Pro);
    a.run("layer.set", json!({"name": "S-COLUMN", "locked": true})).unwrap();
    let mut h = harness(a);
    assert!(!has(&h, "S-COLUMN"), "closed: the layer list is not shown");
    click(&mut h, "A-WALL");
    assert!(has(&h, "S-COLUMN") && has(&h, "E-LIGHT"), "the popup lists the layers");
    assert!(has(&h, "S-COLUMN") && has(&h, "E-LIGHT"));
    // The locked layer is listed but not selectable, and clicking it does not unlock it.
    let before = current(&h);
    click(&mut h, "S-COLUMN");
    assert_eq!(current(&h), before, "a locked layer cannot become current");
    assert!(h.state().session.doc().unwrap().layer("S-COLUMN").unwrap().locked, "clicking it did not unlock it");
    // Search narrows the list; clicking a valid layer makes it current at once.
    h.state_mut().ctx_bar.search = "light".into();
    h.run_steps(3);
    assert!(has(&h, "E-LIGHT") && !has(&h, "S-COLUMN"), "the list is filtered");
    click(&mut h, "E-LIGHT");
    assert_eq!(current(&h), "E-LIGHT");
}

#[test]
fn choosing_a_floor_changes_the_context_and_moves_nothing() {
    let mut a = app(WorkspaceMode::Pro);
    a.run("floors", json!({"floors": [{"name": "Ground"}, {"name": "First"}]})).unwrap();
    let mut h = harness(a);
    let geo = geometry(&h);
    let layers: Vec<_> = h.state().session.doc().unwrap().layers.iter().map(|l| (l.on, l.frozen, l.locked)).collect();
    assert_eq!(h.state().ctx_bar.floor.as_deref(), Some("Ground"), "the bar shows the first floor until another is chosen");
    h.query_all_by_role(Role::ComboBox).next().expect("the floor selector").click();
    h.run_steps(4);
    click(&mut h, "First");
    let r = h.state_mut().run("floor.current", json!({})).unwrap();
    assert_eq!(r["floor"], "First");
    h.run_steps(3);
    assert_eq!(h.state().ctx_bar.floor.as_deref(), Some("First"), "the bar follows");
    assert_eq!(geometry(&h), geo, "no object changed");
    assert_eq!(h.state().session.doc().unwrap().layers.iter().map(|l| (l.on, l.frozen, l.locked)).collect::<Vec<_>>(), layers, "no layer changed");
}

#[test]
fn saving_switching_and_marking_a_modified_drawing_space_from_the_bar() {
    let mut h = harness(app(WorkspaceMode::Pro));
    // Save the current layers as a new space through the popup and the name prompt.
    click(&mut h, "Current layers");
    click(&mut h, "Save current layers as a new space…");
    h.query_all_by_role(Role::TextInput).last().unwrap().click();
    h.query_all_by_role(Role::TextInput).last().unwrap().type_text("Working");
    h.run_steps(2);
    click(&mut h, "OK");
    let list: Value = h.state_mut().run("space.list", json!({})).unwrap();
    assert_eq!(list["spaces"][0]["name"], "Working");
    assert_eq!(list["active"], list["spaces"][0]["id"], "a new space is the active one");
    assert!(has(&h, "Working"));

    // Change a layer: the bar marks the space as modified.
    h.state_mut().run("layer.set", json!({"name": "S-COLUMN", "locked": true})).unwrap();
    h.run_steps(3);
    assert!(has(&h, "Working ●"), "modified marker");
    // A second space with that lock; switching to Working by clicking restores its saved states.
    h.state_mut().run("space.save", json!({"name": "Locked columns", "current": true, "activate": false})).unwrap();
    h.run_steps(2);
    click(&mut h, "Working ●");
    click_last(&mut h, "Working ●");
    assert!(!h.state().session.doc().unwrap().layer("S-COLUMN").unwrap().locked, "applied the saved state of Working");
    h.state_mut().run("space.activate", json!({"name": "Locked columns"})).unwrap();
    assert!(h.state().session.doc().unwrap().layer("S-COLUMN").unwrap().locked);
}

#[test]
fn the_editor_edits_the_saved_space_only_and_shows_the_engines_validation() {
    let mut a = app(WorkspaceMode::Pro);
    a.run("space.save", json!({"name": "Plan", "current": "A-WALL", "activate": true})).unwrap();
    let mut h = harness(a);
    click(&mut h, "Plan");
    click(&mut h, "Edit…");
    assert!(has(&h, "Changes here edit the saved drawing space. The drawing changes only when you apply it."));
    assert!(has(&h, "Save changes") && has(&h, "Apply saved space") && has(&h, "Save and apply to the drawing"));

    // Narrow the table to S-COLUMN, then bulk-lock what is shown: the saved space changes, the drawing does not.
    h.state_mut().ctx_bar.editor.as_mut().unwrap().search = "S-COL".into();
    h.run_steps(3);
    click(&mut h, "Lock");
    assert_eq!(h.state().session.doc().unwrap().layers.iter().filter(|l| l.locked).count(), 0, "editing the table does not change the drawing");
    click(&mut h, "Save changes");
    let r = h.state_mut().run("space.get", json!({"name": "Plan"})).unwrap();
    let locked: Vec<&str> = r["entries"].as_array().unwrap().iter().filter(|e| e["locked"] == true).map(|e| e["layer"].as_str().unwrap()).collect();
    assert_eq!(locked, ["S-COLUMN"], "only the shown row was locked and saved: {r}");
    assert_eq!(h.state().session.doc().unwrap().layers.iter().filter(|l| l.locked).count(), 0, "still not applied");
    assert!(has(&h, "Saved."));

    // Applying sets the drawing's layers from the saved space.
    click(&mut h, "Apply saved space");
    assert!(h.state().session.doc().unwrap().layer("S-COLUMN").unwrap().locked, "applied");

    // Locking the space's own current layer is refused by the engine and the message is shown, nothing saved.
    h.state_mut().ctx_bar.editor.as_mut().unwrap().search = "A-WALL".into();
    h.run_steps(3);
    click(&mut h, "Lock");
    click(&mut h, "Save changes");
    assert!(
        h.state().ctx_bar.editor.as_ref().unwrap().error.as_deref().is_some_and(|e| e.contains("current layer")),
        "{:?}",
        h.state().ctx_bar.editor.as_ref().unwrap().error
    );
    let r = h.state_mut().run("space.get", json!({"name": "Plan"})).unwrap();
    assert!(r["entries"].as_array().unwrap().iter().all(|e| e["layer"] != "A-WALL" || e["locked"] == false), "the refused change was not saved");
}

#[test]
fn big_space_changes_ask_first_and_cancel_changes_nothing() {
    let mut a = app(WorkspaceMode::Pro);
    for i in 0..30 {
        a.run("layer.new", json!({"name": format!("X{i}")})).unwrap();
    }
    let layers: Vec<Value> = (0..30).map(|i| json!({"layer": format!("X{i}"), "visible": false})).collect();
    a.run("space.save", json!({"name": "Hide many", "layers": layers, "activate": false})).unwrap();
    let mut h = harness(a);
    click(&mut h, "Current layers");
    click(&mut h, "Hide many");
    assert!(has(&h, "Apply drawing space"), "the question is asked");
    assert_eq!(h.state().session.doc().unwrap().layers.iter().filter(|l| !l.on).count(), 0, "nothing changed yet");
    click(&mut h, "Cancel");
    assert_eq!(h.state().session.doc().unwrap().layers.iter().filter(|l| !l.on).count(), 0);
    click(&mut h, "Current layers");
    click(&mut h, "Hide many");
    click(&mut h, "Apply");
    assert_eq!(h.state().session.doc().unwrap().layers.iter().filter(|l| !l.on).count(), 30);
}

fn click_last(h: &mut Harness<'_, CadApp>, label: &str) {
    h.query_all_by_label(label).last().unwrap_or_else(|| panic!("nothing labelled `{label}`")).click();
    h.run_steps(4);
}
