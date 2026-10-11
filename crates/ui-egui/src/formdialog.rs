//! The input dialog for commands that take settings: project details, floors, rooms, walls,
//! stairs and the rest of the architecture commands. It is drawn from the engine's form
//! declaration (`cadcraft_engine::forms`), so it knows nothing about any one command. OK validates
//! the settings, then either runs the command (nothing to pick) or starts it with the settings
//! answered so only the points are picked on the drawing.

use std::collections::BTreeMap;

use cadcraft_engine::forms::{self, Field, Form, Kind};
use egui::{RichText, vec2};
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

/// What the open dialog holds: the text of every field, by key.
pub struct State {
    form: &'static Form,
    scalars: BTreeMap<&'static str, String>,
    bools: BTreeMap<&'static str, bool>,
    tables: BTreeMap<&'static str, Vec<Vec<String>>>,
    error: Option<String>,
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

impl State {
    fn new(app: &CadApp, form: &'static Form) -> State {
        let pre = forms::prefill(&app.session, form);
        let mut st = State { form, scalars: BTreeMap::new(), bools: BTreeMap::new(), tables: BTreeMap::new(), error: None };
        for f in form.dialog_fields() {
            let v = pre.get(f.key).cloned().unwrap_or(Value::Null);
            match f.kind {
                Kind::Bool => {
                    st.bools.insert(f.key, v.as_bool().unwrap_or(false));
                }
                Kind::Table(cols) => {
                    let mut rows: Vec<Vec<String>> = v
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|r| {
                            cols.iter()
                                .enumerate()
                                .map(|(i, c)| text_of(&if r.is_array() { r[i].clone() } else { r.get(c.key).cloned().unwrap_or(Value::Null) }))
                                .collect()
                        })
                        .collect();
                    if rows.is_empty() {
                        rows.push(vec![String::new(); cols.len()]);
                    }
                    st.tables.insert(f.key, rows);
                }
                Kind::Numbers => {
                    let t = match &v {
                        Value::Array(a) => a.iter().map(text_of).collect::<Vec<_>>().join(", "),
                        other => text_of(other),
                    };
                    st.scalars.insert(f.key, t);
                }
                _ => {
                    st.scalars.insert(f.key, text_of(&v));
                }
            }
        }
        st
    }

    /// The collected values in the shape `forms::parse_input` takes.
    fn input(&self) -> Value {
        let mut o = serde_json::Map::new();
        for (k, v) in &self.scalars {
            o.insert((*k).into(), json!(v));
        }
        for (k, v) in &self.bools {
            o.insert((*k).into(), json!(v));
        }
        for (k, rows) in &self.tables {
            o.insert((*k).into(), json!(rows));
        }
        Value::Object(o)
    }
}

/// Open the dialog for `name` if it has one. `false` means the command starts the usual way.
pub fn open(app: &mut CadApp, name: &str) -> bool {
    let id = name.trim().trim_start_matches(['_', '.', '\'']).to_ascii_lowercase();
    let Some(form) = forms::get(&id) else { return false };
    if form.dialog_fields().next().is_none() {
        return false;
    }
    app.widgets.form = Some(State::new(app, form));
    true
}

fn row(ui: &mut egui::Ui, t: &Tokens, f: &Field, st: &mut State) {
    let label = crate::i18n::t(f.label);
    match f.kind {
        Kind::Bool => {
            let v = st.bools.entry(f.key).or_default();
            ui.label("");
            ui.checkbox(v, label);
        }
        Kind::Choice(list) => {
            ui.label(label);
            let v = st.scalars.entry(f.key).or_default();
            let shown = if v.is_empty() { crate::tl!("(default)").to_string() } else { v.clone() };
            egui::ComboBox::from_id_salt(("form_choice", f.key)).selected_text(shown).show_ui(ui, |ui| {
                for c in list {
                    let text = if c.is_empty() { crate::tl!("(default)").to_string() } else { (*c).to_string() };
                    ui.selectable_value(v, (*c).to_string(), text);
                }
            });
        }
        Kind::RoomType => {
            ui.label(label);
            let v = st.scalars.entry(f.key).or_default();
            let shown = if v.is_empty() { crate::tl!("Choose...").to_string() } else { v.clone() };
            egui::ComboBox::from_id_salt(("form_room", f.key)).selected_text(shown).height(320.0).show_ui(ui, |ui| {
                for (_, name) in forms::room_types() {
                    ui.selectable_value(v, name.to_string(), name);
                }
            });
        }
        Kind::Table(_) => {}
        _ => {
            ui.label(label);
            let v = st.scalars.entry(f.key).or_default();
            let mut te = egui::TextEdit::singleline(v).desired_width(220.0);
            if !f.help.is_empty() {
                te = te.hint_text(f.help);
            }
            ui.add(te);
        }
    }
    let _ = t;
    ui.end_row();
}

fn table(ui: &mut egui::Ui, f: &Field, st: &mut State) {
    let Kind::Table(cols) = f.kind else { return };
    ui.label(RichText::new(crate::i18n::t(f.label)).strong());
    let rows = st.tables.entry(f.key).or_default();
    egui::ScrollArea::both().max_height(280.0).id_salt(("form_table", f.key)).show(ui, |ui| {
        egui::Grid::new(("form_grid", f.key)).num_columns(cols.len()).spacing(vec2(6.0, 4.0)).show(ui, |ui| {
            for c in cols {
                ui.label(RichText::new(crate::i18n::t(c.label)).small());
            }
            ui.end_row();
            for r in rows.iter_mut() {
                for (i, cell) in r.iter_mut().enumerate() {
                    let w = if cols.get(i).is_some_and(|c| c.numeric) { 70.0 } else { 110.0 };
                    ui.add(egui::TextEdit::singleline(cell).desired_width(w));
                }
                ui.end_row();
            }
        });
    });
    ui.horizontal(|ui| {
        if ui.small_button(crate::tl!("Add row")).clicked() {
            rows.push(vec![String::new(); cols.len()]);
        }
        if ui.small_button(crate::tl!("Remove last row")).clicked() && rows.len() > 1 {
            rows.pop();
        }
    });
}

/// Draw the dialog if one is open.
pub fn show(app: &mut CadApp, ctx: &egui::Context) {
    let t = Tokens::get();
    let Some(mut st) = app.widgets.form.take() else { return };
    let form = st.form;
    let mut open = true;
    let mut ok = false;
    let mut cancel = false;
    egui::Window::new(crate::i18n::t(form.title))
        .id(egui::Id::new(("form_dialog", form.id)))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(440.0)
        .show(ctx, |ui| {
            let scalar_fields: Vec<&Field> = form.dialog_fields().filter(|f| !matches!(f.kind, Kind::Table(_))).collect();
            if !scalar_fields.is_empty() {
                egui::Grid::new(("form_fields", form.id)).num_columns(2).spacing(vec2(10.0, 6.0)).show(ui, |ui| {
                    for f in &scalar_fields {
                        row(ui, &t, f, &mut st);
                    }
                });
            }
            for f in form.dialog_fields().filter(|f| matches!(f.kind, Kind::Table(_))) {
                table(ui, f, &mut st);
            }
            if form.has_picks() {
                ui.add_space(4.0);
                ui.label(RichText::new(crate::tl!("After OK, pick the points on the drawing.")).small().color(t.text_dim));
            }
            if let Some(e) = &st.error {
                ui.add_space(4.0);
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(crate::tl!("OK")).clicked() {
                    ok = true;
                }
                if ui.button(crate::tl!("Cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    if ok {
        let input = st.input();
        match forms::parse_settings(form, &input) {
            Err(e) => st.error = Some(e.to_string()),
            Ok(settings) => {
                if form.has_picks() {
                    match app.session.start_form(form.id, input) {
                        Ok(()) => return,
                        Err(e) => st.error = Some(e.to_string()),
                    }
                } else {
                    match app.run(form.id, settings) {
                        Ok(r) => {
                            if let Some(m) = r.get("message").and_then(Value::as_str) {
                                app.set_status(m.to_string());
                            }
                            return;
                        }
                        Err(e) => st.error = Some(e),
                    }
                }
            }
        }
    }
    if open && !cancel {
        app.widgets.form = Some(st);
    }
}
