//! What a command-line line is, so a sentence can go to the assistant and a command stays a command.
//! UI-only commands are not in this list: the shell handles those before they reach the host.
//!
//! This module is the WinUI/host half of the command-line interaction contract
//! ([docs/COMMAND_BAR.md](../../../../docs/COMMAND_BAR.md)). Keep behaviour in sync with
//! `aad_ui_egui::cmdinput`.

use aad_engine::Session;
use aad_engine::cmd::context::PROVIDERS;
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Empty,
    Native,
    Reference,
    Natural,
    Partial,
}

fn command_known(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }
    let resolved = aad_engine::cmd::resolve_alias(&lower);
    aad_engine::find_command(&resolved).is_some()
}

fn reference_word(word: &str) -> bool {
    let word = word.trim_end_matches([',', '.', ';', ':', '!', '?', ')']);
    word.strip_prefix('/').is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphabetic()))
}

/// Unbalanced `"` or `'` — Space must insert a character, not submit (filenames, text, expressions).
pub fn inside_quoted(buffer: &str) -> bool {
    let mut double = false;
    let mut single = false;
    let mut escape = false;
    for c in buffer.chars() {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' {
            escape = true;
            continue;
        }
        if c == '"' && !single {
            double = !double;
        } else if c == '\'' && !double {
            single = !single;
        }
    }
    double || single
}

pub fn classify(buffer: &str) -> Kind {
    let text = buffer.trim_start();
    if text.is_empty() {
        return Kind::Empty;
    }
    if text.starts_with('/') || text.split_whitespace().any(reference_word) {
        return Kind::Reference;
    }
    let first = text.split_whitespace().next().unwrap_or("");
    if command_known(first) {
        return Kind::Native;
    }
    let words = first.chars().next().is_some_and(char::is_alphabetic);
    if words && text.split_whitespace().count() >= 2 {
        Kind::Natural
    } else {
        Kind::Partial
    }
}

pub fn is_reference_marker(word: &str) -> bool {
    PROVIDERS.iter().any(|provider| provider.name.eq_ignore_ascii_case(word.trim_end_matches([',', '.', ':', ';'])))
}

/// Space with no command waiting: run the line unless it is an assistant `/` request or an open quote.
#[allow(dead_code)] // contract helper; WinUI uses [`space_is_literal`] over FFI; unit tests cover this
pub fn space_executes(buffer: &str) -> bool {
    let text = buffer.trim();
    !text.is_empty() && !text.starts_with('/') && !inside_quoted(buffer)
}

/// Whether Space should type a space rather than submit.
///
/// Literal when: assistant `/` request; pure text prompt; open quote; otherwise CAD (Space runs)
/// except while a non-text prompt is active (Space confirms like Enter).
pub fn space_is_literal(session: &Session, buffer: &str) -> bool {
    if inside_quoted(buffer) {
        return true;
    }
    if session.current_prompt().is_some_and(|prompt| prompt.accept.text && !prompt.accept.point && !prompt.accept.number && prompt.options.is_empty())
    {
        return true;
    }
    if session.running.is_some() {
        return false;
    }
    buffer.trim_start().starts_with('/')
}

/// Idle CAD entry may hand a leading `/` line to the fixed AI prompt.
///
/// Not eligible while a command is running, a pure text prompt is waiting, or quotes are open.
pub fn slash_transfer_eligible(session: &Session, buffer: &str) -> bool {
    if session.running.is_some() {
        return false;
    }
    if inside_quoted(buffer) {
        return false;
    }
    if session.current_prompt().is_some_and(|prompt| prompt.accept.text && !prompt.accept.point && !prompt.accept.number && prompt.options.is_empty())
    {
        return false;
    }
    buffer.trim_start().starts_with('/')
}

/// Text after one leading `/` and the spaces that follow it. Empty when there is no slash.
pub fn slash_transfer_payload(buffer: &str) -> String {
    let text = buffer.trim_start();
    match text.strip_prefix('/') {
        Some(rest) => rest.trim_start().to_string(),
        None => String::new(),
    }
}

fn current_word(buffer: &str) -> &str {
    buffer.rsplit(char::is_whitespace).next().unwrap_or("")
}

/// Autocomplete rows for the draft buffer (commands or `/references`), as JSON for the WinUI shell.
pub fn suggestions_json(buffer: &str) -> String {
    let word = current_word(buffer);
    let rows = if word.starts_with('/') {
        let typed = word.trim_start_matches('/').to_ascii_lowercase();
        PROVIDERS
            .iter()
            .filter(|p| p.name.starts_with(&typed))
            .take(8)
            .map(|p| {
                json!({
                    "insert": format!("/{}", p.name),
                    "label": p.summary,
                    "reference": true,
                })
            })
            .collect::<Vec<_>>()
    } else if !word.is_empty() && !word.contains(char::is_whitespace) && !buffer.trim_start().starts_with('/') {
        let p = word.to_ascii_lowercase();
        let mut v: Vec<(String, &'static str)> = aad_engine::command_specs()
            .iter()
            .filter(|c| !c.id.contains('.') && (c.id.starts_with(&p) || c.aliases.iter().any(|a| *a == p)))
            .map(|c| (c.id.to_ascii_uppercase(), c.label))
            .collect();
        let exact = |id: &str| {
            id.eq_ignore_ascii_case(&p)
                || aad_engine::command_specs().iter().any(|c| c.id.eq_ignore_ascii_case(id) && c.aliases.iter().any(|a| *a == p))
        };
        v.sort_by_key(|(id, _)| (!exact(id), id.len(), id.clone()));
        v.truncate(8);
        v.into_iter().map(|(id, label)| json!({ "insert": id, "label": label, "reference": false })).collect()
    } else {
        Vec::new()
    };
    json!(rows).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_executes_without_a_leading_slash() {
        assert!(space_executes("line"));
        assert!(space_executes("l"), "an alias");
        assert!(space_executes("lin"), "Space still submits; the shell may complete the name");
        assert!(space_executes("audit this"), "without / Space runs the line as a command");
        assert!(!space_executes("/"));
        assert!(!space_executes("/ what is here"));
        assert!(!space_executes(""));
    }

    #[test]
    fn space_stays_literal_inside_unbalanced_quotes() {
        assert!(inside_quoted(r#"open "C:\My"#));
        assert!(!space_executes(r#"open "C:\My Files"#));
        assert!(space_is_literal(&Session::new(), r#"open "C:\My Files"#));
        assert!(!inside_quoted(r#"open "C:\My Files\x.dxf""#));
        assert!(space_executes(r#"open "C:\My Files\x.dxf""#));
    }

    #[test]
    fn space_is_literal_only_for_slash_assistant_requests_when_idle() {
        let session = Session::new();
        assert!(space_is_literal(&session, "/"));
        assert!(space_is_literal(&session, "/ what layers"));
        assert!(!space_is_literal(&session, "what is in this drawing"), "without / Space runs commands");
        assert!(!space_is_literal(&session, "line"));
        assert!(!space_is_literal(&session, ""));
    }

    #[test]
    fn suggestions_list_commands_and_references() {
        let cmds = suggestions_json("lin");
        assert!(cmds.contains("LINE") || cmds.contains("line"), "{cmds}");
        let refs = suggestions_json("/lay");
        assert!(refs.contains("/layers"), "{refs}");
        assert_eq!(suggestions_json("line 0,0"), "[]");
    }

    #[test]
    fn first_suggestion_is_exact_alias_or_shortest_prefix() {
        let e: Vec<serde_json::Value> = serde_json::from_str(&suggestions_json("e")).expect("json");
        assert!(!e.is_empty(), "e should match erase");
        assert_eq!(e[0]["insert"].as_str().unwrap_or("").to_ascii_lowercase(), "erase");
        let lin: Vec<serde_json::Value> = serde_json::from_str(&suggestions_json("lin")).expect("json");
        assert!(!lin.is_empty(), "lin should match line");
        assert_eq!(lin[0]["insert"].as_str().unwrap_or("").to_ascii_lowercase(), "line");
    }

    #[test]
    fn slash_transfer_strips_leading_slash_and_spaces() {
        assert_eq!(slash_transfer_payload("/ create a room 3000 x 4000"), "create a room 3000 x 4000");
        assert_eq!(slash_transfer_payload("/create"), "create");
        assert_eq!(slash_transfer_payload("  /  what layers"), "what layers");
        assert_eq!(slash_transfer_payload("line"), "");
        assert_eq!(slash_transfer_payload(""), "");
    }

    #[test]
    fn slash_transfer_only_when_idle_and_unquoted() {
        let session = Session::new();
        assert!(slash_transfer_eligible(&session, "/"));
        assert!(slash_transfer_eligible(&session, "/ create a room"));
        assert!(!slash_transfer_eligible(&session, "line"));
        assert!(!slash_transfer_eligible(&session, r#"/ open "C:\My"#));
        let mut busy = Session::new();
        busy.cmdline("LINE").expect("start line");
        assert!(busy.running.is_some());
        assert!(!slash_transfer_eligible(&busy, "/ not now"));
        assert!(!slash_transfer_eligible(&busy, "/"));
    }
}
