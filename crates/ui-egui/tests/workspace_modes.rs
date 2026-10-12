//! Pro and Classic workspaces: what each shows, and that switching changes nothing but visibility.

use aad_engine::Session;
use aad_ui_egui::{CadApp, Services, WorkspaceMode};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

fn harness() -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.in_window_menu = false;
    app.integrated_titlebar = false;
    app.run("line", json!({"points": [[0, 0], [10, 0]]})).unwrap();
    app.run("circle", json!({"center": [5, 5], "radius": 2})).unwrap();
    app.run("view.set", json!({"center": [5, 3], "height": 30})).unwrap();
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

#[test]
fn classic_shows_toolbars_and_panels_and_pro_hides_them_without_touching_the_drawing() {
    let mut h = harness();
    assert_eq!(h.state().ui.workspace, WorkspaceMode::Classic, "Classic is the default");
    assert!(h.state().toolbars_shown());
    assert!(has(&h, "Line"), "Draw toolbar");
    assert!(has(&h, "Properties"), "Properties panel");
    assert!(has(&h, "Layers"), "Layers panel");
    assert!(has(&h, "Blocks"), "Blocks panel");

    // Select the circle, then switch.
    let circle = h.state().session.doc().unwrap().model.iter().nth(1).unwrap().handle;
    h.state_mut().session.set_selection(vec![circle]);
    let drawing = h.state().session.doc().unwrap().clone();
    let view = h.state().session.state().unwrap().view();
    let canvas_classic = h.state().canvas.rect.unwrap();
    h.state_mut().run("ui.workspace", json!({"mode": "pro"})).unwrap();
    h.run_steps(5);
    assert!(!h.state().toolbars_shown());
    assert!(!has(&h, "Line"), "no Draw toolbar in Pro");
    assert!(!has(&h, "Blocks"), "no side panels in Pro");
    let canvas_pro = h.state().canvas.rect.unwrap();
    assert!(canvas_pro.width() > canvas_classic.width() && canvas_pro.height() > canvas_classic.height(), "Pro gives the canvas more room");
    assert_eq!(h.state().session.selection(), vec![circle], "selection survives the switch");
    assert_eq!(h.state().session.doc().unwrap(), &drawing);
    assert_eq!(h.state().session.state().unwrap().view(), view, "the view is not zoomed or panned");
    // The command line still works with everything hidden.
    h.state_mut().session.cmdline("line 0,0 3,3").unwrap();
    h.state_mut().session.cmdline("").unwrap();
    assert_eq!(h.state().session.doc().unwrap().model.len(), 3);

    h.state_mut().run("ui.workspace.classic", json!({})).unwrap();
    h.run_steps(5);
    assert!(has(&h, "Blocks"), "panels return in Classic");
    assert_eq!(h.state().session.selection(), vec![circle]);
}

#[test]
fn workspace_mode_is_saved_with_the_preferences() {
    let mut a = CadApp::new(Session::empty(), Services::default());
    a.run("ui.workspace.pro", json!({})).unwrap();
    let mut b = CadApp::new(Session::empty(), Services::default());
    b.load_prefs(&a.prefs_json());
    assert_eq!(b.ui.workspace, WorkspaceMode::Pro);
    assert!(!b.ui.show_palettes);
    assert!(a.run("ui.workspace", json!({"mode": "wide"})).is_err());
}

fn has(h: &Harness<'_, CadApp>, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

#[test]
fn unpinned_panels_collapse_away_from_the_pointer_and_return_at_the_edge_strip() {
    let mut h = harness();
    let before = h.state().canvas.rect.unwrap();
    h.state_mut().ui.pin_panels = false;
    h.state_mut().ui.auto_collapse_ms = 60;
    let circle = h.state().session.doc().unwrap().model.iter().nth(1).unwrap().handle;
    h.state_mut().session.set_selection(vec![circle]);
    let drawing = h.state().session.doc().unwrap().clone();
    // Pointer in the middle of the drawing: not over any panel.
    h.event(egui::Event::PointerMoved(before.center()));
    for _ in 0..8 {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    assert!(!h.state().ui.show_palettes, "collapsed after the delay");
    assert!(h.state().collapse.auto_collapsed);
    let after = h.state().canvas.rect.unwrap();
    assert!(after.width() > before.width(), "the drawing area grew into the freed space");
    assert_eq!(h.state().session.selection(), vec![circle]);
    assert_eq!(h.state().session.doc().unwrap(), &drawing);
    // Moving to the left edge strip opens them again.
    h.event(egui::Event::PointerMoved(egui::pos2(after.left() + 2.0, after.center().y)));
    h.run_steps(3);
    assert!(h.state().ui.show_palettes, "edge strip expands the panels");
    // Pinning keeps them open however long the pointer is away.
    h.state_mut().ui.pin_panels = true;
    h.event(egui::Event::PointerMoved(h.state().canvas.rect.unwrap().center()));
    for _ in 0..8 {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    assert!(h.state().ui.show_palettes);
}

fn layout_json(app: &CadApp) -> serde_json::Value {
    serde_json::to_value(&app.ui.docking).unwrap()
}

#[test]
fn classic_starts_pinned_with_one_tabbed_stack_on_the_right_and_pro_starts_collapsed() {
    use aad_ui_egui::docking::Panel;
    let mut a = CadApp::new(Session::empty(), Services::default());
    assert_eq!(a.ui.workspace, WorkspaceMode::Classic);
    assert!(a.ui.pin_panels, "Classic panels are pinned by default");
    let panels = a.ui.docking.layout.panels();
    for p in [Panel::Canvas, Panel::Layers, Panel::Properties, Panel::Blocks] {
        assert!(panels.contains(&&p), "{p:?} is in the default layout");
    }
    a.run("ui.workspace.pro", json!({})).unwrap();
    assert!(!a.ui.pin_panels, "Pro panels collapse and come back at the edge strip");
    assert!(!a.ui.show_palettes, "Pro opens with all panels collapsed");
    // Preferences saved by earlier builds (which pinned everywhere) still load.
    let mut b = CadApp::new(Session::empty(), Services::default());
    b.load_prefs(r#"{"pinPanels": false}"#);
    assert!(!b.ui.pin_panels);
}

#[test]
fn each_mode_keeps_its_own_arrangement_and_pin_state() {
    let mut a = CadApp::new(Session::empty(), Services::default());
    // Make Classic different from the default: float the Blocks panel and unpin.
    a.run("ui.dock", json!({"operation": "float", "panel": "blocks", "rect": [100, 100, 300, 300]})).unwrap();
    a.run("ui.dock", json!({"operation": "activate", "panel": "properties"})).unwrap();
    a.ui.pin_panels = false;
    let classic = layout_json(&a);
    assert_ne!(classic, serde_json::to_value(aad_ui_egui::docking::Workspace::default()).unwrap());

    a.run("ui.workspace.pro", json!({})).unwrap();
    assert_eq!(
        layout_json(&a),
        serde_json::to_value(aad_ui_egui::docking::Workspace::default()).unwrap(),
        "Pro starts from the default, not Classic's arrangement"
    );
    // Change Pro: pin its panels.
    a.ui.pin_panels = true;
    let pro = layout_json(&a);

    a.run("ui.workspace.classic", json!({})).unwrap();
    assert_eq!(layout_json(&a), classic, "Classic's arrangement came back untouched, including the selected tab");
    assert!(!a.ui.pin_panels, "and its own pin state");
    a.run("ui.workspace.pro", json!({})).unwrap();
    assert_eq!(layout_json(&a), pro, "Pro's arrangement is what Pro left");
    assert!(a.ui.pin_panels);
}

#[test]
fn reset_restores_the_current_mode_only_and_a_broken_saved_slot_falls_back_to_defaults() {
    let mut a = CadApp::new(Session::empty(), Services::default());
    a.run("ui.dock", json!({"operation": "float", "panel": "blocks", "rect": [100, 100, 300, 300]})).unwrap();
    a.run("ui.workspace.pro", json!({})).unwrap();
    a.ui.pin_panels = true;
    let pro = layout_json(&a);
    a.run("ui.workspace.classic", json!({})).unwrap();
    // Reset Classic: default layout and pinned again; the saved Pro slot is untouched.
    a.run("ui.workspace.reset", json!({})).unwrap();
    assert_eq!(layout_json(&a), serde_json::to_value(aad_ui_egui::docking::Workspace::default()).unwrap());
    assert!(a.ui.pin_panels && a.ui.show_palettes);
    a.run("ui.workspace.pro", json!({})).unwrap();
    assert_eq!(layout_json(&a), pro, "reset of Classic did not touch Pro");
    // A slot that no longer reads is dropped on its own.
    a.run("ui.workspace.classic", json!({})).unwrap();
    a.ui.mode_slots.insert("pro".into(), json!({"docking": {"layout": "nonsense"}, "pin": "x"}));
    a.run("ui.workspace.pro", json!({})).unwrap();
    assert_eq!(layout_json(&a), serde_json::to_value(aad_ui_egui::docking::Workspace::default()).unwrap());
    assert!(!a.ui.pin_panels);
}

#[test]
fn switching_panel_tabs_changes_neither_the_selection_nor_the_command_state() {
    let mut h = harness();
    let circle = h.state().session.doc().unwrap().model.iter().nth(1).unwrap().handle;
    h.state_mut().session.set_selection(vec![circle]);
    h.state_mut().start("line");
    h.run_steps(2);
    let running = h.state().session.running.as_ref().map(|r| r.id.clone());
    let doc = h.state().session.doc().unwrap().clone();
    for panel in ["properties", "blocks", "layers", "properties"] {
        h.state_mut().run("ui.dock", json!({"operation": "activate", "panel": panel})).unwrap();
        h.run_steps(3);
        assert_eq!(h.state().session.selection(), vec![circle], "{panel}: selection");
        assert_eq!(h.state().session.running.as_ref().map(|r| r.id.clone()), running, "{panel}: command state");
        assert_eq!(h.state().session.doc().unwrap(), &doc, "{panel}: drawing");
    }
}

#[test]
fn classic_gets_its_toolbars_back_when_entered_and_each_mode_remembers_the_toolbar_switch() {
    let mut a = CadApp::new(Session::empty(), Services::default());
    a.run("ui.workspace.pro", json!({})).unwrap();
    a.ui.show_toolbar = false; // an old saved preference, loaded while the app starts in Pro
    a.run("ui.workspace.classic", json!({})).unwrap();
    assert!(a.ui.show_toolbar && a.toolbars_shown(), "Classic shows its toolbars unless the user switched them off in Classic");
    a.ui.show_toolbar = false;
    a.run("ui.workspace.pro", json!({})).unwrap();
    a.run("ui.workspace.classic", json!({})).unwrap();
    assert!(!a.ui.show_toolbar, "a choice made in Classic is remembered for Classic");
    a.run("ui.workspace.reset", json!({})).unwrap();
    assert!(a.ui.show_toolbar, "reset turns them back on");
}

#[test]
fn resetui_typed_at_the_command_line_brings_back_menus_and_toolbars_and_keeps_theme_and_the_other_mode() {
    let mut a = CadApp::new(Session::empty(), Services::default());
    a.run("ui.workspace.pro", json!({})).unwrap();
    a.ui.pin_panels = true;
    let pro = serde_json::to_value(&a.ui.docking).unwrap();
    a.run("ui.workspace.classic", json!({})).unwrap();
    // Switch everything off, including the menu bar the reset could otherwise be found in.
    for id in ["ui.toggle.toolbar", "ui.toggle.menubar", "ui.toggle.statusbar", "ui.toggle.filetabs", "ui.toggle.cmdline"] {
        a.run(id, json!({})).unwrap();
    }
    a.run("ui.theme", json!({"theme": "light"})).ok();
    let theme = a.ui.theme;
    assert!(!a.ui.show_toolbar && !a.ui.in_window_menu && !a.ui.show_status_bar && !a.ui.show_file_tabs && !a.ui.show_command_line);
    a.ui.pin_panels = false;
    assert_eq!(a.cmdline("resetui"), None, "the command line accepts it");
    assert!(a.ui.show_toolbar && a.ui.in_window_menu && a.ui.show_status_bar && a.ui.show_file_tabs && a.ui.show_command_line, "all of it is back");
    assert!(a.ui.pin_panels && a.ui.show_palettes, "Classic's panels are pinned and open again");
    assert_eq!(a.ui.theme, theme, "the theme is a preference, not part of the layout");
    a.run("ui.workspace.pro", json!({})).unwrap();
    assert_eq!(serde_json::to_value(&a.ui.docking).unwrap(), pro, "the other mode's arrangement was not touched");
    assert!(a.ui.pin_panels);
}

#[test]
fn the_reset_shortcut_works_with_the_menu_bar_and_command_line_hidden() {
    use egui::{Event, Key, Modifiers};
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.in_window_menu = false;
    app.ui.show_toolbar = false;
    app.ui.show_command_line = false;
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    let mods = Modifiers { command: true, ctrl: true, shift: true, ..Modifiers::NONE };
    for pressed in [true, false] {
        h.event(Event::Key { key: Key::F11, physical_key: Some(Key::F11), pressed, repeat: false, modifiers: mods });
        h.run_steps(2);
    }
    assert!(h.state().ui.in_window_menu && h.state().ui.show_toolbar && h.state().ui.show_command_line, "Ctrl+Shift+F11 restored them");
    assert_eq!(h.state().ui.workspace, WorkspaceMode::Classic, "and it did not also flip the workspace (Ctrl+F11)");
}
