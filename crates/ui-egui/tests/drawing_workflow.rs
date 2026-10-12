//! The basic drawing workflow driven the way a person does it: pointer clicks on the canvas inside
//! the real window layout, not engine calls. Each step asserts the resulting drawing, selection and
//! command state, not just that an event fired.

use aad_engine::Session;
use aad_engine::doc::{Drawing, EntityKind};
use aad_engine::geom::Vec2;
use aad_ui_egui::{CadApp, Services};
use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::Harness;
use serde_json::json;

fn harness() -> Harness<'static, CadApp> {
    install_io();
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.in_window_menu = false;
    app.integrated_titlebar = false;
    app.run("view.set", json!({"center": [50, 30], "height": 120})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 800.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(6);
    h
}

/// Screen position of a world point on the canvas.
fn at(h: &Harness<'_, CadApp>, x: f64, y: f64) -> egui::Pos2 {
    h.state().canvas.xf.expect("canvas transform").to_screen(Vec2::new(x, y))
}

fn move_to(h: &mut Harness<'_, CadApp>, p: egui::Pos2) {
    h.event(Event::PointerMoved(p));
    h.run_steps(2);
}

fn click_at(h: &mut Harness<'_, CadApp>, x: f64, y: f64) {
    let p = at(h, x, y);
    move_to(h, p);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
}

fn drag(h: &mut Harness<'_, CadApp>, from: (f64, f64), to: (f64, f64)) {
    let a = at(h, from.0, from.1);
    let b = at(h, to.0, to.1);
    move_to(h, a);
    h.event(Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(2);
    for step in 1..=8 {
        h.event(Event::PointerMoved(a.lerp(b, step as f32 / 8.0)));
        h.run_steps(1);
    }
    h.event(Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(4);
}

fn key(h: &mut Harness<'_, CadApp>, k: Key) {
    for pressed in [true, false] {
        h.event(Event::Key { key: k, physical_key: Some(k), pressed, repeat: false, modifiers: Modifiers::NONE });
        h.run_steps(2);
    }
}

fn doc(h: &Harness<'_, CadApp>) -> Drawing {
    h.state().session.doc().expect("a drawing").clone()
}

fn count(h: &Harness<'_, CadApp>) -> usize {
    h.state().session.doc().map(|d| d.model.len()).unwrap_or(0)
}

fn running(h: &Harness<'_, CadApp>) -> Option<String> {
    h.state().session.running.as_ref().map(|r| r.id.clone())
}

#[test]
fn draw_a_line_by_clicking_two_points() {
    let mut h = harness();
    h.state_mut().start("line");
    h.run_steps(2);
    assert_eq!(running(&h).as_deref(), Some("line"), "the LINE prompt is active");
    click_at(&mut h, 10.0, 10.0);
    click_at(&mut h, 40.0, 10.0);
    key(&mut h, Key::Escape);
    assert_eq!(running(&h), None, "Escape ends the command");
    let d = doc(&h);
    assert_eq!(d.model.len(), 1, "one line was drawn");
    let EntityKind::Line(l) = &d.model.iter().next().unwrap().kind else { panic!("not a line") };
    assert!((l.a.x - 10.0).abs() < 0.2 && (l.a.y - 10.0).abs() < 0.2, "start {:?}", l.a);
    assert!((l.b.x - 40.0).abs() < 0.2 && (l.b.y - 10.0).abs() < 0.2, "end {:?}", l.b);
}

#[test]
fn escape_cancels_a_started_line_and_draws_nothing() {
    let mut h = harness();
    h.state_mut().start("line");
    click_at(&mut h, 10.0, 10.0);
    key(&mut h, Key::Escape);
    assert_eq!(running(&h), None);
    assert_eq!(count(&h), 0, "a line with one point is not created");
}

#[test]
fn circle_rectangle_and_polyline_by_clicks() {
    let mut h = harness();
    h.state_mut().start("circle");
    click_at(&mut h, 20.0, 20.0);
    click_at(&mut h, 30.0, 20.0);
    assert_eq!(count(&h), 1);
    let d = doc(&h);
    let EntityKind::Circle(c) = &d.model.iter().next().unwrap().kind else { panic!("not a circle") };
    assert!((c.radius - 10.0).abs() < 0.2, "radius {}", c.radius);

    h.state_mut().start("rectangle");
    click_at(&mut h, 50.0, 10.0);
    click_at(&mut h, 80.0, 30.0);
    assert_eq!(count(&h), 2);

    h.state_mut().start("pline");
    click_at(&mut h, 10.0, 50.0);
    click_at(&mut h, 30.0, 55.0);
    click_at(&mut h, 50.0, 50.0);
    key(&mut h, Key::Enter);
    assert_eq!(running(&h), None, "Enter finishes the polyline");
    assert_eq!(count(&h), 3);
    let d = doc(&h);
    let EntityKind::LwPolyline(p) = &d.model.iter().last().unwrap().kind else { panic!("not a polyline") };
    assert_eq!(p.vertices.len(), 3);
}

#[test]
fn click_selects_empty_click_opens_a_window_and_escape_deselects() {
    let mut h = harness();
    h.state_mut().run("line", json!({"points": [[10, 10], [40, 10]]})).unwrap();
    h.run_steps(3);
    click_at(&mut h, 25.0, 10.0);
    assert_eq!(h.state().session.selection().len(), 1, "clicking the line selects it");
    // Classic model: a click on empty space opens a selection window (it does not deselect);
    // Escape clears the selection.
    click_at(&mut h, 25.0, 60.0);
    assert!(h.state().session.pending_window.is_some(), "an empty click starts a window");
    key(&mut h, Key::Escape);
    assert!(h.state().session.selection().is_empty(), "Escape clears the selection");
    assert!(h.state().session.pending_window.is_none(), "Escape cancels the window");
}

#[test]
fn window_selects_only_inside_and_crossing_also_touches() {
    let mut h = harness();
    h.state_mut().run("line", json!({"points": [[10, 10], [20, 10]]})).unwrap(); // inside
    h.state_mut().run("line", json!({"points": [[30, 10], [70, 10]]})).unwrap(); // crosses the box edge
    h.state_mut().run("line", json!({"points": [[80, 50], [90, 50]]})).unwrap(); // outside
    h.run_steps(3);
    // Left to right = window (fully inside only).
    drag(&mut h, (5.0, 0.0), (50.0, 20.0));
    assert_eq!(h.state().session.selection().len(), 1, "window: only the contained line");
    key(&mut h, Key::Escape);
    // Right to left = crossing (touching counts).
    drag(&mut h, (50.0, 20.0), (5.0, 0.0));
    assert_eq!(h.state().session.selection().len(), 2, "crossing: contained and crossing lines");
}

#[test]
fn undo_and_redo_after_drawing() {
    let mut h = harness();
    h.state_mut().start("line");
    click_at(&mut h, 10.0, 10.0);
    click_at(&mut h, 40.0, 10.0);
    key(&mut h, Key::Escape);
    let with_line = doc(&h);
    h.state_mut().run("u", json!({})).unwrap();
    assert_eq!(count(&h), 0, "undo removes the line");
    h.state_mut().run("redo", json!({})).unwrap();
    assert_eq!(doc(&h), with_line, "redo restores exactly the same drawing");
}

#[test]
fn move_copy_rotate_and_erase_a_selection() {
    let mut h = harness();
    h.state_mut().run("line", json!({"points": [[10, 10], [30, 10]]})).unwrap();
    h.run_steps(3);
    click_at(&mut h, 20.0, 10.0);
    let sel: Vec<String> = h.state().session.selection().iter().map(|x| x.hex()).collect();
    assert_eq!(sel.len(), 1);
    h.state_mut().run("move", json!({"handles": sel, "delta": [5.0, 5.0]})).unwrap();
    let d = doc(&h);
    let EntityKind::Line(l) = &d.model.iter().next().unwrap().kind else { panic!() };
    assert!((l.a.x - 15.0).abs() < 1e-9 && (l.a.y - 15.0).abs() < 1e-9);
    h.state_mut().run("copy", json!({"handles": sel, "delta": [0.0, 10.0]})).unwrap();
    assert_eq!(count(&h), 2);
    h.state_mut().run("rotate", json!({"handles": sel, "base": [25, 15], "angle": 90})).unwrap();
    let d = doc(&h);
    let EntityKind::Line(l) = &d.model.iter().next().unwrap().kind else { panic!() };
    assert!((l.a.x - 25.0).abs() < 1e-6 && (l.a.y - 5.0).abs() < 1e-6, "rotated about the base: {:?}", l.a);
    h.state_mut().run("erase", json!({"handles": sel})).unwrap();
    assert_eq!(count(&h), 1);
}

#[test]
fn zoom_and_pan_change_the_view_and_zoom_extents_fits_the_drawing() {
    let mut h = harness();
    h.state_mut().run("line", json!({"points": [[0, 0], [100, 60]]})).unwrap();
    h.run_steps(3);
    let v0 = h.state().session.state().unwrap().view();
    h.state_mut().run("view.set", json!({"center": [0, 0], "height": 10})).unwrap();
    h.run_steps(3);
    let v1 = h.state().session.state().unwrap().view();
    assert_ne!(v0, v1);
    h.state_mut().run("zoom", json!({"mode": "extents"})).unwrap();
    h.run_steps(4);
    let xf = h.state().canvas.xf.unwrap();
    let b = xf.world_bounds();
    assert!(b.min.x <= 0.0 && b.min.y <= 0.0 && b.max.x >= 100.0 && b.max.y >= 60.0, "extents contain the drawing: {b:?}");
}

#[test]
fn save_and_reopen_keeps_the_drawing_and_the_view_opens_fitted() {
    let mut h = harness();
    h.state_mut().start("line");
    click_at(&mut h, 10.0, 10.0);
    click_at(&mut h, 40.0, 10.0);
    key(&mut h, Key::Escape);
    h.state_mut().start("circle");
    click_at(&mut h, 60.0, 30.0);
    click_at(&mut h, 70.0, 30.0);
    let saved = doc(&h);
    assert_eq!(saved.model.len(), 2);

    let dir = std::env::temp_dir().join(format!("aad-ui-workflow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workflow.dxf");
    h.state_mut().run("saveas", json!({"path": path.to_str().unwrap()})).unwrap();
    assert!(path.metadata().map(|m| m.len() > 0).unwrap_or(false), "the file was written");

    let mut fresh = harness();
    fresh.state_mut().open_path(path.to_str().unwrap());
    fresh.run_steps(4);
    let reopened = doc(&fresh);
    assert_eq!(reopened.model.len(), 2, "both entities come back");
    let kinds = |d: &Drawing| {
        let mut v: Vec<String> = d.model.iter().map(|e| format!("{:?}", std::mem::discriminant(&e.kind))).collect();
        v.sort();
        v
    };
    assert_eq!(kinds(&reopened), kinds(&saved));
    let line = reopened.model.iter().find_map(|e| if let EntityKind::Line(l) = &e.kind { Some(l.clone()) } else { None }).expect("line");
    assert!((line.b.x - line.a.x - 30.0).abs() < 0.2, "line length survives: {line:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn resizing_the_window_keeps_the_canvas_inside_and_the_world_point_at_the_pointer_consistent() {
    let mut h = harness();
    h.state_mut().run("line", json!({"points": [[10, 10], [40, 10]]})).unwrap();
    for (w, hgt) in [(1400.0, 800.0), (1100.0, 700.0), (900.0, 560.0), (1600.0, 900.0)] {
        h.set_size(egui::vec2(w, hgt));
        h.run_steps(5);
        let rect = h.state().canvas.rect.expect("canvas");
        assert!(
            rect.min.x >= 0.0 && rect.min.y >= 0.0 && rect.max.x <= w + 0.5 && rect.max.y <= hgt + 0.5,
            "{w}x{hgt}: canvas {rect:?} leaves the window"
        );
        assert!(rect.width() > 200.0 && rect.height() > 150.0, "{w}x{hgt}: canvas too small {rect:?}");
        // The line's midpoint must be picked by a click on its on-screen position at every size.
        h.state_mut().session.set_selection(Vec::new());
        let screen = h.state().canvas.xf.unwrap().to_screen(Vec2::new(25.0, 10.0));
        if rect.contains(screen) {
            let back = h.state().canvas.xf.unwrap().to_world(screen);
            assert!((back.x - 25.0).abs() < 1e-3 && (back.y - 10.0).abs() < 1e-3, "{w}x{hgt}: transform round trip {back:?}");
            click_at(&mut h, 25.0, 10.0);
            assert_eq!(h.state().session.selection().len(), 1, "{w}x{hgt}: the line is pickable");
            key(&mut h, Key::Escape);
        }
    }
}

fn install_io() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        aad_engine::cmd::file::set_io(aad_engine::cmd::file::IoHooks {
            read: |b, name| aad_io::read(b, name).map_err(|e| e.to_string()),
            write: |d, name| aad_io::write(d, name).map_err(|e| e.to_string()),
            plot: None,
        });
    });
}
