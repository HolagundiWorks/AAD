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

/// Prompt and the recent command-line history.
pub fn chrome_json(session: &Session) -> String {
    let log = &session.log;
    let start = log.len().saturating_sub(12);
    let history: Vec<&str> = log[start..].iter().map(String::as_str).collect();
    serde_json::json!({
        "prompt": session.prompt_text(),
        "history": history,
    })
    .to_string()
}
