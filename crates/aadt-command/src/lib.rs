//! # aadt-command — Command interpreter (substrate layer, spec §4.2)
//!
//! Single-line command interpreter that is AADT's primary UI surface: a command
//! registry with aliases, history, and tab-completion, plus script/batch
//! execution for power users.
//!
//! The registry is intentionally designed to be **LISP-ready** (spec §4.4): every
//! command is a named handler with typed prompts, so the M6 Scheme interpreter can
//! invoke commands and register new ones without a redesign.
//!
//! **Status:** skeleton. Real implementation lands in **M1 — CAD Core Substrate**
//! (see `docs/ROADMAP.md`).

/// Marker for the command layer's build status. Replaced by the real registry in M1.
pub const MILESTONE: &str = "M1";
