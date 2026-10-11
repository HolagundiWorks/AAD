//! The Plot window (File > Batch Plot): choose which drawings to plot. In a project the choices are
//! its floors and the layouts of each floor (a floor plan, beam layout, electrical layout ...), each
//! ticked when it is marked for printing and shown; and the sheets (layouts with viewports) of the
//! drawing you are in. One PDF is made for each ticked item, into the project's `Exports` folder.
//!
//! The window decides nothing: it builds the `items` and `sheets` of one `plot.batch` call, which
//! reads the saved files, skips what is hidden, frozen or not marked for printing (unless told
//! otherwise) and reports what it did.

use std::collections::BTreeMap;

use cadcraft_doc::EntityKind;
use egui::{RichText, vec2};
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

/// What the window holds while it is open.
#[derive(Default)]
pub struct PlotState {
    /// `floor.list`, when the drawing is part of a project.
    project: Option<Value>,
    /// Ticks by item key: `floor|<name>`, `layout|<floor>|<id>`, `sheet|<layout>`.
    checks: BTreeMap<String, bool>,
    sheets: Vec<String>,
    include_hidden: bool,
    folder: String,
    result: Option<Value>,
    error: Option<String>,
}

/// Open the window: read the project and tick what is marked for printing.
pub fn open(app: &mut CadApp) {
    let mut st = PlotState::default();
    if let Ok(p) = app.session.execute("floor.list", &json!({})) {
        for f in p["floors"].as_array().into_iter().flatten() {
            let name = f["name"].as_str().unwrap_or("");
            st.checks.insert(format!("floor|{name}"), printable(f));
            for l in f["layouts"].as_array().into_iter().flatten() {
                st.checks.insert(format!("layout|{name}|{}", l["id"].as_str().unwrap_or("")), printable(l));
            }
        }
        st.project = Some(p);
    }
    if let Ok(d) = app.session.doc() {
        for l in &d.layouts {
            if l.entities.iter().any(|e| matches!(&e.kind, EntityKind::Viewport(v) if v.id != 1)) {
                st.checks.insert(format!("sheet|{}", l.name), true);
                st.sheets.push(l.name.clone());
            }
        }
    }
    app.widgets.plot = st;
    app.ui.dialog = Some("plotbatch".into());
}

fn printable(entry: &Value) -> bool {
    entry["print"].as_bool().unwrap_or(true)
        && entry["visible"].as_bool().unwrap_or(true)
        && !entry["frozen"].as_bool().unwrap_or(false)
        && entry["exists"].as_bool().unwrap_or(true)
}

fn reason(entry: &Value) -> Option<String> {
    if entry["exists"].as_bool() == Some(false) {
        Some(crate::i18n::t("file missing").to_string())
    } else if entry["frozen"].as_bool() == Some(true) {
        Some(crate::i18n::t("frozen").to_string())
    } else if entry["print"].as_bool() == Some(false) {
        Some(crate::i18n::t("not for printing").to_string())
    } else if entry["visible"].as_bool() == Some(false) {
        Some(crate::i18n::t("hidden").to_string())
    } else {
        None
    }
}

fn row(ui: &mut egui::Ui, t: &Tokens, checks: &mut BTreeMap<String, bool>, key: String, label: &str, entry: &Value, indent: f32) {
    ui.horizontal(|ui| {
        ui.add_space(indent);
        let v = checks.entry(key).or_insert(false);
        ui.checkbox(v, label);
        if let Some(r) = reason(entry) {
            ui.label(RichText::new(format!("({r})")).small().color(t.text_dim));
        }
    });
}

pub fn dialog(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    let t = Tokens::get();
    let mut st = std::mem::take(&mut app.widgets.plot);
    let mut plot = false;
    egui::Window::new(crate::tl!("Batch Plot")).id(egui::Id::new("plotbatch")).open(open).collapsible(false).default_width(460.0).show(ctx, |ui| {
        if let Some(p) = st.project.clone() {
            ui.label(RichText::new(format!("{} — {}", p["project"]["name"].as_str().unwrap_or(""), crate::tl!("floors and layouts"))).strong());
            ui.horizontal(|ui| {
                if ui.small_button(crate::tl!("Select all")).clicked() {
                    st.checks.iter_mut().filter(|(k, _)| !k.starts_with("sheet|")).for_each(|(_, v)| *v = true);
                }
                if ui.small_button(crate::tl!("Select none")).clicked() {
                    st.checks.iter_mut().filter(|(k, _)| !k.starts_with("sheet|")).for_each(|(_, v)| *v = false);
                }
            });
            egui::ScrollArea::vertical().max_height(240.0).id_salt("plot_tree").show(ui, |ui| {
                for f in p["floors"].as_array().into_iter().flatten() {
                    let name = f["name"].as_str().unwrap_or("").to_string();
                    row(ui, &t, &mut st.checks, format!("floor|{name}"), &format!("{name} — {}", crate::tl!("plan")), f, 0.0);
                    for l in f["layouts"].as_array().into_iter().flatten() {
                        let id = l["id"].as_str().unwrap_or("");
                        row(ui, &t, &mut st.checks, format!("layout|{name}|{id}"), l["name"].as_str().unwrap_or(""), l, 18.0);
                    }
                }
            });
        } else {
            ui.label(RichText::new(crate::tl!("This drawing is not part of a project; only its sheets can be plotted.")).color(t.text_dim));
        }
        if !st.sheets.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(crate::tl!("Sheets of this drawing")).strong());
            for n in st.sheets.clone() {
                let v = st.checks.entry(format!("sheet|{n}")).or_insert(true);
                ui.checkbox(v, &n);
            }
        }
        ui.add_space(6.0);
        ui.checkbox(&mut st.include_hidden, crate::tl!("Also plot hidden, frozen and unmarked items that are ticked"));
        ui.horizontal(|ui| {
            ui.label(crate::tl!("Folder"));
            ui.add(egui::TextEdit::singleline(&mut st.folder).desired_width(300.0).hint_text(crate::tl!("Exports in the project folder")));
        });
        ui.add_space(4.0);
        let any = st.checks.values().any(|v| *v);
        if ui.add_enabled(any, egui::Button::new(crate::tl!("Plot"))).clicked() {
            plot = true;
        }
        if let Some(e) = &st.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        if let Some(r) = &st.result {
            ui.separator();
            ui.label(RichText::new(r["message"].as_str().unwrap_or("")).strong());
            for f in r["files"].as_array().into_iter().flatten() {
                ui.label(RichText::new(format!("{}  →  {}", f["item"].as_str().unwrap_or(""), f["path"].as_str().unwrap_or(""))).small());
            }
            for k in r["skipped"].as_array().into_iter().flatten() {
                ui.label(
                    RichText::new(format!("{}: {}", k["item"].as_str().unwrap_or(""), k["reason"].as_str().unwrap_or(""))).small().color(t.text_dim),
                );
            }
            for w in r["warnings"].as_array().into_iter().flatten() {
                ui.label(RichText::new(w.as_str().unwrap_or("")).small().color(ui.visuals().warn_fg_color));
            }
        }
        let _ = vec2(0.0, 0.0);
    });
    if plot {
        let mut items = Vec::new();
        let mut sheets = Vec::new();
        for (k, on) in &st.checks {
            if !*on {
                continue;
            }
            let parts: Vec<&str> = k.split('|').collect();
            match parts.as_slice() {
                ["floor", f] => items.push(json!({ "floor": f })),
                ["layout", f, c] => items.push(json!({ "floor": f, "category": c })),
                ["sheet", n] => sheets.push(json!(n)),
                _ => {}
            }
        }
        let mut params = json!({ "includeHidden": st.include_hidden, "sheets": sheets });
        if !items.is_empty() {
            params["items"] = Value::Array(items);
        }
        if !st.folder.trim().is_empty() {
            params["folder"] = json!(st.folder.trim());
        }
        match app.run("plot.batch", params) {
            Ok(r) => {
                app.set_status(r["message"].as_str().unwrap_or("Plotted").to_string());
                st.error = None;
                st.result = Some(r);
            }
            Err(e) => {
                st.error = Some(e);
                st.result = None;
            }
        }
    }
    app.widgets.plot = st;
}
