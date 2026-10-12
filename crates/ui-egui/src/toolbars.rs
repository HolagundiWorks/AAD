//! Classic docked toolbars: Draw and Modify side by side on the left edge. The customizable row
//! under the menu is [`crate::topbar`]. Every button starts the command it names, exactly like the
//! menu item or the command line.

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

/// The Modify toolbar, beside the Draw toolbar on the left edge: Modify, Dimension and Leader tools.
pub fn modify_toolbar(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let list = tools(&["Modify", "Dimension", "Leader"]);
    egui::Panel::left("aad_tb_modify").exact_size(BAR).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| vertical(app, ui, "modify_tb", &list));
}
