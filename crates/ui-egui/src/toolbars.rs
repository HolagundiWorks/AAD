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
            property_controls(app, ui);
            ui.separator();
            button_row(app, ui, &tools(&["Dimension", "Leader"]));
            ui.separator();
            button_row(app, ui, ARCHITECTURE);
        });
    });
}

/// One property across the selection.
#[derive(Clone, Debug, PartialEq)]
enum Prop {
    Same(String),
    Mixed,
}

impl Prop {
    fn merge(slot: &mut Option<Prop>, v: String) {
        *slot = Some(match slot.take() {
            None => Prop::Same(v),
            Some(Prop::Same(old)) if old == v => Prop::Same(old),
            Some(_) => Prop::Mixed,
        });
    }
    fn text(&self) -> String {
        match self {
            Prop::Same(s) => s.clone(),
            Prop::Mixed => "*Varies*".into(),
        }
    }
}

/// Layer, colour, linetype and lineweight of the selection (mixed values reported), or of the
/// drawing's current settings when nothing is selected. The flag says whether it is a selection.
fn current_props(app: &CadApp) -> Option<([Prop; 4], bool)> {
    let st = app.session.state().ok()?;
    let d = &st.doc;
    let sel: Vec<_> = st.selection.iter().filter_map(|h| d.entity(*h)).collect();
    if sel.is_empty() {
        let c = app.session.current_common();
        let lw = lineweight_name(c.lineweight);
        return Some(([Prop::Same(c.layer), Prop::Same(c.color.name()), Prop::Same(c.linetype), Prop::Same(lw)], false));
    }
    let mut slots: [Option<Prop>; 4] = [None, None, None, None];
    for e in sel {
        Prop::merge(&mut slots[0], e.common.layer.clone());
        Prop::merge(&mut slots[1], e.common.color.name());
        Prop::merge(&mut slots[2], e.common.linetype.clone());
        Prop::merge(&mut slots[3], lineweight_name(e.common.lineweight));
    }
    let [a, b, c, d] = slots;
    Some(([a?, b?, c?, d?], true))
}

fn lineweight_name(w: cadcraft_doc::Lineweight) -> String {
    match w {
        cadcraft_doc::Lineweight::ByLayer => "ByLayer".into(),
        cadcraft_doc::Lineweight::ByBlock => "ByBlock".into(),
        cadcraft_doc::Lineweight::Default => "Default".into(),
        cadcraft_doc::Lineweight::Mm100(v) => format!("{:.2} mm", f64::from(v) / 100.0),
    }
}

/// The colours offered directly: inheritance modes and the seven standard colours.
const COLORS: &[&str] = &["ByLayer", "ByBlock", "Red", "Yellow", "Green", "Cyan", "Blue", "Magenta", "White"];

fn swatch(ui: &mut egui::Ui, name: &str, layer_color: Option<cadcraft_color::Color>) {
    let color = cadcraft_color::Color::parse(name).or(layer_color).unwrap_or(cadcraft_color::Color::Index(7));
    let rgb = color.resolve(cadcraft_color::Color::Index(7), cadcraft_color::Color::Index(7));
    let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 1.0, egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2));
    ui.painter().rect_stroke(r, 1.0, egui::Stroke::new(1.0, egui::Color32::from_gray(90)), egui::StrokeKind::Inside);
}

/// A grid of the 255 indexed colours; returns the one clicked.
fn aci_grid(ui: &mut egui::Ui) -> Option<u8> {
    let mut picked = None;
    ui.spacing_mut().item_spacing = egui::vec2(1.0, 1.0);
    for row in 0..16u8 {
        ui.horizontal(|ui| {
            for col in 0..16u8 {
                let i = row * 16 + col;
                if i == 0 {
                    ui.add_space(14.0);
                    continue;
                }
                let rgb = cadcraft_color::aci_rgb(i);
                let (r, resp) = ui.allocate_exact_size(egui::vec2(13.0, 13.0), egui::Sense::click());
                ui.painter().rect_filled(r, 0.0, egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2));
                if resp.on_hover_text(format!("{i}")).clicked() {
                    picked = Some(i);
                    ui.close();
                }
            }
        });
    }
    picked
}

/// A dropdown that returns the picked item.
fn dropdown(ui: &mut egui::Ui, id: &str, width: f32, shown: &str, items: &[String], colors: bool) -> Option<String> {
    let mut picked = None;
    let resp = egui::ComboBox::from_id_salt(id).width(width).selected_text(shown).show_ui(ui, |ui| {
        for item in items {
            ui.horizontal(|ui| {
                if colors {
                    swatch(ui, item, None);
                }
                if ui.selectable_label(item == shown, item).clicked() {
                    picked = Some(item.clone());
                }
            });
        }
    });
    crate::control::record_widget(ui, &resp.response, id, "combobox", None);
    picked
}

/// Layer, Colour, Linetype and Lineweight dropdowns. They show and change the selection's
/// properties through `properties.set` (objects on locked layers are left alone and the reason is
/// reported); with nothing selected they change the settings new objects get.
fn property_controls(app: &mut CadApp, ui: &mut egui::Ui) {
    let Some((props, selection)) = current_props(app) else { return };
    let Ok(d) = app.session.doc() else { return };
    let layers: Vec<String> = d.layers.iter().map(|l| l.name.clone()).collect();
    let layer_color = d.layer(&props[0].text()).map(|l| l.color);
    let mut linetypes: Vec<String> = vec!["ByLayer".into(), "ByBlock".into()];
    linetypes.extend(d.linetypes.iter().map(|l| l.name.clone()));
    let weights: Vec<String> = ["ByLayer", "ByBlock", "Default"]
        .iter()
        .map(|s| (*s).to_string())
        .chain(cadcraft_doc::Lineweight::STANDARD.iter().map(|v| format!("{:.2} mm", f64::from(*v) / 100.0)))
        .collect();
    let colors: Vec<String> = COLORS.iter().map(|s| (*s).to_string()).collect();
    let hexes: Vec<String> = app.session.selection().iter().map(|h| h.hex()).collect();

    swatch(ui, "", layer_color);
    let layer = dropdown(ui, "Layer", 150.0, &props[0].text(), &layers, false);
    let mut color = dropdown(ui, "Colour", 90.0, &props[1].text(), &colors, true);
    let more = ui.button("…").on_hover_text(crate::tl!("All 255 indexed colours"));
    egui::Popup::from_toggle_button_response(&more).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        if let Some(i) = aci_grid(ui) {
            color = Some(i.to_string());
        }
    });
    let linetype = dropdown(ui, "Linetype", 100.0, &props[2].text(), &linetypes, false);
    let weight = dropdown(ui, "Lineweight", 90.0, &props[3].text(), &weights, false);

    let report = |app: &mut CadApp, r: Result<serde_json::Value, String>| match r {
        Ok(v) if v["skippedLocked"].as_u64().unwrap_or(0) > 0 => {
            app.set_status(format!("{} object(s) on locked layers were not changed", v["skippedLocked"]));
        }
        Ok(_) => {}
        Err(e) => app.set_status(e),
    };
    let weight_mm = |s: &str| -> serde_json::Value {
        match s.trim_end_matches(" mm").parse::<f64>() {
            Ok(v) => json!(v),
            Err(_) => json!(s),
        }
    };
    if selection {
        for (key, value) in [
            ("layer", layer.map(|v| json!(v))),
            ("color", color.map(|v| json!(v))),
            ("linetype", linetype.map(|v| json!(v))),
            ("lineweight", weight.as_deref().map(weight_mm)),
        ] {
            if let Some(v) = value {
                let r = app.run("properties.set", json!({ "handles": hexes, key: v }));
                report(app, r);
            }
        }
    } else {
        if let Some(name) = layer {
            let r = app.run("layer.current", json!({ "name": name }));
            report(app, r);
        }
        if let Some(c) = color {
            let r = app.run("color", json!({ "color": c }));
            report(app, r);
        }
        if let Some(l) = linetype {
            let r = app.run("linetype", json!({ "current": l }));
            report(app, r);
        }
        if let Some(w) = weight {
            let r = app.run("lweight", json!({ "lineweight": weight_mm(&w) }));
            report(app, r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_engine::Session;
    use serde_json::json;

    fn app() -> CadApp {
        let mut app = CadApp::new(Session::new(), crate::Services::default());
        app.run("line", json!({"points": [[0, 0], [5, 0]]})).unwrap();
        app.run("line", json!({"points": [[0, 1], [5, 1]]})).unwrap();
        app
    }

    #[test]
    fn nothing_selected_shows_the_current_settings_and_selection_shows_mixed_values() {
        let mut app = app();
        let (p, selection) = current_props(&app).unwrap();
        assert!(!selection);
        assert_eq!(p[0], Prop::Same("0".into()));
        assert_eq!(p[1], Prop::Same("ByLayer".into()));
        let hs: Vec<_> = app.session.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
        app.run("properties.set", json!({"handles": [hs[0]], "color": "red"})).unwrap();
        app.run("select", json!({"handles": hs})).unwrap();
        let (p, selection) = current_props(&app).unwrap();
        assert!(selection);
        assert_eq!(p[1], Prop::Mixed, "different colours across the selection");
        assert_eq!(p[0], Prop::Same("0".into()), "same layer across the selection");
        assert_eq!(p[1].text(), "*Varies*");
        app.run("select", json!({"handles": [hs[0]]})).unwrap();
        assert_eq!(current_props(&app).unwrap().0[1], Prop::Same(cadcraft_color::Color::Index(1).name()));
    }
}
