//! Classic docked toolbars: Draw on the left, Modify on the right, and a row under the menu with
//! the Layers combo, Dimension and Architecture buttons. The Standard toolbar is in
//! [`crate::chrome::title_and_toolbar`]. Every button starts the command it names, exactly like the
//! menu item or the command line.

use serde_json::json;

use crate::CadApp;
use crate::icons::{self, Icon};
use crate::palettes::{DRAFTING, command_known};
use crate::theme::Tokens;

type Tool = (Icon, &'static str, &'static str);

/// Width of a vertical toolbar and the size of its buttons.
const BAR: f32 = 34.0;
const BUTTON: f32 = 26.0;

/// The Architecture toolbar: the built-in architectural tools (menu: Architecture).
pub const ARCHITECTURE: &[Tool] =
    &[(Icon::Layers, "hcwlayers", "Architectural Layer Set"), (Icon::Boundary, "room", "Room Label"), (Icon::Area, "rarea", "Room Area")];

/// The tools of the named groups of [`DRAFTING`], large ones first.
fn tools(groups: &[&str]) -> Vec<Tool> {
    DRAFTING.iter().filter(|(name, _, _)| groups.contains(name)).flat_map(|(_, large, small)| large.iter().chain(small.iter()).copied()).collect()
}

fn button_row(app: &mut CadApp, ui: &mut egui::Ui, tools: &[Tool]) {
    for (icon, cmd, label) in tools {
        if command_known(cmd) && icons::button(ui, *icon, BUTTON, label, false).clicked() {
            app.start(cmd);
        }
    }
}

fn vertical(app: &mut CadApp, ui: &mut egui::Ui, id: &'static str, tools: &[Tool]) {
    egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(4.0);
            button_row(app, ui, tools);
        });
    });
}

/// The Draw toolbar (left edge).
pub fn draw_toolbar(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let list = tools(&["Draw", "Hatch", "Block", "Text"]);
    egui::Panel::left("aad_tb_draw").exact_size(BAR).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| vertical(app, ui, "draw_tb", &list));
}

/// The Modify toolbar (right edge).
pub fn modify_toolbar(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let list = tools(&["Modify"]);
    egui::Panel::right("aad_tb_modify").exact_size(BAR).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| vertical(app, ui, "modify_tb", &list));
}

/// The row under the menu: Layers, Dimension, Architecture.
pub fn top_row(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    egui::Panel::top("aad_tb_row").exact_size(34.0).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            if icons::button(ui, Icon::Layers, BUTTON, "Layers", false).clicked() {
                app.start("layer");
            }
            layer_combo(app, ui);
            ui.separator();
            button_row(app, ui, &tools(&["Dimension", "Leader"]));
            ui.separator();
            button_row(app, ui, ARCHITECTURE);
        });
    });
}

/// The current layer, with the drawing's layers to pick another.
fn layer_combo(app: &mut CadApp, ui: &mut egui::Ui) {
    let Ok(d) = app.session.doc() else { return };
    let current = d.header.str("CLAYER", "0");
    let layers: Vec<String> = d.layers.iter().map(|l| l.name.clone()).collect();
    let mut picked = None;
    let resp = egui::ComboBox::from_id_salt("aad_layer_combo").width(180.0).selected_text(current.clone()).show_ui(ui, |ui| {
        for name in &layers {
            if ui.selectable_label(*name == current, name).clicked() {
                picked = Some(name.clone());
            }
        }
    });
    crate::control::record_widget(ui, &resp.response, "Current layer", "combobox", None);
    if let Some(name) = picked {
        let _ = app.run("layer.current", json!({ "name": name }));
    }
}
