//! Pro and Classic workspaces: what each shows, and that switching changes nothing but visibility.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services, WorkspaceMode};
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
    assert!(!h.state().session.settings.gridmode && !h.state().session.settings.snapmode);

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
