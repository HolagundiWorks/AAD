//! # aadt-app — Desktop application shell (spec §3, top of the stack)
//!
//! The command-line-first application that composes the substrate (CAD core,
//! render canvas, command interpreter) with the AADT Engine and the on-demand AI
//! layer. The command line is the primary surface; the canvas is a viewport, not a
//! toolbar-driven editor (spec §2, principle 2).
//!
//! This crate's headless-testable core is [`Session`]: a GUI-agnostic wrapper that
//! wires the [`aadt_command`] registry to a *line-in → feedback-out* loop plus a
//! read-only [`Document`] and a rendering snapshot. The egui front-end is **just
//! another driver of the same session** — it feeds the lines a user types and
//! paints [`Session::drawlist`], exactly as the CLI's interactive shell and the
//! unit tests below do. LISP (`LISP` / bare `(…)`) is evaluated here against the
//! live document so `defun`s persist for the life of the session.

#[cfg(feature = "gui")]
pub mod gui;
pub mod osnap;
pub mod prefs;
pub mod selection;

use std::cell::RefCell;
use std::rc::Rc;

use aadt_command::{Context, Interpreter};
use aadt_model::Document;
use aadt_render::{build_drawlist, DrawList};

/// Persistent LISP host: one interpreter for the session so `defun`s stick, with
/// a shared document cell swapped with [`Context::doc`] around each eval.
struct LispHost {
    interp: aadt_lisp::Interpreter,
    doc: Rc<RefCell<Document>>,
}

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
    /// Lazily created on first `LISP` / `(…)` / `LISPLOAD`.
    lisp: Option<LispHost>,
    /// Cached drawlist keyed by document revision.
    draw_cache: Option<(u64, DrawList)>,
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
        let mut ctx = Context::new();
        aadt_standards::apply_standard_layers(&mut ctx.doc);
        Self {
            interp,
            ctx,
            transcript: Vec::new(),
            lisp: None,
            draw_cache: None,
        }
    }

    /// Register more commands on this session's interpreter before driving it.
    pub fn registry_mut(&mut self) -> &mut aadt_command::Registry {
        self.interp.registry_mut()
    }

    /// Evaluate one line, record the outcome in the transcript, and return it.
    /// Never panics: interpreter errors become [`Feedback::Err`].
    ///
    /// Lines starting with `(` are AutoLISP; `LISP <expr>` and `LISPLOAD path`
    /// are handled here so firm routines share the live document and persist
    /// `defun`s for the life of the session.
    pub fn feed(&mut self, line: &str) -> Feedback {
        let trimmed = line.trim();
        let feedback = if let Some(fb) = self.try_feed_lisp(trimmed) {
            fb
        } else {
            match self.interp.eval_line(&mut self.ctx, line) {
                Ok(Some(msg)) => Feedback::Ok(msg),
                Ok(None) => Feedback::Silent,
                Err(e) => Feedback::Err(e),
            }
        };
        self.transcript.push(feedback.clone());
        feedback
    }

    /// Handle LISP forms; returns `None` when the line is not a LISP entry.
    fn try_feed_lisp(&mut self, trimmed: &str) -> Option<Feedback> {
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            return None;
        }
        if trimmed.starts_with('(') {
            return Some(self.eval_lisp(trimmed));
        }
        let (head, rest) = match trimmed.split_once(char::is_whitespace) {
            Some((h, r)) => (h, r.trim()),
            None => (trimmed, ""),
        };
        match head.to_ascii_uppercase().as_str() {
            "LISP" => {
                if rest.is_empty() {
                    Some(Feedback::Err(
                        "LISP: missing expression (try LISP (+ 1 2) or bare (…))".into(),
                    ))
                } else {
                    Some(self.eval_lisp(rest))
                }
            }
            "LISPLOAD" => {
                if rest.is_empty() {
                    Some(Feedback::Err("LISPLOAD: missing path".into()))
                } else {
                    match std::fs::read_to_string(rest) {
                        Ok(src) => Some(self.eval_lisp(&src)),
                        Err(e) => Some(Feedback::Err(format!("LISPLOAD: reading {rest}: {e}"))),
                    }
                }
            }
            _ => None,
        }
    }

    fn ensure_lisp(&mut self) {
        if self.lisp.is_some() {
            return;
        }
        let doc = Rc::new(RefCell::new(Document::new()));
        let interp = aadt_lisp::Interpreter::new();
        aadt_lisp::host::install_document(interp.env(), doc.clone());
        self.lisp = Some(LispHost { interp, doc });
    }

    fn eval_lisp(&mut self, src: &str) -> Feedback {
        self.ensure_lisp();
        let host = self.lisp.as_mut().expect("ensure_lisp");
        let taken = std::mem::replace(&mut self.ctx.doc, Document::new());
        *host.doc.borrow_mut() = taken;
        let result = host.interp.eval_str(src);
        self.ctx.doc = std::mem::replace(&mut *host.doc.borrow_mut(), Document::new());
        match result {
            Ok(value) => {
                let printed = host.interp.output();
                let mut msg = value.to_string();
                if !printed.is_empty() {
                    let t = printed.trim_end();
                    msg = format!("{t}\n=> {msg}");
                }
                Feedback::Ok(msg)
            }
            Err(e) => Feedback::Err(e.to_string()),
        }
    }

    /// Tab-completion candidates for a command-name `prefix` (names + aliases,
    /// canonical casing, sorted). The API a live prompt calls on <Tab>; unit-
    /// tested here even though there is no terminal to press Tab in yet.
    pub fn complete(&self, prefix: &str) -> Vec<String> {
        let mut out = self.interp.registry().complete(prefix);
        // Session-handled power commands (not on the substrate registry).
        for name in ["LISP", "LISPLOAD"] {
            if name.to_lowercase().starts_with(&prefix.to_lowercase())
                && !out.iter().any(|s| s.eq_ignore_ascii_case(name))
            {
                out.push(name.to_string());
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The working document, read-only — for rendering and inspection. Mutation
    /// only ever happens through [`feed`](Session::feed) (i.e. registered
    /// commands and `Document::transact`), never by reaching in here.
    pub fn document(&self) -> &Document {
        &self.ctx.doc
    }

    /// Mutable access for GUI helpers that must seed blocks before INSERT.
    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.ctx.doc
    }

    /// Replace the working document — a front-end opening a file at launch. This
    /// is a bulk load, not a tracked edit: it does not push undo history, exactly
    /// like importing a DXF (spec §4.1). Interactive edits still go through
    /// [`feed`](Session::feed).
    pub fn set_document(&mut self, doc: Document) {
        self.ctx.doc = doc;
        self.draw_cache = None;
    }

    /// Replace Drawing Property Manager fields (not undoable).
    pub fn set_drawing_properties(&mut self, props: aadt_model::DrawingProperties) {
        self.ctx.doc.set_properties(props);
        self.draw_cache = None;
    }

    /// Rendering snapshot of the current document. Rebuilt only when
    /// [`Document::revision`](aadt_model::Document::revision) changes — safe to
    /// call every frame.
    pub fn drawlist(&mut self) -> &DrawList {
        self.refresh_draw_cache();
        &self.draw_cache.as_ref().expect("cache populated").1
    }

    /// Ensure the drawlist cache matches the current revision (no borrow held).
    pub fn refresh_draw_cache(&mut self) {
        let rev = self.ctx.doc.revision();
        let stale = match &self.draw_cache {
            Some((cached, _)) => *cached != rev,
            None => true,
        };
        if stale {
            self.draw_cache = Some((rev, build_drawlist(&self.ctx.doc)));
        }
    }

    /// Borrow the cached drawlist. Call [`refresh_draw_cache`] first each frame.
    pub fn cached_drawlist(&self) -> &DrawList {
        static EMPTY: std::sync::OnceLock<DrawList> = std::sync::OnceLock::new();
        self.draw_cache
            .as_ref()
            .map(|(_, list)| list)
            .unwrap_or_else(|| EMPTY.get_or_init(DrawList::default))
    }

    /// True when the drawlist cache matches the current document revision.
    #[cfg(test)]
    pub fn drawlist_cache_hit(&self) -> bool {
        self.draw_cache
            .as_ref()
            .map(|(r, _)| *r == self.ctx.doc.revision())
            .unwrap_or(false)
    }

    /// The command history recorded by the interpreter (successful, non-comment
    /// lines), oldest first.
    pub fn history(&self) -> &[String] {
        &self.ctx.history
    }

    /// The current selection set (the last `SELECT`), for a front-end to
    /// highlight. Read-only; the GUI drives it through `SELECT` like any command.
    pub fn selection(&self) -> &[aadt_model::EntityId] {
        &self.ctx.selection
    }

    /// The feedback transcript, oldest first.
    pub fn transcript(&self) -> &[Feedback] {
        &self.transcript
    }

    /// Take a pending navigation hint from ZOOM/PAN (`extents`, `window:…`, …).
    pub fn take_nav_hint(&mut self) -> Option<String> {
        self.ctx.nav_hint.take()
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
    fn mutating_feed_bumps_revision_for_dirty_tracking() {
        let mut s = Session::with_builtins();
        let r0 = s.document().revision();
        s.feed("LINE 0 0 10 0 A-WALL");
        assert!(
            s.document().revision() > r0,
            "GUI dirty flag keys off revision change after feed"
        );
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
    fn drawlist_is_revision_cached() {
        let mut s = Session::with_builtins();
        s.feed("LINE 0 0 10 0 A-WALL");
        let _ = s.drawlist();
        assert!(s.drawlist_cache_hit());
        let len1 = s.drawlist().len();
        let _ = s.drawlist(); // second call must hit cache
        assert!(s.drawlist_cache_hit());
        assert_eq!(s.drawlist().len(), len1);
        s.feed("CIRCLE 1 1 0.5");
        assert!(!s.drawlist_cache_hit());
        let _ = s.drawlist();
        assert!(s.drawlist_cache_hit());
        assert_eq!(s.drawlist().len(), 2);
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

    #[test]
    fn lisp_eval_and_defun_persist_across_feeds() {
        let mut s = Session::with_builtins();
        assert!(matches!(s.feed("(+ 1 2)"), Feedback::Ok(m) if m.contains('3')));
        assert!(matches!(
            s.feed("LISP (defun sq (x) (* x x))"),
            Feedback::Ok(_)
        ));
        let fb = s.feed("(sq 9)");
        assert!(
            matches!(fb, Feedback::Ok(ref m) if m.contains("81")),
            "{fb:?}"
        );
    }

    #[test]
    fn lisp_can_draw_into_the_document() {
        let mut s = Session::with_builtins();
        let fb = s.feed(r#"(aadt-add-line "0" 0 0 10 0)"#);
        assert!(matches!(fb, Feedback::Ok(_)), "{fb:?}");
        assert_eq!(s.document().len(), 1);
        assert!(matches!(s.feed("(aadt-entity-count)"), Feedback::Ok(m) if m.contains('1')));
    }
}
