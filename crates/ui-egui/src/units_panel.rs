//! The Units and Plot popup in the status bar: drawing units, display units, plot scale and paper,
//! and annotation conflicts. Every control runs an engine command (`drawing.units`,
//! `display.units`, `plot.config`, `annotation.collisions`, `annotation.layout`), so the popup has
//! no state of its own beyond what is being typed.

use egui::Id;
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

const UNITS: [&str; 5] = ["mm", "cm", "m", "in", "ft"];
const DISPLAY: [&str; 6] = ["mm", "cm", "m", "in", "ft", "ftin"];
const PAPERS: [&str; 5] = ["A0", "A1", "A2", "A3", "A4"];

fn say(app: &mut CadApp, r: Result<Value, String>) {
    match r {
        Ok(v) => {
            if let Some(w) = v.get("warning").and_then(Value::as_str) {
                app.set_status(w.to_string());
            }
        }
        Err(e) => app.set_status(e),
    }
}

/// The status-bar label: drawing unit, display unit and plot scale.
pub fn label(app: &CadApp) -> String {
    let Ok(d) = app.session.doc() else { return "Units".into() };
    let drawn = cadcraft_engine::annot::drawing_unit(d).map_or("units?", |u| u.name());
    let shown = cadcraft_engine::annot::display_unit(d).map_or(drawn, |u| u.name());
    let scale = d.header.f64("AADPLOTSCALE", 0.0);
    if scale > 0.0 { format!("{drawn} → {shown}  1:{scale}") } else { format!("{drawn} → {shown}") }
}

/// The popup body.
pub fn show(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let Ok(d) = app.session.doc() else {
        ui.label(crate::tl!("No drawing open"));
        return;
    };
    let drawn = cadcraft_engine::annot::drawing_unit(d).map_or("not set", |u| u.name()).to_string();
    let shown = cadcraft_engine::annot::display_unit(d).map_or("not set", |u| u.name()).to_string();
    let scale = d.header.f64("AADPLOTSCALE", 100.0);
    ui.set_min_width(300.0);

    ui.label(egui::RichText::new(crate::tl!("Drawing units")).strong());
    ui.label(
        egui::RichText::new(crate::tl!("How the numbers in the drawing are read. Changing them never rescales silently.")).small().color(t.text_dim),
    );
    let id = Id::new("units_pick");
    let mut pick: String = ui.data(|x| x.get_temp(id)).unwrap_or_else(|| if UNITS.contains(&drawn.as_str()) { drawn.clone() } else { "mm".into() });
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("drawing_units").selected_text(format!("{} ({})", pick, crate::tl!("now")) + &format!(" {drawn}")).show_ui(
            ui,
            |ui| {
                for u in UNITS {
                    ui.selectable_value(&mut pick, u.to_string(), u);
                }
            },
        );
    });
    ui.data_mut(|x| x.insert_temp(id, pick.clone()));
    let confirm = Id::new("units_confirm_scale");
    let confirming: bool = ui.data(|x| x.get_temp(confirm)).unwrap_or(false);
    ui.horizontal(|ui| {
        if ui
            .button(crate::tl!("Change unit only"))
            .on_hover_text(crate::tl!("Geometry is not changed; existing objects are read as the new unit"))
            .clicked()
        {
            let r = app.run("drawing.units", json!({ "units": pick, "mode": "metadata" }));
            say(app, r);
            ui.data_mut(|x| x.insert_temp(confirm, false));
        }
        let label = if confirming { crate::tl!("Confirm: rescale all objects") } else { crate::tl!("Change unit and rescale…") };
        if ui.button(label).clicked() {
            if confirming {
                let r = app.run("drawing.units", json!({ "units": pick, "mode": "scale" }));
                say(app, r);
                ui.data_mut(|x| x.insert_temp(confirm, false));
            } else {
                ui.data_mut(|x| x.insert_temp(confirm, true));
            }
        }
    });
    if confirming {
        ui.label(egui::RichText::new(crate::tl!("This multiplies every model-space coordinate. Undo restores it.")).small().color(t.warn));
    }

    ui.separator();
    ui.label(egui::RichText::new(crate::tl!("Display units")).strong());
    ui.label(egui::RichText::new(crate::tl!("How dimensions show measurements. Geometry is not changed.")).small().color(t.text_dim));
    let mut chosen = None;
    egui::ComboBox::from_id_salt("display_units").selected_text(&shown).show_ui(ui, |ui| {
        for u in DISPLAY {
            if ui.selectable_label(shown == u, u).clicked() {
                chosen = Some(u);
            }
        }
    });
    if let Some(u) = chosen {
        let r = app.run("display.units", json!({ "units": u }));
        say(app, r);
    }

    ui.separator();
    ui.label(egui::RichText::new(crate::tl!("Plot scale and paper")).strong());
    let sid = Id::new("plot_scale_text");
    let pid = Id::new("plot_paper");
    let mut text: String = ui.data(|x| x.get_temp(sid)).unwrap_or_else(|| format!("1:{scale}"));
    let mut paper: String = ui.data(|x| x.get_temp(pid)).unwrap_or_else(|| "A1".into());
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut text).desired_width(70.0));
        egui::ComboBox::from_id_salt("plot_paper_combo").selected_text(&paper).show_ui(ui, |ui| {
            for p in PAPERS {
                ui.selectable_value(&mut paper, p.to_string(), p);
            }
        });
        if ui.button(crate::tl!("Size annotation")).on_hover_text(crate::tl!("Text 2.5 mm, arrows 2.5 mm printed at this scale")).clicked() {
            let r = app.run("plot.config", json!({ "scale": text, "paper": paper }));
            say(app, r);
        }
    });
    ui.data_mut(|x| {
        x.insert_temp(sid, text);
        x.insert_temp(pid, paper);
    });
    if ui.button(crate::tl!("Check the sheet")).clicked() {
        match app.run("plot.validate", json!({})) {
            Ok(v) => {
                let w = v["warnings"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; ")).unwrap_or_default();
                app.set_status(if w.is_empty() { "The drawing fits the sheet at this scale.".to_string() } else { w });
            }
            Err(e) => app.set_status(e),
        }
    }

    ui.separator();
    ui.label(egui::RichText::new(crate::tl!("Annotation conflicts")).strong());
    ui.horizontal(|ui| {
        if ui.button(crate::tl!("Find")).clicked() {
            match app.run("annotation.collisions", json!({})) {
                Ok(v) => app.set_status(format!("{} annotation conflict(s)", v["count"])),
                Err(e) => app.set_status(e),
            }
        }
        if ui.button(crate::tl!("Resolve overlaps")).on_hover_text(crate::tl!("Moves only text; geometry stays. One undo step.")).clicked() {
            match app.run("annotation.layout", json!({ "mode": "apply" })) {
                Ok(v) => {
                    let n = v["applied"].as_u64().unwrap_or(0);
                    let left = v["unresolved"].as_array().map_or(0, Vec::len);
                    app.set_status(format!("{n} text object(s) moved; {left} could not be resolved"));
                }
                Err(e) => app.set_status(e),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_engine::Session;

    #[test]
    fn label_follows_the_drawing_display_and_plot_settings() {
        let mut app = CadApp::new(Session::new(), crate::Services::default());
        app.run("drawing.units", json!({"units": "mm", "mode": "metadata"})).unwrap();
        assert_eq!(label(&app), "mm → mm");
        app.run("display.units", json!({"units": "m"})).unwrap();
        app.run("plot.config", json!({"scale": "1:100", "paper": "A1"})).unwrap();
        assert_eq!(label(&app), "mm → m  1:100");
        // Changing only the display unit does not touch the drawing's own unit.
        assert_eq!(cadcraft_engine::annot::drawing_unit(app.session.doc().unwrap()), Some(cadcraft_engine::annot::Unit::Mm));
    }
}
