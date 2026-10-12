//! Every menu entry is a real, reachable control: its command id resolves, activating it does not
//! panic or leave the application stuck, and it has an observable effect (a command prompt, a
//! dialog or panel change, an edit, a message on the command line, or an error message). An entry
//! that does none of these appears interactive but does nothing.
//!
//! Observed effects are compared before and after `CadApp::start(id)` on a fresh application with
//! a small drawing and a selection, so selection-dependent commands are enabled.

use cadcraft_engine::Session;
use cadcraft_ui_egui::menus::{self, Entry};
use cadcraft_ui_egui::{CadApp, Services};
use serde_json::json;

fn app() -> CadApp {
    let mut a = CadApp::new(Session::default(), Services::default());
    a.run("line", json!({"points": [[0, 0], [10, 0]]})).unwrap();
    a.run("circle", json!({"center": [5, 5], "radius": 2})).unwrap();
    a.run("selectall", json!({})).ok();
    a
}

fn items(entries: &[Entry], path: &str, out: &mut Vec<(String, String, bool)>) {
    for e in entries {
        match e {
            Entry::Item { label, id, enabled, .. } => out.push((format!("{path}/{label}"), id.clone(), *enabled)),
            Entry::Sub { label, children } => items(children, &format!("{path}/{label}"), out),
        }
    }
}

fn fingerprint(a: &CadApp) -> String {
    let doc = a.session.doc().map(|d| (d.model.len(), a.session.state().map(|s| s.revision).unwrap_or(0))).unwrap_or((0, 0));
    format!(
        "{}|{}|{}|{}|{}|{:?}|{:?}",
        serde_json::to_string(&a.ui).unwrap_or_default(),
        a.session.running.is_some(),
        doc.0 * 1_000_000 + doc.1 as usize,
        a.session.log.len(),
        a.widgets.form.is_some() as u8
            + 2 * a.status.is_some() as u8
            + 4 * a.cmd.expanded as u8
            + 8 * a.closing.is_some() as u8
            + 16 * a.assistant.open as u8,
        a.session.running.as_ref().map(|r| r.id.clone()),
        a.session.settings.pickadd
    )
}

#[test]
fn every_menu_entry_resolves_and_reacts() {
    let probe = app();
    let mut all = Vec::new();
    for (menu, entries) in menus::tree(&probe) {
        items(&entries, &menu, &mut all);
    }
    assert!(all.len() > 200, "the menu tree is unexpectedly small: {}", all.len());

    let known: std::collections::HashSet<&str> = cadcraft_engine::command_specs().iter().map(|c| c.id).collect();
    let mut unresolved = Vec::new();
    let mut silent = Vec::new();
    let mut checked = 0;
    for (path, id, enabled) in &all {
        if !known.contains(id.as_str()) && !menus::is_ui_command(id) {
            unresolved.push(format!("{path} -> {id}"));
            continue;
        }
        if !enabled {
            continue;
        }
        let mut a = app();
        let before = fingerprint(&a);
        a.start(id);
        let reacted = fingerprint(&a) != before;
        a.session.cancel();
        checked += 1;
        if !reacted {
            silent.push(format!("{path} -> {id}"));
        }
    }
    eprintln!(
        "menu entries: {} total, {} enabled and activated, {} unresolved, {} without an observable effect",
        all.len(),
        checked,
        unresolved.len(),
        silent.len()
    );
    for s in &silent {
        eprintln!("  silent: {s}");
    }
    assert!(unresolved.is_empty(), "menu entries whose command does not exist: {unresolved:#?}");
}

/// Entries that legitimately show nothing in this headless setup, with the reason. A new entry that
/// does nothing is not allowed to join this list without a reason.
const EXPECTED_SILENT: &[(&str, &str)] = &[
    ("open", "asks the platform file dialog (Services::pick_open is absent here)"),
    ("saveas", "asks the platform file dialog (Services::pick_save is absent here)"),
    ("qsave", "an untitled drawing falls back to Save As, which needs the file dialog service"),
    ("ui.dock.reset", "the layout is already the default, so there is nothing to reset"),
    ("ui.panel.properties", "shows a panel that Classic already shows"),
    ("ui.panel.layers", "shows a panel that Classic already shows"),
    ("properties", "shows the Properties panel that Classic already shows"),
    ("ui.workspace.classic", "the workspace is already Classic"),
    ("ui.resetpalettes", "the palettes are already at their defaults"),
    ("ui.workspace.reset", "the workspace is already at its defaults (reset with a changed workspace is tested in workspace_modes)"),
    ("resetui", "the interface is already at its defaults (reset of a changed interface is tested in workspace_modes)"),
];

#[test]
fn entries_without_an_observable_effect_are_only_the_explained_ones() {
    let probe = app();
    let mut all = Vec::new();
    for (menu, entries) in menus::tree(&probe) {
        items(&entries, &menu, &mut all);
    }
    let mut unexplained = Vec::new();
    for (path, id, enabled) in &all {
        if !enabled {
            continue;
        }
        let mut a = app();
        let before = fingerprint(&a);
        a.start(id);
        let reacted = fingerprint(&a) != before;
        a.session.cancel();
        if !reacted && !EXPECTED_SILENT.iter().any(|(e, _)| e == id) {
            unexplained.push(format!("{path} -> {id}"));
        }
    }
    assert!(unexplained.is_empty(), "menu entries with no observable effect and no explanation: {unexplained:#?}");
}
