//! Double-clicking text in the drawing starts TEXTEDIT on it, and the edit lands in the drawing.
//! (egui's double-click timing is not simulated by the harness, so the test calls the handler the
//! second click runs.)

use aad_engine::Session;
use aad_engine::doc::EntityKind;
use aad_ui_egui::{CadApp, Services};
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::Harness;
use serde_json::json;

fn click(h: &mut Harness<'_, CadApp>, at: egui::Pos2) {
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
}

#[test]
fn double_click_on_text_opens_textedit_and_the_edit_updates_the_entity() {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.in_window_menu = false;
    app.run("text", json!({"at": [0, 0], "text": "ROOM", "height": 4})).unwrap();
    app.run("view.set", json!({"center": [8, 2], "height": 40})).unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 760.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    let rect = h.state().canvas.rect.expect("canvas rect");
    let at = rect.center();
    h.event(Event::PointerMoved(at));
    h.run_steps(2);
    click(&mut h, at);
    h.run_steps(2);
    // The second click of a double-click calls this with the world point under the pointer.
    let w = aad_engine::geom::Vec2::new(0.5, 1.0);
    assert!(aad_ui_egui::canvas::edit_selected_text_at(h.state_mut(), w), "TEXTEDIT starts on the selected text");
    assert_eq!(h.state().session.running.as_ref().map(|r| r.id.as_str()), Some("textedit"));
    h.state_mut().session.input(aad_engine::Input::Text("SALON".into())).unwrap();
    let d = h.state().session.doc().unwrap();
    assert!(d.model.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "SALON")));
    h.state_mut().session.cancel();
    h.state_mut().run("u", json!({})).ok();
}
