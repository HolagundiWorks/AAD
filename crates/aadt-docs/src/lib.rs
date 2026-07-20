//! # aadt-docs — Documentation & Sheet Production (flagship, spec §5.3)
//!
//! Deterministic-by-default paperwork. Everything here is computed geometry and
//! computed checks — no model required — with AI positioned only as an
//! overridable accelerant on top (it may rewrite a suggested note; it does not
//! decide the sheet size, fill the title block, or judge completeness).
//!
//! - [`sheet`] — `PLOTSETUP`: paper size, orientation, and the drawing border.
//! - [`titleblock`] — the labelled information box, placed as `LINE`/`TEXT`
//!   geometry in a sheet corner, exporting to DXF and undoing as one unit.
//! - [`notes`] — the firm notes library, numbered placement, and rule-based
//!   suggestion of the notes a drawing's detected elements call for.
//! - [`qa`] — documentation QA: deterministic completeness checks that gate
//!   whether a sheet is ready to issue.
//!
//! The placement APIs mirror the takeoff schedule table
//! ([`aadt_takeoff::schedule`]): each draws ordinary geometry through
//! [`Document::transact`](aadt_model::Document::transact), so a title block or a
//! notes block is a single undoable, DXF-round-trippable object — not a special
//! entity type.
//!
//! Still ahead in M5: fitting model space into the sheet viewport at scale, and
//! `AADT-PRESENT` layout composition. Those build on this geometry core.

pub mod notes;
pub mod qa;
pub mod sheet;
pub mod titleblock;

pub use notes::{place_notes, NotesLibrary, NotesStyle};
pub use qa::{QaFinding, QaSeverity, SheetReview};
pub use sheet::{Orientation, PaperSize, Sheet};
pub use titleblock::{TitleBlock, TitleBlockStyle};

/// Milestone in which sheet production is implemented.
pub const MILESTONE: &str = "M5";
