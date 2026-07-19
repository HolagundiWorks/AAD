//! # aadt-lisp — Embedded LISP engine (substrate, spec §4.4)
//!
//! Embedded interpreter targeting AutoLISP syntax compatibility so existing firm
//! routines import with minimal changes. Exposed API: entity
//! creation/query/modification, selection sets, command invocation, dialog/prompt
//! primitives, and sandboxed file I/O — plus **read access to AADT Engine outputs**
//! (wall graph, detected elements, takeoff data), a deliberate extensibility point
//! unique to AADT versus plain CAD LISP.
//!
//! **Scheduling note:** spec §10 places LISP in Phase 1. AADT defers it to **M6** —
//! the command registry (`aadt-command`) is designed LISP-ready from the start so
//! firm migration isn't architecturally blocked, but a full interpreter is not on
//! the critical path to the takeoff differentiator. See `docs/ROADMAP.md`.
//!
//! **Status:** skeleton. Real implementation lands in **M6 — LISP Engine**.

/// Milestone in which the interpreter is implemented.
pub const MILESTONE: &str = "M6";
