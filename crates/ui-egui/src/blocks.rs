//! The Blocks panel: the drawing's block definitions with a small preview, how many references of
//! each are in the drawing, insertion of the selected definition, selection of its references and
//! its attribute definitions. It reads the drawing database every frame, so it never goes stale.

use cadcraft_doc::{Drawing, EntityKind, Handle};
use cadcraft_engine::Input;
use egui::{Rect, Sense, Stroke, pos2, vec2};
use serde_json::json;

use crate::CadApp;
use crate::theme::Tokens;

/// The block definition selected in the panel.
const SELECTED: &str = "blocks_panel_selected";
/// Largest number of references listed for a definition.
const MAX_REFS: usize = 200;

/// References to each block, by block name, across model space and layouts.
fn reference_counts(d: &Drawing) -> std::collections::BTreeMap<String, Vec<Handle>> {
    let mut out: std::collections::BTreeMap<String, Vec<Handle>> = std::collections::BTreeMap::new();
    for e in std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).flat_map(|s| s.iter()) {
        if let EntityKind::Insert(i) = &e.kind {
            out.entry(i.block.clone()).or_default().push(e.handle);
        }
    }
    out
}

/// A small line drawing of a block definition, fitted into `rect`.
fn preview(ui: &egui::Ui, rect: Rect, d: &Drawing, block: &cadcraft_doc::Block) {
    let t = Tokens::get();
    ui.painter().rect_filled(rect, 2.0, t.canvas);
    let mut lines: Vec<Vec<cadcraft_geom::Vec2>> = Vec::new();
    for e in block.entities.iter().take(400) {
        if matches!(e.kind, EntityKind::Text(_) | EntityKind::MText(_) | EntityKind::AttDef(_) | EntityKind::Insert(_)) {
            continue;
        }
        lines.extend(cadcraft_engine::select::hit_polylines(d, e, 0.05));
    }
    let pts = lines.iter().flatten();
    let (mut lo, mut hi) = (cadcraft_geom::Vec2::new(f64::INFINITY, f64::INFINITY), cadcraft_geom::Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    for p in pts {
        lo = cadcraft_geom::Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = cadcraft_geom::Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    if !(lo.x.is_finite() && hi.x.is_finite()) {
        return;
    }
    let (w, h) = ((hi.x - lo.x).max(1e-9), (hi.y - lo.y).max(1e-9));
    let k = (f64::from(rect.width() - 6.0) / w).min(f64::from(rect.height() - 6.0) / h);
    let origin = pos2(rect.center().x - (w * k / 2.0) as f32, rect.center().y + (h * k / 2.0) as f32);
    let map = |p: &cadcraft_geom::Vec2| pos2(origin.x + ((p.x - lo.x) * k) as f32, origin.y - ((p.y - lo.y) * k) as f32);
    for l in &lines {
        ui.painter().add(egui::Shape::line(l.iter().map(map).collect(), Stroke::new(1.0, t.text)));
    }
}

/// Start INSERT for `name` (the command asks for the insertion point next).
fn insert(app: &mut CadApp, name: &str) {
    app.start("insert");
    if let Err(e) = app.session.input(Input::Text(name.to_string())) {
        app.session.echo(e.to_string());
    }
}

pub(crate) fn blocks_section(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    ui.add_space(6.0);
    ui.label(egui::RichText::new(crate::tl!("Block definitions")).strong());
    let Ok(d) = app.session.doc() else {
        ui.label(crate::tl!("No drawing open"));
        return;
    };
    let refs = reference_counts(d);
    let mut names: Vec<&cadcraft_doc::Block> = d.blocks.values().map(|b| b.as_ref()).filter(|b| !b.anonymous).collect();
    names.sort_by_key(|b| b.name.to_ascii_lowercase());
    let id = egui::Id::new(SELECTED);
    let mut selected: Option<String> = ui.data(|data| data.get_temp::<String>(id)).filter(|n| d.block(n).is_some());
    if names.is_empty() {
        ui.label(egui::RichText::new(crate::tl!("This drawing defines no blocks.")).color(t.text_dim));
    }
    let mut action: Option<(&'static str, String)> = None;
    for b in &names {
        let count = refs.get(&b.name).map_or(0, Vec::len);
        let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
        let is_sel = selected.as_deref() == Some(b.name.as_str());
        if is_sel {
            ui.painter().rect_filled(row, 2.0, t.select_bg.gamma_multiply(0.35));
        } else if resp.hovered() {
            ui.painter().rect_filled(row, 2.0, t.control_hover.gamma_multiply(0.5));
        }
        preview(ui, Rect::from_min_size(row.min + vec2(4.0, 4.0), vec2(36.0, 36.0)), d, b);
        ui.painter().text(row.min + vec2(48.0, 14.0), egui::Align2::LEFT_CENTER, &b.name, crate::theme::body(), t.text);
        let detail = format!("{} {} · {} {}", b.entities.len(), crate::tl!("objects"), count, crate::tl!("references"));
        ui.painter().text(row.min + vec2(48.0, 30.0), egui::Align2::LEFT_CENTER, detail, crate::theme::small(), t.text_dim);
        crate::control::record_widget(ui, &resp, &b.name, "block-definition", Some(is_sel));
        if resp.clicked() {
            selected = Some(b.name.clone());
        }
        if resp.double_clicked() {
            action = Some(("insert", b.name.clone()));
        }
    }
    if let Some(name) = selected.clone()
        && let Some(b) = d.block(&name)
    {
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(crate::tl!("Insert")).on_hover_text(crate::tl!("Insert the selected block (asks for the insertion point)")).clicked() {
                action = Some(("insert", name.clone()));
            }
            let list = refs.get(&name).cloned().unwrap_or_default();
            ui.add_enabled_ui(!list.is_empty(), |ui| {
                if ui.button(crate::tl!("Select references")).clicked() {
                    action = Some(("select", name.clone()));
                }
            });
        });
        let tags: Vec<String> = b
            .entities
            .iter()
            .filter_map(|e| if let EntityKind::AttDef(a) = &e.kind { Some(format!("{} = {}", a.tag, a.text.value)) } else { None })
            .collect();
        if !tags.is_empty() {
            ui.label(egui::RichText::new(crate::tl!("Attribute definitions")).color(t.text_dim));
            for line in tags.iter().take(50) {
                ui.label(line);
            }
            ui.label(egui::RichText::new(crate::tl!("Edit attribute values on a reference with ATTEDIT.")).small().color(t.text_faint));
        }
    }
    ui.add_space(8.0);
    ui.label(egui::RichText::new(crate::tl!("External references are not supported.")).small().color(t.text_faint));
    ui.data_mut(|data| match &selected {
        Some(n) => {
            data.insert_temp(id, n.clone());
        }
        None => {
            data.remove::<String>(id);
        }
    });
    match action {
        Some(("insert", name)) => insert(app, &name),
        Some(("select", name)) => {
            let hs: Vec<String> = app
                .session
                .doc()
                .map(|d| reference_counts(d).remove(&name).unwrap_or_default().into_iter().take(MAX_REFS).map(|h| h.hex()).collect())
                .unwrap_or_default();
            let _ = app.run("select", json!({ "handles": hs }));
        }
        _ => {}
    }
}
