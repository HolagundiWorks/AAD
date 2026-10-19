//! What a command-line line is, so a sentence can go to the assistant and a command stays a command.
//! UI-only commands are not in this list: the shell handles those before they reach the host.

use aad_engine::Session;
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

/// Space with no command waiting: run the line unless it is an assistant `/` request.
#[allow(dead_code)] // exercised in unit tests; WinUI asks [`space_is_literal`] over FFI
pub fn space_executes(buffer: &str) -> bool {
    let text = buffer.trim();
    !text.is_empty() && !text.starts_with('/')
}

/// Whether Space should type a space rather than submit.
/// Assistant requests start with `/` and keep their spaces; everything else is CAD (Space runs).
pub fn space_is_literal(session: &Session, buffer: &str) -> bool {
    if session.current_prompt().is_some_and(|prompt| prompt.accept.text && !prompt.accept.point && !prompt.accept.number && prompt.options.is_empty()) {
        return true;
    }
    if session.running.is_some() {
        return false;
    }
    buffer.trim_start().starts_with('/')
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
    fn space_is_literal_only_for_slash_assistant_requests_when_idle() {
        let session = Session::new();
        assert!(space_is_literal(&session, "/"));
        assert!(space_is_literal(&session, "/ what layers"));
        assert!(!space_is_literal(&session, "what is in this drawing"), "without / Space runs commands");
        assert!(!space_is_literal(&session, "line"));
        assert!(!space_is_literal(&session, ""));
    }
}
