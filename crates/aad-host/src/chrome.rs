//! Menu tree and command-line snapshot for the WinUI shell.
//!
//! Entries come from the engine registry. UI-only commands (file dialogs, panels, theme) stay in
//! the egui shell until that shell can host them.

use aad_engine::{Session, command_specs};
use serde::Serialize;

/// Top-level menus, in the same order as the egui shell.
const MENUS: &[&str] =
    &["File", "Edit", "View", "Insert", "Format", "Tools", "Draw", "Dimension", "Modify", "Architecture", "AORMS", "Window", "Help"];

const MAX_DEPTH: usize = 8;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Node {
    title: String,
    items: Vec<Node>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shortcut: Option<String>,
    enabled: bool,
}

/// JSON array of menus. Each item has an `id` the shell passes to the command line.
pub fn menu_json(session: &Session) -> String {
    let mut menus: Vec<Node> =
        MENUS.iter().map(|title| Node { title: (*title).to_string(), items: Vec::new(), id: None, shortcut: None, enabled: true }).collect();
    for spec in command_specs() {
        if spec.menu.is_empty() {
            continue;
        }
        let Some((top, rest)) = spec.menu.split_first() else { continue };
        if rest.is_empty() {
            continue;
        }
        let Some(menu) = menus.iter_mut().find(|m| m.title == *top) else { continue };
        push_item(&mut menu.items, rest, spec.label, spec.id, spec.shortcut, (spec.enabled)(session).is_ok(), 0);
    }
    menus.retain(|m| !m.items.is_empty());
    serde_json::to_string(&menus).unwrap_or_else(|_| "[]".into())
}

fn push_item(entries: &mut Vec<Node>, rest: &[&str], label: &str, id: &str, shortcut: Option<&str>, enabled: bool, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some((seg, tail)) = rest.split_first() else { return };
    if tail.is_empty() {
        let title = if seg.is_empty() { label.to_string() } else { (*seg).to_string() };
        entries.push(Node { title, items: Vec::new(), id: Some(id.to_string()), shortcut: shortcut.map(|s| s.replace("Cmd+", "Ctrl+")), enabled });
        return;
    }
    if !entries.iter().any(|e| e.id.is_none() && e.title == *seg) {
        entries.push(Node { title: (*seg).to_string(), items: Vec::new(), id: None, shortcut: None, enabled: true });
    }
    let Some(sub) = entries.iter_mut().find(|e| e.id.is_none() && e.title == *seg) else { return };
    push_item(&mut sub.items, tail, label, id, shortcut, enabled, depth + 1);
}

/// Right-click menu. `snap` is the object-snap override list; otherwise the shortcut menu.
/// A running command still treats a plain right-click as Enter, so this is only built when the
/// shell should show a menu.
/// World coordinate icon menu. The model view is the world system; hiding is the other choice.
pub fn ucs_popup(x: i32, y: i32) -> String {
    serde_json::json!({
        "kind": "ucs",
        "x": x,
        "y": y,
        "groups": [[
            {"label": "World (WCS)", "id": "show", "enabled": true, "run": "ucs"},
            {"label": "Hide UCS Icon", "id": "hide", "enabled": true, "run": "ucs"},
        ]],
    })
    .to_string()
}

pub fn popup_json(session: &Session, snap: bool, x: i32, y: i32) -> String {
    let groups = if snap { snap_groups(session) } else { command_groups(session) };
    serde_json::json!({ "kind": if snap { "snap" } else { "command" }, "x": x, "y": y, "groups": groups }).to_string()
}

fn command_groups(session: &Session) -> Vec<Vec<serde_json::Value>> {
    let mut groups = Vec::new();
    if let Some(id) = session.last_command.as_deref()
        && let Some(item) = command_item(session, id, Some(&format!("Repeat {}", id.to_ascii_uppercase())))
    {
        groups.push(vec![item]);
    }
    let clip = ["cutclip", "copyclip", "copybase", "pasteclip", "pasteblock", "pasteorig"];
    if let Some(menu) = submenu(session, "Clipboard", &clip) {
        groups.push(vec![menu]);
    }
    if !session.selection().is_empty() {
        let rows = ["erase", "move", "copy", "scale", "rotate", "properties"]
            .into_iter()
            .filter_map(|id| command_item(session, id, if id == "copy" { Some("Copy Selection") } else { None }))
            .collect::<Vec<_>>();
        if !rows.is_empty() {
            groups.push(rows);
        }
    }
    push_row(&mut groups, session, &["u", "redo", "pan", "zoom"]);
    push_row(&mut groups, session, &["qselect", "find"]);
    groups
}

fn snap_groups(session: &Session) -> Vec<Vec<serde_json::Value>> {
    let on = session.current_prompt().is_some_and(|prompt| aad_engine::pointmod::accepts(&prompt));
    let typed = |label: &str, id: &str| serde_json::json!({ "label": label, "id": id, "enabled": on, "run": "type" });
    let name = |bit: u32| aad_engine::snap::mode::ALL.iter().find(|(value, _)| *value == bit).map_or("", |(_, name)| *name);
    let filters: Vec<_> = [".x", ".y", ".z", ".xy", ".xz", ".yz"].into_iter().map(|id| typed(id, id)).collect();
    let mut groups = vec![vec![
        typed("From", "_from"),
        typed("Mid Between 2 Points", "_m2p"),
        serde_json::json!({ "label": "Point Filters", "enabled": on, "items": filters }),
    ]];
    let modes: [&[(u32, &str)]; 3] = [
        &[
            (aad_engine::snap::mode::END, "_endp"),
            (aad_engine::snap::mode::MID, "_mid"),
            (aad_engine::snap::mode::INT, "_int"),
            (aad_engine::snap::mode::APP, "_app"),
            (aad_engine::snap::mode::EXT, "_ext"),
        ],
        &[
            (aad_engine::snap::mode::CEN, "_cen"),
            (aad_engine::snap::mode::GCEN, "_gcen"),
            (aad_engine::snap::mode::QUA, "_qua"),
            (aad_engine::snap::mode::TAN, "_tan"),
        ],
        &[
            (aad_engine::snap::mode::PER, "_per"),
            (aad_engine::snap::mode::PAR, "_par"),
            (aad_engine::snap::mode::NOD, "_nod"),
            (aad_engine::snap::mode::INS, "_ins"),
            (aad_engine::snap::mode::NEA, "_nea"),
        ],
    ];
    for row in modes {
        groups.push(row.iter().map(|(bit, id)| typed(name(*bit), id)).collect());
    }
    groups.push(vec![typed("None", "_non")]);
    groups.push(vec![serde_json::json!({ "label": "Osnap Settings...", "id": "dsettings", "enabled": true, "run": "command" })]);
    groups
}

fn push_row(groups: &mut Vec<Vec<serde_json::Value>>, session: &Session, ids: &[&str]) {
    let row: Vec<_> = ids.iter().filter_map(|id| command_item(session, id, None)).collect();
    if !row.is_empty() {
        groups.push(row);
    }
}

fn submenu(session: &Session, label: &str, ids: &[&str]) -> Option<serde_json::Value> {
    let items: Vec<_> = ids.iter().filter_map(|id| command_item(session, id, None)).collect();
    if items.is_empty() { None } else { Some(serde_json::json!({ "label": label, "enabled": true, "items": items })) }
}

fn command_item(session: &Session, id: &str, label: Option<&str>) -> Option<serde_json::Value> {
    let spec = aad_engine::find_command(id)?;
    Some(serde_json::json!({
        "label": label.unwrap_or(spec.label),
        "id": id,
        "enabled": (spec.enabled)(session).is_ok(),
        "run": "command",
    }))
}

/// Prompt, history, the layout tabs and the drafting modes the status bar shows.
/// `draft` is the unsubmitted command-line text (Space is literal for `/` requests and sentences).
pub fn chrome_json(session: &Session, cursor: Option<aad_geom::Vec2>, draft: &str) -> String {
    let log = &session.log;
    let start = log.len().saturating_sub(500);
    let history: Vec<&str> = log[start..].iter().map(String::as_str).collect();
    let settings = &session.settings;
    let osnap_off = aad_engine::snap::mode::OFF;
    let mut space = "Model".to_string();
    let mut layer = "0".to_string();
    let mut layouts = vec!["Model".to_string()];
    if let Ok(state) = session.state() {
        space = match &state.space {
            aad_engine::doc::Space::Model => "Model".to_string(),
            aad_engine::doc::Space::Paper(name) => name.clone(),
        };
        layer = state.doc.header.str("CLAYER", "0");
        let mut papers: Vec<_> = state.doc.layouts.iter().collect();
        papers.sort_by_key(|layout| layout.tab_order);
        for layout in papers.into_iter().take(32) {
            layouts.push(layout.name.clone());
        }
    }
    let documents: Vec<_> = session
        .docs
        .iter()
        .take(32)
        .enumerate()
        .map(|(index, doc)| {
            serde_json::json!({
                "index": index,
                "title": doc.title,
                "dirty": doc.is_dirty(),
                "active": index == session.active,
            })
        })
        .collect();
    let cursor = cursor.filter(|point| point.is_finite());
    serde_json::json!({
        "prompt": session.prompt_text(),
        "history": history,
        "space": space,
        "layouts": layouts,
        "documents": documents,
        "layer": layer,
        "ortho": settings.orthomode,
        "polar": settings.polarmode,
        "otrack": settings.otrack,
        "dynmode": settings.dynmode,
        "osnap": settings.osmode & osnap_off == 0 && settings.osmode != 0,
        "osmode": settings.osmode,
        "polarang": settings.polarang.to_degrees(),
        "dynpiCartesian": settings.dynpi_cartesian,
        "dynpiAbsolute": settings.dynpi_absolute,
        "lwdisplay": settings.lwdisplay,
        "transparency": settings.transparency_display,
        "qpmode": settings.qpmode,
        "command": session.running.as_ref().map(|running| running.id.as_str()).unwrap_or(""),
        "literalSpace": crate::bar::space_is_literal(session, draft),
        "version": env!("CARGO_PKG_VERSION"),
        "units": units_label(session),
        "cursor": cursor.map(|point| format!("{:.4}, {:.4}", point.x, point.y)).unwrap_or_default(),
    })
    .to_string()
}

fn units_label(session: &Session) -> String {
    let Ok(drawing) = session.doc() else { return "Units".into() };
    let drawn = aad_engine::annot::drawing_unit(drawing).map_or("units?", |unit| unit.name());
    let shown = aad_engine::annot::display_unit(drawing).map_or(drawn, |unit| unit.name());
    let scale = drawing.header.f64("AADPLOTSCALE", 0.0);
    if scale > 0.0 { format!("{drawn} → {shown}  1:{scale}") } else { format!("{drawn} → {shown}") }
}
