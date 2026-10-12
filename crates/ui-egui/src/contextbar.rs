//! The context bar: a thin row above the drawing, in both Classic and Pro, with the three things
//! that say what the next object will be and what can be changed: the current layer, the current
//! floor and the active drawing space. Everything here is a view of engine state and acts through
//! commands (`layer.current`, `floor.current`, `space.*`), so the rules (a locked or frozen layer is
//! not offered as current, a space never moves an object) live in the engine, not here.
//! See docs/ACTIVE_DRAWING_SPACE.md.

use cadcraft_color::Color;
use egui::{Color32, RichText, Sense, vec2};
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

/// One drawing space as the bar shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpaceRow {
    pub id: String,
    pub name: String,
    pub floor: Option<String>,
    pub layers: usize,
}

/// A name being asked for: saving the current layers as a new space, renaming, duplicating.
#[derive(Clone, Debug)]
pub enum NameFor {
    New,
    Rename(String),
    Duplicate(String),
}

#[derive(Clone, Debug)]
struct NameDialog {
    what: NameFor,
    text: String,
    focus: bool,
}

/// A big change waiting for the user's yes (activating a space that changes many layers).
#[derive(Clone, Debug)]
struct Confirm {
    id: String,
    name: String,
    count: usize,
}

#[derive(Default)]
pub struct State {
    /// (document uid, revision) the cached lists below were read for.
    key: Option<(u64, u64)>,
    pub active: Option<String>,
    pub dirty: bool,
    pub spaces: Vec<SpaceRow>,
    pub floors: Vec<String>,
    pub floor: Option<String>,
    pub search: String,
    pub all_floors: bool,
    name_dialog: Option<NameDialog>,
    confirm: Option<Confirm>,
    pub editor: Option<crate::spaceeditor::Editor>,
}

enum Action {
    SetLayer(String),
    SetFloor(String),
    Activate(String),
    ConfirmedActivate(String),
    Edit(String),
    NewSpace,
    Capture(String),
    Duplicate(String),
    Rename(String),
    Delete(String),
    Recover,
    TogglePin,
}

fn rgb32(c: Color) -> Color32 {
    let rgb = c.resolve(Color::Index(7), Color::Index(7));
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}

fn swatch(ui: &mut egui::Ui, c: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
    ui.painter().rect_filled(r, 1.0, c);
    ui.painter().rect_stroke(r, 1.0, egui::Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Inside);
}

/// Re-read the lists when the document or its revision changed (the lists live in the drawing).
pub fn refresh(app: &mut CadApp, force: bool) {
    let Ok(st) = app.session.state() else { return };
    let key = (st.uid, st.revision);
    if !force && app.ctx_bar.key == Some(key) {
        return;
    }
    app.ctx_bar.key = Some(key);
    let list = app.run("space.list", json!({})).unwrap_or(Value::Null);
    let status = app.run("space.status", json!({})).unwrap_or(Value::Null);
    let spaces: Vec<SpaceRow> = list["spaces"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|s| SpaceRow {
                    id: s["id"].as_str().unwrap_or("").to_string(),
                    name: s["name"].as_str().unwrap_or("").to_string(),
                    floor: s["floor"].as_str().map(str::to_string),
                    layers: s["layers"].as_u64().unwrap_or(0) as usize,
                })
                .collect()
        })
        .unwrap_or_default();
    let s = &mut app.ctx_bar;
    s.active = status["active"]["id"].as_str().map(str::to_string);
    s.dirty = status["dirty"].as_bool().unwrap_or(false);
    s.spaces = spaces;
    refresh_floors(app);
}

fn refresh_floors(app: &mut CadApp) {
    let v = app.run("floor.current", json!({})).unwrap_or(Value::Null);
    let names: Vec<String> = match v["mode"].as_str() {
        Some("project") => {
            v["floors"].as_array().map(|a| a.iter().filter_map(|f| f["name"].as_str().map(str::to_string)).collect()).unwrap_or_default()
        }
        _ => v["floors"].as_array().map(|a| a.iter().filter_map(|f| f.as_str().map(str::to_string)).collect()).unwrap_or_default(),
    };
    app.ctx_bar.floors = names;
    app.ctx_bar.floor = v["floor"].as_str().filter(|f| !f.is_empty()).map(str::to_string);
}

struct LayerRow {
    name: String,
    color: Color,
    on: bool,
    frozen: bool,
    locked: bool,
}

pub fn show(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    refresh(app, false);
    let mut actions: Vec<Action> = Vec::new();
    egui::Panel::top("aad_context_bar").exact_size(30.0).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            layer_selector(app, ui, &mut actions);
            ui.separator();
            floor_selector(app, ui, &mut actions);
            ui.separator();
            space_selector(app, ui, &mut actions);
            // Panels: pinned (always shown) or auto-hide (collapse when the pointer is away).
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                let pinned = app.ui.pin_panels;
                let label = if pinned { crate::tl!("Panels: pinned") } else { crate::tl!("Panels: auto-hide") };
                let tip = if pinned {
                    crate::tl!("The side panels stay open. Click to let them collapse when the pointer is away")
                } else {
                    crate::tl!("The side panels collapse when the pointer is away; the edge strip brings them back. Click to pin them open")
                };
                let pin = ui.add(egui::Button::new(RichText::new(label).small()).selected(!pinned)).on_hover_text(tip);
                crate::control::record_widget(ui, &pin, "Panels pin", "button", Some(pinned));
                if pin.clicked() {
                    actions.push(Action::TogglePin);
                }
            });
        });
    });
    for a in actions {
        run(app, a);
    }
    windows(app, ui.ctx());
}

fn layer_selector(app: &mut CadApp, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    let Ok(d) = app.session.doc() else { return };
    let cur = d.header.str("CLAYER", "0");
    let rows: Vec<LayerRow> =
        d.layers.iter().map(|l| LayerRow { name: l.name.clone(), color: l.color, on: l.on, frozen: l.frozen, locked: l.locked }).collect();
    let current = rows.iter().find(|r| r.name.eq_ignore_ascii_case(&cur));
    ui.label(RichText::new(crate::tl!("Layer")).weak().small());
    swatch(ui, current.map_or(Color32::GRAY, |r| rgb32(r.color)));
    let resp =
        ui.add(egui::Button::new(cur.clone()).min_size(vec2(150.0, 0.0))).on_hover_text(crate::tl!("Current layer: new objects are drawn on it"));
    if let Some(r) = current {
        let badge = if r.frozen {
            Some(crate::tl!("frozen"))
        } else if r.locked {
            Some(crate::tl!("locked"))
        } else if !r.on {
            Some(crate::tl!("hidden"))
        } else {
            None
        };
        if let Some(b) = badge {
            ui.label(RichText::new(b).small().color(Color32::from_rgb(220, 120, 60)));
        }
    }
    crate::control::record_widget(ui, &resp, "Layer selector", "button", None);
    let opened = resp.clicked();
    egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_min_width(240.0);
        let edit = ui.add(egui::TextEdit::singleline(&mut app.ctx_bar.search).hint_text(crate::tl!("Search layers")).desired_width(220.0));
        if opened {
            edit.request_focus();
        }
        let q = app.ctx_bar.search.to_lowercase();
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for r in rows.iter().filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q)) {
                let usable = !r.frozen && !r.locked;
                ui.horizontal(|ui| {
                    swatch(ui, rgb32(r.color));
                    let resp = ui.add_enabled_ui(usable, |ui| ui.selectable_label(r.name.eq_ignore_ascii_case(&cur), r.name.as_str())).inner;
                    if usable && resp.clicked() {
                        actions.push(Action::SetLayer(r.name.clone()));
                        ui.close();
                    } else if !usable {
                        resp.on_disabled_hover_text(if r.frozen {
                            crate::tl!("Frozen: thaw it first")
                        } else {
                            crate::tl!("Locked: unlock it first")
                        });
                    }
                    for (on, text) in [(r.frozen, crate::tl!("frozen")), (r.locked, crate::tl!("locked")), (!r.on, crate::tl!("hidden"))] {
                        if on {
                            ui.label(RichText::new(text).small().weak());
                        }
                    }
                });
            }
        });
    });
}

fn floor_selector(app: &mut CadApp, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    ui.label(RichText::new(crate::tl!("Floor")).weak().small());
    let floors = app.ctx_bar.floors.clone();
    let shown = app.ctx_bar.floor.clone().unwrap_or_else(|| "—".to_string());
    ui.add_enabled_ui(!floors.is_empty(), |ui| {
        egui::ComboBox::from_id_salt("ctx_floor").width(110.0).selected_text(shown.clone()).show_ui(ui, |ui| {
            for f in &floors {
                if ui.selectable_label(*f == shown, f.as_str()).clicked() {
                    actions.push(Action::SetFloor(f.clone()));
                }
            }
        });
    })
    .response
    .on_hover_text(if floors.is_empty() {
        crate::tl!("No floors defined: Architecture > Project > Floors")
    } else {
        crate::tl!("Current floor: choosing it moves no object")
    });
}

fn space_selector(app: &mut CadApp, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    ui.label(RichText::new(crate::tl!("Drawing space")).weak().small());
    let s = &app.ctx_bar;
    let active = s.active.as_ref().and_then(|a| s.spaces.iter().find(|x| &x.id == a)).cloned();
    let mut label = active.as_ref().map_or_else(|| crate::tl!("Current layers").to_string(), |a| a.name.clone());
    if active.is_some() && s.dirty {
        label.push_str(" ●");
    }
    let resp = ui.add(egui::Button::new(label).min_size(vec2(170.0, 0.0))).on_hover_text(if active.is_some() && s.dirty {
        crate::tl!("The layers differ from the saved drawing space")
    } else {
        crate::tl!("Drawing space: a saved set of visible, frozen and locked layers")
    });
    crate::control::record_widget(ui, &resp, "Drawing space selector", "button", None);
    if resp.clicked() {
        refresh(app, true);
    }
    egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_min_width(260.0);
        let floor = app.ctx_bar.floor.clone();
        let all = app.ctx_bar.all_floors;
        let spaces = app.ctx_bar.spaces.clone();
        let active_id = app.ctx_bar.active.clone();
        let dirty = app.ctx_bar.dirty;
        if spaces.is_empty() {
            ui.label(RichText::new(crate::tl!("No saved drawing spaces")).weak());
        }
        for sp in spaces.iter().filter(|sp| all || sp.floor.is_none() || sp.floor.as_deref() == floor.as_deref()) {
            ui.horizontal(|ui| {
                let on = active_id.as_deref() == Some(sp.id.as_str());
                let mut text = sp.name.clone();
                if on && dirty {
                    text.push_str(" ●");
                }
                let r = ui.selectable_label(on, text).on_hover_text(match &sp.floor {
                    Some(f) => format!("{} · {}", f, sp.layers),
                    None => format!("{}", sp.layers),
                });
                if r.clicked() {
                    actions.push(Action::Activate(sp.id.clone()));
                    ui.close();
                }
                if ui.small_button("…").on_hover_text(crate::tl!("Edit this drawing space")).clicked() {
                    actions.push(Action::Edit(sp.id.clone()));
                    ui.close();
                }
            });
        }
        let mut all_floors = all;
        if ui.checkbox(&mut all_floors, crate::tl!("Show spaces of every floor")).changed() {
            app.ctx_bar.all_floors = all_floors;
        }
        ui.separator();
        if ui.button(crate::tl!("Save current layers as a new space…")).clicked() {
            actions.push(Action::NewSpace);
            ui.close();
        }
        if let Some(id) = active_id.filter(|a| spaces.iter().any(|s| &s.id == a)) {
            ui.add_enabled_ui(dirty, |ui| {
                if ui.button(crate::tl!("Update the space from the current layers")).clicked() {
                    actions.push(Action::Capture(id.clone()));
                    ui.close();
                }
            });
            if ui.button(crate::tl!("Edit…")).clicked() {
                actions.push(Action::Edit(id.clone()));
                ui.close();
            }
            if ui.button(crate::tl!("Duplicate…")).clicked() {
                actions.push(Action::Duplicate(id.clone()));
                ui.close();
            }
            if ui.button(crate::tl!("Rename…")).clicked() {
                actions.push(Action::Rename(id.clone()));
                ui.close();
            }
            if ui.button(crate::tl!("Delete")).clicked() {
                actions.push(Action::Delete(id));
                ui.close();
            }
        }
        ui.separator();
        if ui
            .button(crate::tl!("Show, thaw and unlock all layers"))
            .on_hover_text(crate::tl!("The way out of a configuration that hides or locks too much"))
            .clicked()
        {
            actions.push(Action::Recover);
            ui.close();
        }
    });
}

fn say(app: &mut CadApp, r: Result<Value, String>) -> Option<Value> {
    match r {
        Ok(v) => {
            let mut msg = v["message"].as_str().unwrap_or("").to_string();
            if let Some(w) = v["warnings"].as_array() {
                for w in w.iter().filter_map(Value::as_str) {
                    msg.push_str(" — ");
                    msg.push_str(w);
                }
            }
            if !msg.is_empty() {
                app.set_status(msg);
            }
            Some(v)
        }
        Err(e) => {
            app.set_status(e);
            None
        }
    }
}

fn run(app: &mut CadApp, a: Action) {
    match a {
        Action::SetLayer(name) => {
            let r = app.run("layer.current", json!({"name": name}));
            say(app, r);
        }
        Action::SetFloor(name) => {
            let r = app.run("floor.current", json!({"name": name}));
            say(app, r);
            refresh(app, true);
        }
        Action::Activate(id) => {
            // A change to many layers is shown to the user first; a small one is just applied.
            match app.run("space.preview", json!({"id": id})) {
                Ok(p) if p["many"].as_bool() == Some(true) => {
                    let n = p["changed"].as_array().map_or(0, Vec::len);
                    let name = p["space"]["name"].as_str().unwrap_or("").to_string();
                    app.ctx_bar.confirm = Some(Confirm { id, name, count: n });
                }
                Ok(_) => run(app, Action::ConfirmedActivate(id)),
                Err(e) => app.set_status(e),
            }
        }
        Action::ConfirmedActivate(id) => {
            let r = app.run("space.activate", json!({"id": id}));
            say(app, r);
            refresh(app, true);
        }
        Action::Edit(id) => {
            let e = crate::spaceeditor::Editor::open(app, &id);
            app.ctx_bar.editor = e;
        }
        Action::NewSpace => app.ctx_bar.name_dialog = Some(NameDialog { what: NameFor::New, text: String::new(), focus: true }),
        Action::Capture(id) => {
            let r = app.run("space.capture", json!({"id": id}));
            say(app, r);
            refresh(app, true);
        }
        Action::Duplicate(id) => {
            let base = app.ctx_bar.spaces.iter().find(|s| s.id == id).map(|s| format!("{} copy", s.name)).unwrap_or_default();
            app.ctx_bar.name_dialog = Some(NameDialog { what: NameFor::Duplicate(id), text: base, focus: true });
        }
        Action::Rename(id) => {
            let base = app.ctx_bar.spaces.iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or_default();
            app.ctx_bar.name_dialog = Some(NameDialog { what: NameFor::Rename(id), text: base, focus: true });
        }
        Action::Delete(id) => {
            let r = app.run("space.delete", json!({"id": id}));
            say(app, r);
            refresh(app, true);
        }
        Action::TogglePin => {
            let _ = app.run("ui.toggle.pinpanels", json!({}));
        }
        Action::Recover => {
            let r = app.run("space.recover", json!({}));
            say(app, r);
            refresh(app, true);
        }
    }
}

/// The small windows: a name prompt, the "this changes many layers" question, and the editor.
fn windows(app: &mut CadApp, ctx: &egui::Context) {
    if let Some(mut dlg) = app.ctx_bar.name_dialog.take() {
        let mut done: Option<bool> = None;
        let title = match &dlg.what {
            NameFor::New => crate::tl!("Save drawing space"),
            NameFor::Rename(_) => crate::tl!("Rename drawing space"),
            NameFor::Duplicate(_) => crate::tl!("Duplicate drawing space"),
        };
        egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, vec2(0.0, 80.0)).show(ctx, |ui| {
            ui.label(crate::tl!("Name"));
            let r = ui.add(egui::TextEdit::singleline(&mut dlg.text).desired_width(260.0));
            if dlg.focus {
                r.request_focus();
                dlg.focus = false;
            }
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.horizontal(|ui| {
                if (ui.button(crate::tl!("OK")).clicked() || enter) && !dlg.text.trim().is_empty() {
                    done = Some(true);
                }
                if ui.button(crate::tl!("Cancel")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    done = Some(false);
                }
            });
        });
        match done {
            None => app.ctx_bar.name_dialog = Some(dlg),
            Some(false) => {}
            Some(true) => {
                let name = dlg.text.trim().to_string();
                let r = match &dlg.what {
                    NameFor::New => app.run("space.save", json!({"name": name, "current": true})),
                    NameFor::Rename(id) => app.run("space.update", json!({"id": id, "newName": name})),
                    NameFor::Duplicate(id) => app.run("space.duplicate", json!({"id": id, "newName": name})),
                };
                if say(app, r).is_none() {
                    // Keep the dialog open so the name can be corrected.
                    app.ctx_bar.name_dialog = Some(dlg);
                }
                refresh(app, true);
            }
        }
    }
    if let Some(c) = app.ctx_bar.confirm.take() {
        let mut answer: Option<bool> = None;
        egui::Window::new(crate::tl!("Apply drawing space"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 80.0))
            .show(ctx, |ui| {
                ui.label(format!("{}: {} {}", c.name, c.count, crate::tl!("layer settings will change. Objects are not changed.")));
                ui.horizontal(|ui| {
                    if ui.button(crate::tl!("Apply")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(crate::tl!("Cancel")).clicked() {
                        answer = Some(false);
                    }
                });
            });
        match answer {
            None => app.ctx_bar.confirm = Some(c),
            Some(true) => run(app, Action::ConfirmedActivate(c.id)),
            Some(false) => {}
        }
    }
    crate::spaceeditor::show(app, ctx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb32_resolves_index_colours() {
        assert_eq!(rgb32(Color::Index(1)), Color32::from_rgb(255, 0, 0));
    }
}
