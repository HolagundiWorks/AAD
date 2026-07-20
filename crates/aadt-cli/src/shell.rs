//! Interactive command shell — the `aadt shell` entry point.
//!
//! A thin stdin→feedback→stdout driver over [`aadt_app::Session`]. The session is
//! the reusable, GUI-agnostic core (it owns the interpreter, document, and
//! completion); this module is just *one* driver of it, exactly as a future
//! egui/`wgpu` front-end would be. It registers the same file-I/O and engine
//! commands (`OPEN`/`SAVE`/`TAKEOFF`/`SCHEDULE`) the batch `run` path uses, so the
//! prompt and scripts share one command set.
//!
//! Meta-commands (prefixed `:`, so they never clash with drawing commands):
//! `:help` lists commands, `:complete <prefix>` shows tab-completion candidates,
//! and `:quit` / `:exit` leave. The loop is factored into [`drive`], which reads
//! from any [`BufRead`] and writes to any [`Write`], so it is unit-tested without a
//! live terminal.

use std::io::{BufRead, Write};

use aadt_app::{Feedback, Session};
use aadt_command::Interpreter;

/// Build a session carrying the drawing built-ins plus the CLI's file-I/O and
/// engine commands. Mirrors the registration the batch `run` path performs, so
/// the interactive shell and scripts expose the identical command set.
fn engine_session() -> Session {
    let mut interp = Interpreter::with_builtins();
    crate::commands::register(&mut interp);
    Session::from_interpreter(interp)
}

/// Run the read→eval→feedback loop over `input`, writing feedback (and, when
/// `prompt` is set, that prompt before each read) to `out`.
///
/// Returns on EOF or an explicit `:quit`. Errors from commands are printed and the
/// loop continues — a bad line never ends the session. Factored out of
/// [`run`](crate::shell::run) so the whole loop is testable against in-memory
/// buffers.
pub fn drive<R: BufRead, W: Write>(
    mut input: R,
    out: &mut W,
    prompt: Option<&str>,
) -> std::io::Result<()> {
    let mut session = engine_session();
    let mut line = String::new();
    loop {
        if let Some(p) = prompt {
            write!(out, "{p}")?;
            out.flush()?;
        }
        line.clear();
        if input.read_line(&mut line)? == 0 {
            break; // EOF
        }
        let trimmed = line.trim();
        if let Some(meta) = trimmed.strip_prefix(':') {
            if handle_meta(meta, &session, out)? {
                break;
            }
            continue;
        }
        match session.feed(trimmed) {
            Feedback::Ok(msg) => writeln!(out, "{msg}")?,
            Feedback::Err(msg) => writeln!(out, "error: {msg}")?,
            Feedback::Silent => {}
        }
    }
    Ok(())
}

/// Handle a `:`-prefixed meta-command. Returns `Ok(true)` when the loop should
/// exit (`:quit` / `:exit`).
fn handle_meta<W: Write>(meta: &str, session: &Session, out: &mut W) -> std::io::Result<bool> {
    let mut parts = meta.split_whitespace();
    match parts.next() {
        Some("quit") | Some("exit") => return Ok(true),
        Some("help") => {
            writeln!(out, "commands: {}", session.complete("").join(" "))?;
        }
        Some("complete") => {
            let prefix = parts.next().unwrap_or("");
            let hits = session.complete(prefix);
            if hits.is_empty() {
                writeln!(out, "(no completions for {prefix:?})")?;
            } else {
                writeln!(out, "{}", hits.join(" "))?;
            }
        }
        Some(other) => writeln!(out, "error: unknown meta-command {other:?}")?,
        None => {}
    }
    Ok(false)
}

/// Drive the interactive shell against the real stdin/stdout.
pub fn run() -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    drive(stdin.lock(), &mut stdout, Some("aadt> ")).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a canned block of input through the loop with no prompt and return
    /// everything written to stdout.
    fn transcript(input: &str) -> String {
        let mut out = Vec::new();
        drive(input.as_bytes(), &mut out, None).expect("drive over a byte slice");
        String::from_utf8(out).expect("utf8 output")
    }

    #[test]
    fn drives_draw_commands_and_reports_feedback() {
        let out = transcript("LINE 0 0 10 0 A-WALL\nCIRCLE 5 5 2 A-COL\nLIST\n");
        assert!(out.contains("entity 0"), "line feedback: {out}");
        assert!(out.contains("CIRCLE"), "list shows the circle: {out}");
    }

    #[test]
    fn a_bad_line_is_reported_and_the_loop_survives() {
        let out = transcript("NOSUCH 1\nLINE 0 0 1 0\n");
        assert!(out.contains("error:"), "bad line reported: {out}");
        assert!(out.contains("entity 0"), "loop kept going: {out}");
    }

    #[test]
    fn comments_and_blank_lines_are_silent() {
        let out = transcript("# just a comment\n\n; also a comment\n");
        assert!(out.trim().is_empty(), "no output for silent lines: {out:?}");
    }

    #[test]
    fn meta_complete_lists_candidates() {
        let out = transcript(":complete ci\n:complete zzz\n");
        assert!(out.contains("CIRCLE"), "completion listed: {out}");
        assert!(out.contains("no completions"), "empty case reported: {out}");
    }

    #[test]
    fn meta_help_lists_engine_commands_too() {
        // OPEN/SAVE/TAKEOFF are registered by the CLI, proving the shell shares
        // the full command set with the batch `run` path.
        let out = transcript(":help\n");
        assert!(out.contains("OPEN"), "engine commands present: {out}");
        assert!(out.contains("TAKEOFF"), "engine commands present: {out}");
    }

    #[test]
    fn quit_stops_the_loop_early() {
        // The LINE after :quit must never run.
        let out = transcript(":quit\nLINE 0 0 1 0\n");
        assert!(!out.contains("entity"), "input after :quit ignored: {out}");
    }
}
