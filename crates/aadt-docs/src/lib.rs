//! # aadt-docs — Documentation & Sheet Production (flagship, spec §5.3)
//!
//! Deterministic-by-default paperwork workflows with AI as an explicit,
//! overridable accelerant:
//! - Auto title block & general notes (`PLOTSETUP` / `AADT-NOTES`): sheet-size and
//!   template selection, auto-filled sheet number / project / date / scale, and a
//!   keyboard-navigable notes checklist backed by the firm notes library.
//! - Firm-customizable title-block templates with attribute mapping.
//! - Per-firm/per-project-type general notes library.
//! - Presentation prep (`AADT-PRESENT`) and documentation QA (completeness checks).
//!
//! **Status:** skeleton. Real implementation lands in **M5 — Documentation & Sheet
//! Production** (see `docs/ROADMAP.md`).

/// Milestone in which sheet production is implemented.
pub const MILESTONE: &str = "M5";
