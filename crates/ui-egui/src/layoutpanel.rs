//! The Layout panel: where a sheet is designed. It is a face on the engine's sheet commands
//! (`pagesetup`, `viewport.*`, `sheet.*`, `floor.viewport`), so it holds no drawing state: it reads
//! the layout every frame and every change is a command.
//!
//! - **Sheet**: paper size and orientation of the current layout.
//! - **Viewports**: scale (typed as `1:100`), lock, which floors and layouts each one shows (in a
//!   project), remove; and adding a viewport.
//! - **Title block**: the fields of the sheet's title block, edited in place.
//! - **Saved layouts**: sheet templates (paper, border, title block) of this drawing or of the
//!   user, applied to the current layout as its title card; the current layout can be saved.

use cadcraft_doc::{EntityKind, Space};
use egui::{Id, RichText, Ui};
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

const PAPERS: [&str; 8] = ["A4", "A3", "A2", "A1", "A0", "Letter", "ANSI B", "ANSI C"];

/// What the panel remembers between frames (everything else is read from the drawing).
#[derive(Default)]
pub struct PanelState {
    /// The saved layouts; `None` until read (reading looks in the user's folder).
    templates: Option<Vec<Value>>,
    pub template_name: String,
    user_scope: bool,
    new_scale: String,
}

fn run(app: &mut CadApp, id: &str, params: Value) -> Option<Value> {
    match app.run(id, params) {
        Ok(v) => Some(v),
        Err(e) => {
            app.set_status(e);
            None
        }
    }
}

/// A one-line text field showing `current` that reports the new text once it is committed (Enter,
/// Tab or clicking away).
fn text_cell(ui: &mut Ui, id: Id, current: &str, width: f32) -> Option<String> {
    let focused = ui.memory(|m| m.has_focus(id));
    let mut buf: String = if focused { ui.data(|d| d.get_temp(id)).unwrap_or_else(|| current.to_string()) } else { current.to_string() };
    let r = ui.add(egui::TextEdit::singleline(&mut buf).id(id).desired_width(width));
    ui.data_mut(|d| d.insert_temp(id, buf.clone()));
    (r.lost_focus() && buf != current).then_some(buf)
}

fn heading(ui: &mut Ui, t: &Tokens, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).strong());
    ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), egui::Stroke::new(1.0, t.border));
    ui.add_space(3.0);
}

/// The panel body.
pub fn section(app: &mut CadApp, ui: &mut Ui) {
    let t = Tokens::get();
    let Ok(st) = app.session.state() else {
        ui.label(crate::tl!("No drawing open"));
        return;
    };
    let Space::Paper(layout) = app.session.layout_space() else {
        ui.add_space(8.0);
        ui.label(RichText::new(crate::tl!("Open a layout tab to design a sheet.")).color(t.text_dim));
        ui.add_space(6.0);
        if ui.button(crate::tl!("New layout")).clicked() {
            let _ = run(app, "layout.new", json!({}));
        }
        return;
    };
    let _ = st;
    ui.add_space(4.0);
    ui.label(RichText::new(&layout).strong().size(15.0));

    sheet_section(app, ui, &t, &layout);
    viewports_section(app, ui, &t, &layout);
    title_section(app, ui, &t, &layout);
    templates_section(app, ui, &t, &layout);
}

fn sheet_section(app: &mut CadApp, ui: &mut Ui, t: &Tokens, layout: &str) {
    heading(ui, t, crate::i18n::t("Sheet"));
    let Some(page) = app.session.doc().ok().and_then(|d| d.layout(layout)).map(|l| l.page.clone()) else { return };
    ui.horizontal(|ui| {
        ui.label(crate::tl!("Paper"));
        let mut pick: Option<&str> = None;
        egui::ComboBox::from_id_salt("layout_paper").selected_text(page.paper.split(" (").next().unwrap_or(&page.paper).to_string()).show_ui(
            ui,
            |ui| {
                for p in PAPERS {
                    if ui.selectable_label(page.paper.starts_with(p), p).clicked() {
                        pick = Some(p);
                    }
                }
            },
        );
        if let Some(p) = pick {
            let _ = run(app, "pagesetup", json!({ "paper": p }));
        }
    });
    let mut landscape = page.landscape;
    if ui.checkbox(&mut landscape, crate::tl!("Landscape")).changed() {
        let _ = run(app, "pagesetup", json!({ "landscape": landscape }));
    }
    ui.label(
        RichText::new(format!(
            "{:.0} × {:.0} mm",
            if page.landscape { page.width_mm.max(page.height_mm) } else { page.width_mm.min(page.height_mm) },
            if page.landscape { page.width_mm.min(page.height_mm) } else { page.width_mm.max(page.height_mm) }
        ))
        .small()
        .color(t.text_dim),
    );
}

fn viewports_section(app: &mut CadApp, ui: &mut Ui, t: &Tokens, layout: &str) {
    heading(ui, t, crate::i18n::t("Viewports"));
    let list = run(app, "viewport.list", json!({})).and_then(|v| v["viewports"].as_array().cloned()).unwrap_or_default();
    if list.is_empty() {
        ui.label(RichText::new(crate::tl!("No viewports on this sheet.")).small().color(t.text_dim));
    }
    for (i, v) in list.iter().enumerate() {
        let handle = v["handle"].as_str().unwrap_or("").to_string();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}", i + 1)).color(t.text_dim));
            let id = Id::new(("vp_scale", &handle));
            if let Some(new) = text_cell(ui, id, v["scaleText"].as_str().unwrap_or(""), 64.0) {
                let _ = run(app, "viewport.set", json!({ "handle": handle, "scale": new.trim() }));
            }
            let mut locked = v["locked"].as_bool().unwrap_or(false);
            if ui.checkbox(&mut locked, crate::tl!("Lock")).changed() {
                let _ = run(app, "viewport.set", json!({ "handle": handle, "locked": locked }));
            }
            if ui.small_button("✕").on_hover_text(crate::tl!("Remove this viewport")).clicked() {
                let _ = run(app, "viewport.remove", json!({ "handle": handle }));
            }
        });
        viewport_floors(app, ui, &handle, v);
    }
    ui.horizontal(|ui| {
        let scale = app.widgets.layout_panel.new_scale.clone();
        let mut buf = if scale.is_empty() { "1:100".to_string() } else { scale };
        ui.add(egui::TextEdit::singleline(&mut buf).desired_width(64.0));
        app.widgets.layout_panel.new_scale = buf.clone();
        if ui.button(crate::tl!("Add viewport")).clicked() {
            let _ = run(app, "viewport.add", json!({ "scale": buf.trim(), "layout": layout }));
        }
    });
}

/// Which floors and layouts one viewport shows: a collapsible list of check boxes that thaw or
/// freeze that entry's Xref layers in the viewport only. Only in a project.
fn viewport_floors(app: &mut CadApp, ui: &mut Ui, handle: &str, v: &Value) {
    let has_xrefs = app.session.doc().is_ok_and(|d| d.blocks.values().any(|b| b.xref_path.is_some() && !b.name.contains('|')));
    if !has_xrefs {
        return;
    }
    let frozen: Vec<String> = v["frozen"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    let response = egui::CollapsingHeader::new(RichText::new(crate::tl!("Shows")).small()).id_salt(("vp_shows", handle)).show(ui, |ui| {
        let xrefs: Vec<String> = app
            .session
            .doc()
            .map(|d| d.blocks.values().filter(|b| b.xref_path.is_some() && !b.name.contains('|')).map(|b| b.name.clone()).collect())
            .unwrap_or_default();
        for key in xrefs {
            let prefix = format!("{key}|");
            let mut shown = !frozen.iter().any(|l| l.starts_with(&prefix));
            if ui.checkbox(&mut shown, key.replace('_', " ")).changed() {
                let layers: Vec<String> = app
                    .session
                    .doc()
                    .map(|d| d.layers.iter().filter(|l| l.name.starts_with(&prefix)).map(|l| l.name.clone()).collect())
                    .unwrap_or_default();
                let params = if shown { json!({ "handle": handle, "thaw": layers }) } else { json!({ "handle": handle, "freeze": layers }) };
                let _ = run(app, "viewport.set", params);
            }
        }
    });
    let _ = response;
}

fn title_section(app: &mut CadApp, ui: &mut Ui, t: &Tokens, layout: &str) {
    heading(ui, t, crate::i18n::t("Title block"));
    let Some(r) = run(app, "sheet.fields", json!({ "layout": layout })) else { return };
    if r["titleBlock"].as_bool() != Some(true) {
        ui.label(RichText::new(crate::tl!("This sheet has no title block.")).small().color(t.text_dim));
        if ui.button(crate::tl!("Add title block")).clicked() {
            let _ = run(app, "sheet.titleblock", json!({ "layout": layout }));
        }
        return;
    }
    let fields = r["fields"].clone();
    egui::Grid::new(("title_fields", layout)).num_columns(2).spacing(egui::vec2(8.0, 4.0)).show(ui, |ui| {
        for k in r["known"].as_array().into_iter().flatten() {
            let tag = k["tag"].as_str().unwrap_or("");
            ui.label(RichText::new(crate::i18n::t(k["label"].as_str().unwrap_or(tag))).small().color(t.text_dim));
            let id = Id::new(("field", layout, tag));
            if let Some(new) = text_cell(ui, id, fields[tag].as_str().unwrap_or(""), 150.0) {
                let _ = run(app, "sheet.fields", json!({ "layout": layout, "fields": { tag: new } }));
            }
            ui.end_row();
        }
    });
}

fn templates_section(app: &mut CadApp, ui: &mut Ui, t: &Tokens, layout: &str) {
    heading(ui, t, crate::i18n::t("Saved layouts"));
    if app.widgets.layout_panel.templates.is_none() {
        let list = run(app, "sheet.template.list", json!({})).and_then(|v| v["templates"].as_array().cloned()).unwrap_or_default();
        app.widgets.layout_panel.templates = Some(list);
    }
    let templates = app.widgets.layout_panel.templates.clone().unwrap_or_default();
    if templates.is_empty() {
        ui.label(RichText::new(crate::tl!("No saved layouts yet.")).small().color(t.text_dim));
    }
    let mut changed = false;
    for tp in &templates {
        let name = tp["name"].as_str().unwrap_or("").to_string();
        let scope = tp["scope"].as_str().unwrap_or("drawing").to_string();
        ui.horizontal(|ui| {
            ui.label(&name);
            ui.label(RichText::new(if scope == "user" { crate::tl!("mine") } else { crate::tl!("this drawing") }).small().color(t.text_dim));
            if ui.small_button(crate::tl!("Apply")).on_hover_text(crate::tl!("Place it on this sheet as its title card")).clicked() {
                let _ = run(app, "sheet.template.apply", json!({ "name": name, "layout": layout }));
            }
            if ui.small_button("✕").on_hover_text(crate::tl!("Delete this saved layout")).clicked() {
                let _ = run(app, "sheet.template.delete", json!({ "name": name, "scope": scope }));
                changed = true;
            }
        });
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let mut name = app.widgets.layout_panel.template_name.clone();
        ui.add(egui::TextEdit::singleline(&mut name).desired_width(110.0).hint_text(crate::tl!("Name")));
        app.widgets.layout_panel.template_name = name.clone();
        let mut user = app.widgets.layout_panel.user_scope;
        ui.checkbox(&mut user, crate::tl!("For me"));
        app.widgets.layout_panel.user_scope = user;
        if ui
            .add_enabled(!name.trim().is_empty(), egui::Button::new(crate::tl!("Save")))
            .on_hover_text(crate::tl!("Save this sheet as a reusable layout"))
            .clicked()
        {
            let _ = run(app, "sheet.template.save", json!({ "name": name.trim(), "layout": layout, "scope": if user { "user" } else { "drawing" } }));
            changed = true;
        }
    });
    if changed {
        app.widgets.layout_panel.templates = None;
    }
}

/// Whether the active drawing's layout has a real viewport (a sheet being designed).
pub fn has_viewport(app: &CadApp, layout: &str) -> bool {
    app.session
        .doc()
        .ok()
        .and_then(|d| d.layout(layout))
        .is_some_and(|l| l.entities.iter().any(|e| matches!(&e.kind, EntityKind::Viewport(v) if v.id != 1)))
}
