//! Natural-language command routing.
//!
//! The rule this module exists to enforce: **natural language never produces
//! geometry.** It produces a *command string* — the same text a user could have
//! typed — which the existing deterministic interpreter then executes, or
//! doesn't. There is no path from a sentence to the document that skips the
//! command band, so NL inherits every guarantee the command layer already has:
//! undo, validation, and audit.
//!
//! The router is a deterministic pattern matcher, not a language model. It
//! recognises a small, explicit vocabulary and reports how confident it is. When
//! it cannot map a phrase it says so and offers the closest commands rather than
//! guessing — a wrong command silently executed is far worse than no answer.

/// The outcome of routing a phrase.
#[derive(Debug, Clone, PartialEq)]
pub enum Routed {
    /// The phrase maps to this command string.
    Command {
        /// The command line to run, e.g. `"LINE 0,0 5,0"`.
        command: String,
        /// Confidence in `[0, 1]` that this is what was meant.
        confidence: f32,
        /// Why the router chose it.
        rationale: String,
    },
    /// The phrase could not be mapped. Carries the commands most likely meant.
    Unroutable {
        /// Candidate command names to suggest to the user.
        candidates: Vec<&'static str>,
        /// Why routing failed.
        reason: String,
    },
}

/// One recognised phrasing: trigger words → a command name.
struct Pattern {
    /// Words that must all appear for the pattern to fire.
    triggers: &'static [&'static str],
    /// The command this maps to.
    command: &'static str,
    /// How many coordinate pairs the command needs.
    coords: usize,
}

/// The recognised vocabulary, most specific first — the first full match wins,
/// so a two-coordinate phrasing is preferred over a bare command name.
const PATTERNS: &[Pattern] = &[
    Pattern {
        triggers: &["wall"],
        command: "LINE",
        coords: 2,
    },
    Pattern {
        triggers: &["line"],
        command: "LINE",
        coords: 2,
    },
    Pattern {
        triggers: &["circle"],
        command: "CIRCLE",
        coords: 1,
    },
    Pattern {
        triggers: &["move"],
        command: "MOVE",
        coords: 2,
    },
    Pattern {
        triggers: &["copy"],
        command: "COPY",
        coords: 2,
    },
    Pattern {
        triggers: &["erase"],
        command: "ERASE",
        coords: 0,
    },
    Pattern {
        triggers: &["delete"],
        command: "ERASE",
        coords: 0,
    },
    Pattern {
        triggers: &["dimension"],
        command: "SUGGEST DIMS",
        coords: 0,
    },
    Pattern {
        triggers: &["check"],
        command: "CHECK",
        coords: 0,
    },
    Pattern {
        triggers: &["find"],
        command: "FIND",
        coords: 0,
    },
    Pattern {
        triggers: &["save"],
        command: "SAVE",
        coords: 0,
    },
    Pattern {
        triggers: &["undo"],
        command: "UNDO",
        coords: 0,
    },
];

/// Route a natural-language phrase to a command string.
///
/// Never executes anything and never touches a document — the caller decides
/// whether to run what comes back.
pub fn route(phrase: &str) -> Routed {
    let lower = phrase.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return Routed::Unroutable {
            candidates: vec![],
            reason: "empty phrase".into(),
        };
    }

    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != ',')
        .filter(|w| !w.is_empty())
        .collect();

    let coords = extract_coords(&lower);

    for p in PATTERNS {
        if !p.triggers.iter().all(|t| words.contains(t)) {
            continue;
        }
        if coords.len() < p.coords {
            return Routed::Unroutable {
                candidates: vec![p.command],
                reason: format!(
                    "{} needs {} coordinate pair(s) like `0,0`, found {}",
                    p.command,
                    p.coords,
                    coords.len()
                ),
            };
        }
        // A radius follows the centre for CIRCLE; take it from the trailing number.
        let mut command = p.command.to_string();
        for c in coords.iter().take(p.coords) {
            command.push(' ');
            command.push_str(c);
        }
        if p.command == "CIRCLE" {
            let Some(r) = trailing_number(&lower, &coords) else {
                return Routed::Unroutable {
                    candidates: vec!["CIRCLE"],
                    reason:
                        "CIRCLE needs a radius after the centre, e.g. `circle at 2,2 radius 0.5`"
                            .into(),
                };
            };
            command.push(' ');
            command.push_str(&r);
        }

        // Confidence: an exact trigger with all its coordinates is a strong
        // match; a bare command name with no arguments is weaker.
        let confidence = if p.coords > 0 { 0.9 } else { 0.7 };
        return Routed::Command {
            command,
            confidence,
            rationale: format!("matched {:?} → {}", p.triggers.join(" + "), p.command),
        };
    }

    Routed::Unroutable {
        candidates: nearest_commands(&words),
        reason: format!("no known command matches {phrase:?}"),
    }
}

/// Pull `x,y` coordinate pairs out of a phrase, in order of appearance.
fn extract_coords(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tok in s.split(|c: char| c.is_whitespace()) {
        let t =
            tok.trim_matches(|c: char| !c.is_alphanumeric() && c != ',' && c != '.' && c != '-');
        if let Some((a, b)) = t.split_once(',') {
            if !a.is_empty()
                && !b.is_empty()
                && a.parse::<f64>().is_ok()
                && b.parse::<f64>().is_ok()
            {
                out.push(format!("{a},{b}"));
            }
        }
    }
    out
}

/// The last standalone number in the phrase that is not part of a coordinate —
/// used as a `CIRCLE` radius.
fn trailing_number(s: &str, coords: &[String]) -> Option<String> {
    let used: Vec<&str> = coords.iter().flat_map(|c| c.split(',')).collect();
    let mut last = None;
    for tok in s.split(|c: char| c.is_whitespace()) {
        let t = tok.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '-');
        if t.contains(',') || t.is_empty() {
            continue;
        }
        if t.parse::<f64>().is_ok() && !used.contains(&t) {
            last = Some(t.to_string());
        }
    }
    last
}

/// Commands whose name shares a prefix with any word in the phrase — the
/// "did you mean" list shown when routing fails.
fn nearest_commands(words: &[&str]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for p in PATTERNS {
        for w in words {
            if w.len() >= 3
                && (p.command.to_ascii_lowercase().starts_with(w)
                    || p.triggers.iter().any(|t| t.starts_with(w)))
                && !out.contains(&p.command)
            {
                out.push(p.command);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(phrase: &str) -> String {
        match route(phrase) {
            Routed::Command { command, .. } => command,
            other => panic!("expected a command for {phrase:?}, got {other:?}"),
        }
    }

    #[test]
    fn routes_a_wall_to_a_line_command() {
        assert_eq!(cmd("draw a wall from 0,0 to 5,0"), "LINE 0,0 5,0");
    }

    #[test]
    fn routes_a_line() {
        assert_eq!(cmd("draw a line from 1,1 to 2,3"), "LINE 1,1 2,3");
    }

    #[test]
    fn routes_a_circle_with_radius() {
        assert_eq!(cmd("circle at 2,2 radius 0.5"), "CIRCLE 2,2 0.5");
    }

    #[test]
    fn routes_argument_free_commands() {
        assert_eq!(cmd("undo that"), "UNDO");
        assert_eq!(cmd("check the drawing"), "CHECK");
        assert_eq!(cmd("dimension the walls"), "SUGGEST DIMS");
    }

    #[test]
    fn routing_is_deterministic() {
        let a = route("draw a wall from 0,0 to 5,0");
        let b = route("draw a wall from 0,0 to 5,0");
        assert_eq!(a, b);
    }

    #[test]
    fn negative_and_decimal_coordinates_survive() {
        assert_eq!(
            cmd("draw a wall from -1.5,0 to 2.25,-3"),
            "LINE -1.5,0 2.25,-3"
        );
    }

    #[test]
    fn missing_coordinates_are_refused_not_guessed() {
        match route("draw a wall") {
            Routed::Unroutable { candidates, reason } => {
                assert_eq!(candidates, vec!["LINE"]);
                assert!(reason.contains("coordinate"), "got: {reason}");
            }
            other => panic!("expected refusal, got {other:?}"),
        }
    }

    #[test]
    fn circle_without_radius_is_refused() {
        match route("circle at 2,2") {
            Routed::Unroutable { reason, .. } => {
                assert!(reason.contains("radius"), "got: {reason}")
            }
            other => panic!("expected refusal, got {other:?}"),
        }
    }

    #[test]
    fn unknown_phrases_are_refused_with_no_command() {
        match route("make the building beautiful") {
            Routed::Unroutable { reason, .. } => assert!(reason.contains("no known command")),
            other => panic!("expected refusal, got {other:?}"),
        }
    }

    #[test]
    fn empty_phrase_is_refused() {
        assert!(matches!(route("   "), Routed::Unroutable { .. }));
    }

    #[test]
    fn confidence_is_reported_and_bounded() {
        match route("draw a wall from 0,0 to 5,0") {
            Routed::Command { confidence, .. } => {
                assert!((0.0..=1.0).contains(&confidence));
                assert!(confidence >= 0.8, "a full match should be confident");
            }
            other => panic!("expected a command, got {other:?}"),
        }
    }
}
