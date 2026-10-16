//! What a command-line line is, so a sentence can go to the assistant and a command stays a command.
//! UI-only commands are not in this list: the shell handles those before they reach the host.

use aad_engine::cmd::context::PROVIDERS;

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
    if words && text.split_whitespace().count() >= 2 { Kind::Natural } else { Kind::Partial }
}

pub fn is_reference_marker(word: &str) -> bool {
    PROVIDERS.iter().any(|provider| provider.name.eq_ignore_ascii_case(word.trim_end_matches([',', '.', ':', ';'])))
}
