//! The drawing space editor: a table of layers with their saved Visible, Frozen and Locked
//! states and the derived Editable state, for one saved space. Edits here change the *saved*
//! configuration only (`space.update`); the drawing changes only when the space is applied
//! (`space.activate`). The two are labelled apart. Validation is the engine's: its message is shown
//! as it comes. See docs/ACTIVE_DRAWING_SPACE.md.

use egui::{Color32, RichText, vec2};
use serde_json::{Value, json};

use crate::CadApp;

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub layer: String,
    /// Part of the saved space (listed). Unlisted layers keep whatever state they have on activation
    /// (or what the space's "unlisted" policy says).
    pub listed: bool,
    pub visible: bool,
    pub frozen: bool,
    pub locked: bool,
    /// The layer exists in the drawing now (a listed layer can have been deleted since).
    pub exists: bool,
    pub selected: bool,
}

impl Row {
    pub fn editable(&self) -> bool {
        !self.frozen && !self.locked
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Listed,
    Unlisted,
    Visible,
    Hidden,
    Frozen,
    Locked,
    Editable,
    Differs,
}

impl Filter {
    const ALL: [(Filter, &'static str); 9] = [
        (Filter::All, "All"),
        (Filter::Listed, "In the space"),
        (Filter::Unlisted, "Not in the space"),
        (Filter::Visible, "Visible"),
        (Filter::Hidden, "Hidden"),
        (Filter::Frozen, "Frozen"),
        (Filter::Locked, "Locked"),
        (Filter::Editable, "Editable"),
        (Filter::Differs, "Differs from the drawing"),
    ];
}

#[derive(Clone, Debug)]
pub struct Editor {
    pub id: String,
    pub name: String,
    pub description: String,
    pub floor: String,
    pub current: String,
    pub unlisted: String,
    pub rows: Vec<Row>,
    /// What was last saved, to tell unsaved edits from saved ones.
    saved: Vec<Row>,
    saved_fields: (String, String, String, String, String),
    pub search: String,
    pub filter: Filter,
    pub error: Option<String>,
    pub notice: Option<String>,
}

impl Editor {
    pub fn open(app: &mut CadApp, id: &str) -> Option<Editor> {
        let v = match app.run("space.get", json!({"id": id})) {
            Ok(v) => v,
            Err(e) => {
                app.set_status(e);
                return None;
            }
        };
        Some(Editor::from_json(app, &v))
    }

    fn from_json(app: &CadApp, v: &Value) -> Editor {
        let live: Vec<(String, bool, bool, bool)> =
            app.session.doc().map(|d| d.layers.iter().map(|l| (l.name.clone(), l.on, l.frozen, l.locked)).collect()).unwrap_or_default();
        let entries = v["entries"].as_array().cloned().unwrap_or_default();
        let mut rows: Vec<Row> = Vec::new();
        for (name, on, frozen, locked) in &live {
            let e = entries.iter().find(|e| e["layer"].as_str().is_some_and(|l| l.eq_ignore_ascii_case(name)));
            rows.push(match e {
                Some(e) => Row {
                    layer: name.clone(),
                    listed: true,
                    visible: e["visible"].as_bool().unwrap_or(true),
                    frozen: e["frozen"].as_bool().unwrap_or(false),
                    locked: e["locked"].as_bool().unwrap_or(false),
                    exists: true,
                    selected: false,
                },
                None => Row { layer: name.clone(), listed: false, visible: *on, frozen: *frozen, locked: *locked, exists: true, selected: false },
            });
        }
        for e in entries.iter().filter(|e| e["exists"].as_bool() == Some(false)) {
            rows.push(Row {
                layer: e["layer"].as_str().unwrap_or("").to_string(),
                listed: true,
                visible: e["visible"].as_bool().unwrap_or(true),
                frozen: e["frozen"].as_bool().unwrap_or(false),
                locked: e["locked"].as_bool().unwrap_or(false),
                exists: false,
                selected: false,
            });
        }
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        let fields = (s("name"), s("description"), s("floor"), s("current"), s("unlisted"));
        Editor {
            id: s("id"),
            name: fields.0.clone(),
            description: fields.1.clone(),
            floor: fields.2.clone(),
            current: fields.3.clone(),
            unlisted: if fields.4.is_empty() { "keep".into() } else { fields.4.clone() },
            saved: rows.clone(),
            rows,
            saved_fields: (
                fields.0,
                fields.1,
                fields.2,
                fields.3,
                if v["unlisted"].as_str().unwrap_or("").is_empty() { "keep".into() } else { s("unlisted") },
            ),
            search: String::new(),
            filter: Filter::All,
            error: None,
            notice: None,
        }
    }

    pub fn unsaved(&self) -> bool {
        let strip = |rows: &[Row]| rows.iter().map(|r| Row { selected: false, ..r.clone() }).collect::<Vec<_>>();
        strip(&self.rows) != strip(&self.saved)
            || (self.name.clone(), self.description.clone(), self.floor.clone(), self.current.clone(), self.unlisted.clone()) != self.saved_fields
    }

    /// The row passes the search and the state filter. `live` is the layer's state in the drawing.
    pub fn shown(&self, r: &Row, live: Option<(bool, bool, bool)>) -> bool {
        let q = self.search.to_lowercase();
        if !q.is_empty() && !r.layer.to_lowercase().contains(&q) {
            return false;
        }
        match self.filter {
            Filter::All => true,
            Filter::Listed => r.listed,
            Filter::Unlisted => !r.listed,
            Filter::Visible => r.visible,
            Filter::Hidden => !r.visible,
            Filter::Frozen => r.frozen,
            Filter::Locked => r.locked,
            Filter::Editable => r.editable(),
            Filter::Differs => r.listed && live.is_some_and(|l| l != (r.visible, r.frozen, r.locked)),
        }
    }

    /// Apply a bulk change to the selected rows (or the shown rows when none is selected).
    pub fn bulk(&mut self, f: impl Fn(&mut Row), shown: &[usize]) {
        let any = self.rows.iter().any(|r| r.selected);
        for (i, r) in self.rows.iter_mut().enumerate() {
            if (any && r.selected) || (!any && shown.contains(&i)) {
                f(r);
                r.listed = true;
            }
        }
    }

    fn save_params(&self) -> Value {
        let layers: Vec<Value> = self
            .rows
            .iter()
            .filter(|r| r.listed && r.exists)
            .map(|r| json!({"layer": r.layer, "visible": r.visible, "frozen": r.frozen, "locked": r.locked}))
            .collect();
        let remove: Vec<&str> = self.rows.iter().filter(|r| !r.listed).map(|r| r.layer.as_str()).collect();
        json!({"id": self.id, "newName": self.name, "description": self.description, "floor": self.floor, "current": self.current, "unlisted": self.unlisted, "layers": layers, "remove": remove})
    }
}

pub fn show(app: &mut CadApp, ctx: &egui::Context) {
    let Some(mut ed) = app.ctx_bar.editor.take() else { return };
    let live: std::collections::HashMap<String, (bool, bool, bool)> =
        app.session.doc().map(|d| d.layers.iter().map(|l| (l.name.to_lowercase(), (l.on, l.frozen, l.locked))).collect()).unwrap_or_default();
    let floors = app.ctx_bar.floors.clone();
    let mut close = false;
    let mut do_save = false;
    let mut do_apply = false;
    let mut open = true;
    let title = format!("{} — {}", crate::tl!("Drawing space"), ed.name);
    egui::Window::new(title)
        .id(egui::Id::new("space_editor"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size(vec2(640.0, 520.0))
        .show(ctx, |ui| {
            ui.label(
                RichText::new(crate::tl!("Changes here edit the saved drawing space. The drawing changes only when you apply it.")).weak().small(),
            );
            egui::Grid::new("space_fields").num_columns(2).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
                ui.label(crate::tl!("Name"));
                ui.add(egui::TextEdit::singleline(&mut ed.name).desired_width(240.0));
                ui.end_row();
                ui.label(crate::tl!("Description"));
                ui.add(egui::TextEdit::singleline(&mut ed.description).desired_width(240.0));
                ui.end_row();
                ui.label(crate::tl!("Floor"));
                egui::ComboBox::from_id_salt("space_floor")
                    .selected_text(if ed.floor.is_empty() { crate::tl!("Every floor").to_string() } else { ed.floor.clone() })
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(ed.floor.is_empty(), crate::tl!("Every floor")).clicked() {
                            ed.floor.clear();
                        }
                        for f in &floors {
                            if ui.selectable_label(ed.floor == *f, f.as_str()).clicked() {
                                ed.floor = f.clone();
                            }
                        }
                    });
                ui.end_row();
                ui.label(crate::tl!("Current layer"));
                let editable: Vec<String> =
                    ed.rows.iter().filter(|r| r.listed && r.exists && r.visible && r.editable()).map(|r| r.layer.clone()).collect();
                egui::ComboBox::from_id_salt("space_current")
                    .selected_text(if ed.current.is_empty() { crate::tl!("Keep the current one").to_string() } else { ed.current.clone() })
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(ed.current.is_empty(), crate::tl!("Keep the current one")).clicked() {
                            ed.current.clear();
                        }
                        for l in &editable {
                            if ui.selectable_label(ed.current == *l, l.as_str()).clicked() {
                                ed.current = l.clone();
                            }
                        }
                    });
                ui.end_row();
                ui.label(crate::tl!("Layers not in the space"));
                egui::ComboBox::from_id_salt("space_unlisted")
                    .selected_text(match ed.unlisted.as_str() {
                        "hide" => crate::tl!("Hide them"),
                        "lock" => crate::tl!("Lock them"),
                        _ => crate::tl!("Leave them as they are"),
                    })
                    .show_ui(ui, |ui| {
                        for (v, label) in
                            [("keep", crate::tl!("Leave them as they are")), ("hide", crate::tl!("Hide them")), ("lock", crate::tl!("Lock them"))]
                        {
                            if ui.selectable_label(ed.unlisted == v, label).clicked() {
                                ed.unlisted = v.to_string();
                            }
                        }
                    });
                ui.end_row();
            });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut ed.search).hint_text(crate::tl!("Search layers")).desired_width(160.0));
                for (f, label) in Filter::ALL {
                    if ui.selectable_label(ed.filter == f, crate::i18n::t(label)).clicked() {
                        ed.filter = f;
                    }
                }
            });
            let shown: Vec<usize> =
                (0..ed.rows.len()).filter(|&i| ed.shown(&ed.rows[i], live.get(&ed.rows[i].layer.to_lowercase()).copied())).collect();
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(crate::tl!("Apply to the selected rows (or all shown):")).small());
                let sh = shown.clone();
                for (label, f) in [
                    (crate::tl!("Show"), (|r: &mut Row| r.visible = true) as fn(&mut Row)),
                    (crate::tl!("Hide"), |r| r.visible = false),
                    (crate::tl!("Freeze"), |r| r.frozen = true),
                    (crate::tl!("Thaw"), |r| r.frozen = false),
                    (crate::tl!("Lock"), |r| r.locked = true),
                    (crate::tl!("Unlock"), |r| r.locked = false),
                ] {
                    if ui.small_button(label).clicked() {
                        ed.bulk(f, &sh);
                    }
                }
                if ui.small_button(crate::tl!("Select shown")).clicked() {
                    for i in &sh {
                        ed.rows[*i].selected = true;
                    }
                }
                if ui.small_button(crate::tl!("Select none")).clicked() {
                    for r in &mut ed.rows {
                        r.selected = false;
                    }
                }
            });
            egui::ScrollArea::both().max_height(300.0).auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("space_rows").striped(true).num_columns(8).spacing(vec2(10.0, 3.0)).show(ui, |ui| {
                    for h in [
                        "",
                        crate::tl!("Layer"),
                        crate::tl!("In space"),
                        crate::tl!("Visible"),
                        crate::tl!("Frozen"),
                        crate::tl!("Locked"),
                        crate::tl!("Editable"),
                        crate::tl!("Drawing now"),
                    ] {
                        ui.label(RichText::new(h).strong().small());
                    }
                    ui.end_row();
                    for &i in &shown {
                        let now = live.get(&ed.rows[i].layer.to_lowercase()).copied();
                        let r = &mut ed.rows[i];
                        ui.checkbox(&mut r.selected, "");
                        let name = if r.exists {
                            RichText::new(&r.layer)
                        } else {
                            RichText::new(format!("{} ({})", r.layer, crate::tl!("missing"))).color(Color32::from_rgb(220, 120, 60))
                        };
                        ui.label(if r.listed { name } else { name.weak() });
                        ui.checkbox(&mut r.listed, "");
                        ui.add_enabled_ui(r.listed, |ui| {
                            ui.checkbox(&mut r.visible, "");
                            ui.checkbox(&mut r.frozen, "");
                            ui.checkbox(&mut r.locked, "");
                        });
                        ui.label(if !r.listed {
                            RichText::new("—").weak()
                        } else if r.editable() {
                            RichText::new(crate::tl!("Yes")).color(Color32::from_rgb(90, 170, 90))
                        } else {
                            RichText::new(crate::tl!("No")).color(Color32::from_rgb(200, 90, 80))
                        });
                        match now {
                            Some((on, fr, lk)) => {
                                let txt = format!("{}{}{}", if on { "V" } else { "-" }, if fr { "F" } else { "-" }, if lk { "L" } else { "-" });
                                let differs = r.listed && (on, fr, lk) != (r.visible, r.frozen, r.locked);
                                ui.label(if differs {
                                    RichText::new(txt).monospace().color(Color32::from_rgb(220, 160, 60))
                                } else {
                                    RichText::new(txt).monospace().weak()
                                });
                            }
                            None => {
                                ui.label("");
                            }
                        }
                        ui.end_row();
                    }
                });
            });
            if let Some(e) = &ed.error {
                ui.colored_label(Color32::from_rgb(220, 90, 80), e);
            }
            if let Some(n) = &ed.notice {
                ui.label(RichText::new(n).small());
            }
            ui.separator();
            ui.horizontal(|ui| {
                let dirty = ed.unsaved();
                if ui.add_enabled(dirty, egui::Button::new(crate::tl!("Save changes"))).clicked() {
                    do_save = true;
                }
                if ui
                    .button(crate::tl!("Save and apply to the drawing"))
                    .on_hover_text(crate::tl!("Saves this space, then sets the layers of the drawing to it"))
                    .clicked()
                {
                    do_save = true;
                    do_apply = true;
                }
                if ui
                    .add_enabled(!dirty, egui::Button::new(crate::tl!("Apply saved space")))
                    .on_hover_text(crate::tl!("Sets the layers of the drawing to the saved space"))
                    .clicked()
                {
                    do_apply = true;
                }
                if dirty {
                    ui.label(RichText::new(crate::tl!("unsaved changes")).small().color(Color32::from_rgb(220, 160, 60)));
                }
                if ui.button(crate::tl!("Close")).clicked() {
                    close = true;
                }
            });
        });
    if do_save {
        match app.run("space.update", ed.save_params()) {
            Ok(v) => {
                // Reload from the engine so the table shows what is stored.
                let (search, filter) = (ed.search.clone(), ed.filter);
                ed = Editor::from_json(app, &v["space"]);
                ed.search = search;
                ed.filter = filter;
                ed.notice = Some(crate::tl!("Saved.").to_string());
                ed.error = None;
            }
            Err(e) => {
                ed.error = Some(e);
                ed.notice = None;
                do_apply = false;
            }
        }
    }
    if do_apply && ed.error.is_none() {
        match app.run("space.activate", json!({"id": ed.id})) {
            Ok(v) => {
                let mut msg = v["message"].as_str().unwrap_or("").to_string();
                for w in v["warnings"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                    msg.push_str(" — ");
                    msg.push_str(w);
                }
                app.set_status(msg.clone());
                ed.notice = Some(msg);
            }
            Err(e) => ed.error = Some(e),
        }
    }
    if do_save || do_apply {
        crate::contextbar::refresh(app, true);
    }
    if open && !close {
        app.ctx_bar.editor = Some(ed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(layer: &str, listed: bool, v: bool, f: bool, l: bool) -> Row {
        Row { layer: layer.into(), listed, visible: v, frozen: f, locked: l, exists: true, selected: false }
    }

    fn editor(rows: Vec<Row>) -> Editor {
        Editor {
            id: "ds-1".into(),
            name: "X".into(),
            description: String::new(),
            floor: String::new(),
            current: String::new(),
            unlisted: "keep".into(),
            saved: rows.clone(),
            rows,
            saved_fields: ("X".into(), String::new(), String::new(), String::new(), "keep".into()),
            search: String::new(),
            filter: Filter::All,
            error: None,
            notice: None,
        }
    }

    #[test]
    fn editable_is_derived_never_stored() {
        assert!(row("A", true, true, false, false).editable());
        assert!(!row("A", true, true, false, true).editable());
        assert!(!row("A", true, true, true, false).editable());
        assert!(row("A", true, false, false, false).editable(), "hidden but unlocked is editable by the engine's rules");
    }

    #[test]
    fn filters_and_search_select_rows() {
        let mut e =
            editor(vec![row("A-WALL", true, true, false, false), row("S-COL", true, true, false, true), row("A-FURN", false, false, false, false)]);
        e.filter = Filter::Locked;
        assert!(!e.shown(&e.rows[0], None) && e.shown(&e.rows[1], None));
        e.filter = Filter::Unlisted;
        assert!(e.shown(&e.rows[2], None));
        e.filter = Filter::Differs;
        assert!(e.shown(&e.rows[0], Some((true, true, false))));
        assert!(!e.shown(&e.rows[0], Some((true, false, false))));
        e.filter = Filter::All;
        e.search = "col".into();
        assert!(e.shown(&e.rows[1], None) && !e.shown(&e.rows[0], None));
    }

    #[test]
    fn bulk_changes_the_selection_or_else_what_is_shown_and_lists_the_rows() {
        let mut e = editor(vec![row("A", false, true, false, false), row("B", false, true, false, false), row("C", false, true, false, false)]);
        e.bulk(|r| r.locked = true, &[0, 2]);
        assert_eq!(e.rows.iter().map(|r| r.locked).collect::<Vec<_>>(), [true, false, true]);
        assert!(e.rows[0].listed && !e.rows[1].listed);
        e.rows[1].selected = true;
        e.bulk(|r| r.visible = false, &[0, 2]);
        assert_eq!(e.rows.iter().map(|r| r.visible).collect::<Vec<_>>(), [true, false, true], "the selection wins over the shown rows");
    }

    #[test]
    fn unsaved_edits_are_noticed_and_save_params_drop_unlisted_and_missing_rows() {
        let mut e = editor(vec![row("A", true, true, false, false), row("B", false, true, false, false)]);
        assert!(!e.unsaved());
        e.rows[0].locked = true;
        assert!(e.unsaved());
        let p = e.save_params();
        assert_eq!(p["layers"].as_array().unwrap().len(), 1);
        assert_eq!(p["remove"], json!(["B"]));
        e.rows[0].exists = false;
        assert!(e.save_params()["layers"].as_array().unwrap().is_empty(), "a deleted layer is not written back");
    }
}
