//! The interface typography preset: chosen by command, reported with what is substituted, rebuilt
//! on the next frame, and the layout does not change when the fonts do.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use egui_kittest::Harness;
use serde_json::json;

fn harness() -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.in_window_menu = false;
    app.integrated_titlebar = false;
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
fn the_preset_is_set_by_command_menu_item_or_id_and_unknown_ones_are_refused() {
    let mut h = harness();
    assert_eq!(h.state().ui.font_preset, "engineering", "IBM Plex Engineering is the default");
    let r = h.state_mut().run("ui.fonts", json!({"preset": "drafting"})).unwrap();
    assert_eq!(r["preset"], "drafting");
    assert_eq!(r["roles"].as_array().unwrap().len(), 7);
    h.run_steps(3);
    assert_eq!(h.state().ui.font_preset, "drafting");
    h.state_mut().run("ui.fonts.compact", json!({})).unwrap();
    assert_eq!(h.state().ui.font_preset, "compact");
    assert_eq!(h.state_mut().run("ui.fonts", json!({})).unwrap()["preset"], "compact", "without a preset it reports the current one");
    assert!(h.state_mut().run("ui.fonts", json!({"preset": "comic"})).is_err());
    assert_eq!(h.state().ui.font_preset, "compact", "a refused preset changes nothing");
    h.run_steps(3);
}

#[test]
fn a_preset_whose_fonts_are_not_installed_does_not_break_or_move_the_interface() {
    let mut h = harness();
    let before = h.state().canvas.rect;
    for p in ["editorial", "multilingual", "compact", "drafting", "engineering"] {
        h.state_mut().run("ui.fonts", json!({"preset": p})).unwrap();
        h.run_steps(3);
    }
    assert_eq!(h.state().canvas.rect, before, "the drawing area does not move when the font preset changes");
}

#[test]
fn compact_text_and_headings_use_the_condensed_and_heading_families_once_installed() {
    let mut h = harness();
    h.run_steps(5);
    assert!(cadcraft_ui_egui::theme::named_ready(), "the named families are used from the frame after the fonts were installed");
    assert_eq!(cadcraft_ui_egui::theme::small().family, egui::FontFamily::Name(cadcraft_ui_egui::theme::CONDENSED_FAMILY.into()));
    assert_eq!(cadcraft_ui_egui::theme::heading().family, egui::FontFamily::Name(cadcraft_ui_egui::theme::HEADING_FAMILY.into()));
    // Changing the preset keeps the families defined, so nothing drawn can ask for a missing one.
    h.state_mut().run("ui.fonts", json!({"preset": "multilingual"})).unwrap();
    h.run_steps(5);
}
