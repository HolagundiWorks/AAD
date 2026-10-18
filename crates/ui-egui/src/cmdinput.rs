//! The command bar's input rules, with no drawing and no keyboard in them so they can be tested:
//! what typed text is (a native command, a natural-language request, a `/reference`), when Space
//! executes, and what the suggestion menu offers. The command bar ([`crate::cmdline`]) applies them.
//!
//! Native commands are never defined here: [`command_known`] asks the engine's registry (and the
//! UI's own command list), so aliases, new commands and the existing prompts behave as before.

use aad_engine::cmd::context::PROVIDERS;

/// What the text in the bar is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Empty,
    /// The first word is a command or alias.
    Native,
    /// A request for the assistant that mentions `/references`, or starts with `/`.
    Reference,
    /// Several words and the first is not a command: a natural-language request.
    Natural,
    /// One unknown word: a command being typed (or a typo). Never sent to the assistant.
    Partial,
}

/// Whether `word` names a command: an engine command, an alias, or a UI-only command.
pub fn command_known(word: &str) -> bool {
    let l = word.to_ascii_lowercase();
    if l.is_empty() {
        return false;
    }
    crate::menus::is_ui_command(&l) || aad_engine::find_command(&aad_engine::cmd::resolve_alias(&l)).is_some()
}

/// A whole word of the form `/letters` (trailing punctuation allowed): a reference, not a path.
pub fn is_reference_word(w: &str) -> bool {
    let w = w.trim_end_matches([',', '.', ';', ':', '!', '?', ')']);
    w.strip_prefix('/').is_some_and(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_alphabetic()))
}

/// Classify the buffer (while no command is waiting for input).
pub fn classify(buffer: &str) -> Kind {
    let t = buffer.trim_start();
    if t.is_empty() {
        return Kind::Empty;
    }
    if t.starts_with('/') || t.split_whitespace().any(is_reference_word) {
        return Kind::Reference;
    }
    let first = t.split_whitespace().next().unwrap_or("");
    if command_known(first) {
        return Kind::Native;
    }
    // A coordinate, a number or a JSON body is input for a command, not a sentence.
    let starts_like_words = first.chars().next().is_some_and(char::is_alphabetic);
    if starts_like_words && t.split_whitespace().count() >= 2 { Kind::Natural } else { Kind::Partial }
}

/// Whether the request should go to the assistant rather than the command engine.
pub fn is_ai(kind: Kind) -> bool {
    matches!(kind, Kind::Reference | Kind::Natural)
}

/// Space with no command waiting for input: execute only when the whole text is one resolved
/// command, or a suggestion was explicitly chosen; otherwise it is a space.
pub fn space_executes(buffer: &str, suggestion_chosen: bool) -> bool {
    let t = buffer.trim();
    suggestion_chosen || (!t.is_empty() && !t.contains(char::is_whitespace) && command_known(t))
}

/// One row of the suggestion menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// What accepting it puts in the bar (a command name, or `/name`).
    pub insert: String,
    pub label: String,
    /// Shown dimmed: a reference with nothing to read here (the reason is in `note`).
    pub dim: bool,
    pub note: String,
    pub reference: bool,
}

/// The word being typed at the end of the buffer.
pub fn current_word(buffer: &str) -> &str {
    buffer.rsplit(char::is_whitespace).next().unwrap_or("")
}

/// Whether the word being typed is a `/reference` (the reference menu applies).
pub fn typing_reference(buffer: &str) -> bool {
    current_word(buffer).starts_with('/')
}

/// Reference rows matching what has been typed after the slash. `unavailable` gives the reason a
/// reference has nothing to read in the open drawing.
pub fn reference_entries(buffer: &str, unavailable: &dyn Fn(&str) -> Option<String>) -> Vec<Entry> {
    let typed = current_word(buffer).trim_start_matches('/').to_ascii_lowercase();
    PROVIDERS
        .iter()
        .filter(|p| p.name.starts_with(&typed))
        .map(|p| {
            let why = unavailable(p.name);
            Entry { insert: format!("/{}", p.name), label: p.summary.to_string(), dim: why.is_some(), note: why.unwrap_or_default(), reference: true }
        })
        .collect()
}

/// Command rows for a single typed word: command names and aliases that begin with it (the existing
/// AutoComplete: top-level commands only, short names first).
pub fn command_entries(prefix: &str) -> Vec<Entry> {
    if prefix.is_empty() || prefix.contains(char::is_whitespace) || prefix.starts_with('/') {
        return Vec::new();
    }
    let p = prefix.to_ascii_lowercase();
    let mut v: Vec<(String, &'static str)> = aad_engine::command_specs()
        .iter()
        .filter(|c| !c.id.contains('.') && (c.id.starts_with(&p) || c.aliases.iter().any(|a| *a == p)))
        .map(|c| (c.id.to_ascii_uppercase(), c.label))
        .collect();
    // An exact name or alias first, then the shortest names: the first entry is what Space or Enter runs.
    let exact = |id: &str| {
        id.eq_ignore_ascii_case(&p) || aad_engine::command_specs().iter().any(|c| c.id.eq_ignore_ascii_case(id) && c.aliases.iter().any(|a| *a == p))
    };
    v.sort_by_key(|(id, _)| (!exact(id), id.len(), id.clone()));
    v.truncate(8);
    v.into_iter().map(|(id, label)| Entry { insert: id, label: label.to_string(), dim: false, note: String::new(), reference: false }).collect()
}

/// The command name (upper case) for a whole command name or alias; `None` for anything else.
pub fn exact_command(buffer: &str) -> Option<String> {
    let t = buffer.trim();
    command_entries(t).into_iter().next().map(|e| e.insert).filter(|id| {
        id.eq_ignore_ascii_case(t)
            || aad_engine::command_specs().iter().any(|c| c.id.eq_ignore_ascii_case(id) && c.aliases.iter().any(|a| a.eq_ignore_ascii_case(t)))
    })
}

/// Replace the word being typed with `insert` plus a space, so the next word can follow.
pub fn accept_reference(buffer: &str, insert: &str) -> String {
    let cut = buffer.len() - current_word(buffer).len();
    format!("{}{} ", &buffer[..cut], insert)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_aliases_and_ui_commands_are_native() {
        assert_eq!(classify("line 0,0 10,10"), Kind::Native);
        assert_eq!(classify("L"), Kind::Native, "an alias");
        assert_eq!(classify("resetui"), Kind::Native, "a UI-only command");
        assert_eq!(classify("zoom e"), Kind::Native);
        assert_eq!(classify(""), Kind::Empty);
    }

    #[test]
    fn sentences_and_references_are_for_the_assistant_and_typos_and_numbers_are_not() {
        assert_eq!(classify("tell me what this drawing contains"), Kind::Natural);
        assert_eq!(
            classify("audit this drawing"),
            Kind::Native,
            "a sentence that starts with a command word runs the command: use a leading / to ask the assistant"
        );
        assert_eq!(classify("what layers are locked"), Kind::Natural);
        assert_eq!(classify("/rooms"), Kind::Reference);
        assert_eq!(classify("list /layers please"), Kind::Reference, "a reference anywhere makes it a request");
        assert_eq!(classify("/audit this"), Kind::Reference, "a leading slash is always the assistant");
        assert_eq!(classify("lnie"), Kind::Partial, "one unknown word is a typo, not a request");
        assert_eq!(classify("open /tmp/x.dxf"), Kind::Native, "a path is not a reference");
        assert_eq!(classify("0,0 10,10"), Kind::Partial, "coordinates are input for a command");
        assert_eq!(classify("{\"a\": 1}"), Kind::Partial);
        assert!(is_ai(Kind::Natural) && is_ai(Kind::Reference) && !is_ai(Kind::Native) && !is_ai(Kind::Partial));
    }

    #[test]
    fn space_executes_only_a_whole_resolved_command_or_a_chosen_suggestion() {
        assert!(space_executes("line", false));
        assert!(space_executes("l", false), "an alias");
        assert!(!space_executes("lin", false), "a partial name is just text");
        assert!(!space_executes("audit this", false), "a sentence keeps its spaces");
        assert!(!space_executes("", false));
        assert!(space_executes("lin", true), "an explicitly chosen suggestion runs");
        assert!(!space_executes("circle 5", false), "arguments typed after the name are not a command word");
    }

    #[test]
    fn the_reference_menu_filters_by_what_follows_the_slash_and_marks_what_has_no_data() {
        let none = |_: &str| None;
        let all = reference_entries("/", &none);
        assert_eq!(all.len(), PROVIDERS.len());
        assert!(all.iter().all(|e| e.reference && e.insert.starts_with('/')));
        let r = reference_entries("show /ro", &none);
        assert_eq!(r.iter().map(|e| e.insert.as_str()).collect::<Vec<_>>(), ["/rooms"]);
        let gone = |n: &str| (n == "rooms").then(|| "no room labels".to_string());
        let rooms = reference_entries("/r", &gone);
        assert!(rooms[0].dim && rooms[0].note == "no room labels");
        assert!(reference_entries("/zzz", &none).is_empty());
    }

    #[test]
    fn accepting_a_reference_replaces_only_the_word_being_typed() {
        assert_eq!(accept_reference("/ro", "/rooms"), "/rooms ");
        assert_eq!(accept_reference("check /ro", "/rooms"), "check /rooms ");
        assert_eq!(accept_reference("a /layers and /fl", "/floor"), "a /layers and /floor ");
        assert!(typing_reference("a /fl") && !typing_reference("a fl") && !typing_reference("a /floor "));
    }

    #[test]
    fn command_suggestions_are_the_existing_autocomplete() {
        let e = command_entries("cir");
        assert!(e.iter().any(|x| x.insert == "CIRCLE"));
        assert!(command_entries("").is_empty() && command_entries("/x").is_empty() && command_entries("two words").is_empty());
    }
}
