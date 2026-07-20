//! # aadt-app — Desktop application shell (spec §3, top of the stack)
//!
//! The command-line-first application that composes the substrate (CAD core,
//! render canvas, command interpreter) with the AADT Engine and the on-demand AI
//! layer. The command line is the primary surface; the canvas is a viewport, not a
//! toolbar-driven editor (spec §2, principle 2).
//!
//! This crate's headless-testable core is [`Session`]: a GUI-agnostic wrapper that
//! wires the [`aadt_command`] registry to a *line-in → feedback-out* loop plus a
//! read-only [`Document`] and a rendering snapshot. A future egui/`wgpu` front-end
//! is **just another driver of the same session** — it feeds the lines a user
//! types and paints [`Session::drawlist`], exactly as the CLI's interactive shell
//! and the unit tests below do. Nothing here links a windowing or GPU crate, so
//! the whole session loop is provable without a display.

use aadt_command::{Context, Interpreter};
use aadt_model::Document;
use aadt_render::{build_drawlist, DrawList};
use aadt_takeoff::{LiveTakeoff, TakeoffTable};

/// Milestone in which the interactive shell first ships.
pub const MILESTONE: &str = "M1";

/// The outcome of feeding one line to a [`Session`]: what the UI should show.
///
/// A blank line or comment yields [`Feedback::Silent`]; a successful command
/// yields [`Feedback::Ok`] with its message; a failure yields [`Feedback::Err`].
/// Errors are values, not panics — a bad command never tears down the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feedback {
    /// Command succeeded; carries its user-facing message.
    Ok(String),
    /// Nothing to report (blank line or comment).
    Silent,
    /// Command failed; carries the error message.
    Err(String),
}

impl Feedback {
    /// The text a terminal driver would print for this feedback, if any.
    pub fn message(&self) -> Option<&str> {
        match self {
            Feedback::Ok(m) => Some(m),
            Feedback::Err(m) => Some(m),
            Feedback::Silent => None,
        }
    }

    pub fn is_err(&self) -> bool {
        matches!(self, Feedback::Err(_))
    }
}

/// A GUI-agnostic interactive session: one [`Interpreter`], one working
/// [`Context`] (document + history), and a transcript of feedback.
///
/// Drivers (the CLI shell, a future egui front-end, tests) call [`feed`] with each
/// line and render the returned [`Feedback`] and [`drawlist`]. Because the session
/// owns the interpreter, a driver can register extra commands (file-I/O, engine)
/// through [`registry_mut`] before the loop starts — which is exactly how the CLI
/// adds `OPEN`/`SAVE`/`TAKEOFF` without this crate depending on the DXF or engine
/// layers.
///
/// [`feed`]: Session::feed
/// [`drawlist`]: Session::drawlist
/// [`registry_mut`]: Session::registry_mut
pub struct Session {
    interp: Interpreter,
    ctx: Context,
    transcript: Vec<Feedback>,
    /// The live takeoff panel's data source. Owned here, recomputed lazily and
    /// only when the document's revision changes — the session neither reruns
    /// detection eagerly nor reimplements the takeoff.
    live: LiveTakeoff,
}

impl Default for Session {
    fn default() -> Self {
        Self::with_builtins()
    }
}

impl Session {
    /// A session whose interpreter carries only the generic drawing built-ins.
    /// Drivers that need file-I/O or engine commands register them via
    /// [`registry_mut`](Session::registry_mut) before feeding lines.
    pub fn with_builtins() -> Self {
        Self::from_interpreter(Interpreter::with_builtins())
    }

    /// A session over a caller-configured interpreter — the seam a front-end uses
    /// to inject its own command set.
    pub fn from_interpreter(interp: Interpreter) -> Self {
        Self {
            interp,
            ctx: Context::new(),
            transcript: Vec::new(),
            live: LiveTakeoff::new(),
        }
    }

    /// Register more commands on this session's interpreter before driving it.
    pub fn registry_mut(&mut self) -> &mut aadt_command::Registry {
        self.interp.registry_mut()
    }

    /// Evaluate one line, record the outcome in the transcript, and return it.
    /// Never panics: interpreter errors become [`Feedback::Err`].
    pub fn feed(&mut self, line: &str) -> Feedback {
        let feedback = match self.interp.eval_line(&mut self.ctx, line) {
            Ok(Some(msg)) => Feedback::Ok(msg),
            Ok(None) => Feedback::Silent,
            Err(e) => Feedback::Err(e),
        };
        self.transcript.push(feedback.clone());
        feedback
    }

    /// Tab-completion candidates for a command-name `prefix` (names + aliases,
    /// canonical casing, sorted). The API a live prompt calls on <Tab>; unit-
    /// tested here even though there is no terminal to press Tab in yet.
    pub fn complete(&self, prefix: &str) -> Vec<String> {
        self.interp.registry().complete(prefix)
    }

    /// The working document, read-only — for rendering and inspection. Mutation
    /// only ever happens through [`feed`](Session::feed) (i.e. registered
    /// commands and `Document::transact`), never by reaching in here.
    pub fn document(&self) -> &Document {
        &self.ctx.doc
    }

    /// A fresh rendering snapshot of the current document. A canvas driver calls
    /// this after any `feed` that changed the document (compare
    /// [`Document::revision`](aadt_model::Document::revision)).
    pub fn drawlist(&self) -> DrawList {
        build_drawlist(&self.ctx.doc)
    }

    /// The current takeoff table for the live panel. Backed by [`LiveTakeoff`],
    /// so this recomputes detection + takeoff **only** when the document has
    /// changed since the last call; an unchanged document returns the cached
    /// table for free. A panel driver calls this to render quantities and can
    /// call it every frame without cost.
    ///
    /// Takes `&mut self` because the cache lives in the session; it does not
    /// mutate the document.
    pub fn takeoff(&mut self) -> &TakeoffTable {
        self.live.table(&self.ctx.doc)
    }

    /// Whether the cached takeoff already matches the document — i.e. the next
    /// [`takeoff`](Session::takeoff) would answer without recomputing. Exposed so
    /// a driver (and the tests) can confirm the revision-gated caching holds:
    /// `true` after a `takeoff()` with no intervening edit, `false` once an edit
    /// bumps the document revision.
    pub fn takeoff_is_current(&self) -> bool {
        self.live.is_current(&self.ctx.doc)
    }

    /// The command history recorded by the interpreter (successful, non-comment
    /// lines), oldest first.
    pub fn history(&self) -> &[String] {
        &self.ctx.history
    }

    /// The feedback transcript, oldest first.
    pub fn transcript(&self) -> &[Feedback] {
        &self.transcript
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feeds_lines_and_tracks_document_state() {
        let mut s = Session::with_builtins();
        assert!(matches!(s.feed("# a comment"), Feedback::Silent));
        assert!(matches!(s.feed(""), Feedback::Silent));

        let fb = s.feed("LINE 0 0 10 0 A-WALL");
        assert!(matches!(fb, Feedback::Ok(_)));
        assert_eq!(s.document().len(), 1);

        s.feed("CIRCLE 5 5 2 A-COL");
        assert_eq!(s.document().len(), 2);

        // Undo flows through the same loop.
        assert!(matches!(s.feed("UNDO"), Feedback::Ok(_)));
        assert_eq!(s.document().len(), 1);
    }

    #[test]
    fn errors_are_values_not_panics() {
        let mut s = Session::with_builtins();
        let fb = s.feed("NOSUCH 1 2");
        assert!(fb.is_err());
        assert_eq!(s.document().len(), 0);
        // The session is still usable after an error.
        assert!(matches!(s.feed("LINE 0 0 1 1"), Feedback::Ok(_)));
        assert_eq!(s.document().len(), 1);
    }

    #[test]
    fn transcript_and_history_accumulate() {
        let mut s = Session::with_builtins();
        s.feed("LINE 0 0 1 0");
        s.feed("bogus");
        s.feed("# comment");
        assert_eq!(s.transcript().len(), 3);
        assert!(s.transcript()[1].is_err());
        // History records only the executed, non-comment lines.
        assert_eq!(s.history(), &["LINE 0 0 1 0".to_string()]);
    }

    #[test]
    fn drawlist_reflects_the_document() {
        let mut s = Session::with_builtins();
        assert!(s.drawlist().is_empty());
        s.feed("LINE 0 0 10 0 A-WALL");
        s.feed("CIRCLE 5 5 2 A-COL");
        let list = s.drawlist();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn completion_is_exposed_on_the_session() {
        let s = Session::with_builtins();
        let c = s.complete("ci");
        assert!(c.contains(&"CIRCLE".to_string()));
        assert!(s.complete("zzz").is_empty());
    }

    #[test]
    fn a_driver_can_register_extra_commands() {
        use aadt_command::{Command, Context as Ctx};

        struct PingCmd;
        impl Command for PingCmd {
            fn name(&self) -> &str {
                "PING"
            }
            fn run(&self, _ctx: &mut Ctx, _args: &[String]) -> Result<String, String> {
                Ok("pong".to_string())
            }
        }

        let mut s = Session::with_builtins();
        s.registry_mut().register(Box::new(PingCmd));
        assert_eq!(s.feed("PING"), Feedback::Ok("pong".to_string()));
        // And it shows up in completion.
        assert!(s.complete("pi").contains(&"PING".to_string()));
    }

    /// Draw a wall (two parallel faces 0.2 apart) and a 4x3 closed room through
    /// the session's command loop — the same inputs `aadt-takeoff`'s own tests use.
    fn draw_wall_and_room(s: &mut Session) {
        s.feed("LINE 0 0 10 0 A-WALL");
        s.feed("LINE 0 0.2 10 0.2 A-WALL");
        s.feed("POLYLINE 0 0 4 0 4 3 0 3 CLOSED A-ROOM");
    }

    #[test]
    fn takeoff_panel_reports_totals_and_reflects_edits() {
        let mut s = Session::with_builtins();
        draw_wall_and_room(&mut s);

        // The panel computes quantities from the current document.
        let walls = s.takeoff().walls.len();
        let area0 = s.takeoff().total_room_area();
        assert_eq!(walls, 1, "one wall detected from the two faces");
        assert!((area0 - 12.0).abs() < 1e-6, "4x3 room = 12, got {area0}");

        // Mutating the document changes the numbers on the next read.
        s.feed("POLYLINE 20 0 22 0 22 2 20 2 CLOSED A-ROOM");
        let area1 = s.takeoff().total_room_area();
        assert!(
            area1 > area0 + 1e-6,
            "adding a room increases area: {area0} -> {area1}"
        );
    }

    #[test]
    fn takeoff_is_revision_gated_not_recomputed_on_a_pure_query() {
        let mut s = Session::with_builtins();

        // Before the first read there is no cached table.
        assert!(!s.takeoff_is_current(), "nothing computed yet");

        draw_wall_and_room(&mut s);
        assert!(!s.takeoff_is_current(), "edits left the panel stale");

        // First read populates the cache; it is then current.
        let _ = s.takeoff();
        assert!(s.takeoff_is_current(), "current right after a read");

        // Pure queries must not invalidate the cache — the whole point of the
        // revision gate is that reading the panel every frame is free.
        let _ = s.drawlist();
        let _ = s.complete("li");
        let _ = s.document().len();
        assert!(s.takeoff_is_current(), "queries do not force a recompute");

        // The next edit bumps the document revision and marks it stale again.
        s.feed("LINE 5 5 6 6 A-MISC");
        assert!(!s.takeoff_is_current(), "an edit invalidates the cache");
        let _ = s.takeoff();
        assert!(s.takeoff_is_current(), "recomputed and current once more");
    }
}
