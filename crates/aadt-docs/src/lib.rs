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
//! - [`compose`] — fit the model drawing into the sheet's drawing area at a
//!   single scale ([`place_drawing`]), and lay several views out on one sheet
//!   ([`present`], `AADT-PRESENT`) — an overall plan plus an enlarged detail, or
//!   two revisions side by side.
//!
//! The placement APIs mirror the takeoff schedule table
//! ([`aadt_takeoff::schedule`]): each draws ordinary geometry through
//! [`Document::transact`](aadt_model::Document::transact), so a title block or a
//! notes block is a single undoable, DXF-round-trippable object — not a special
//! entity type.
//!
//! M5 is complete: sheet setup, title blocks, notes, QA, scaled drawing
//! composition, and `AADT-PRESENT` multi-view layout all land here.

pub mod compose;
pub mod notes;
pub mod plot;
pub mod qa;
pub mod sheet;
pub mod titleblock;

pub use compose::{grid_areas, place_drawing, present, Composed, DrawingArea, View};
pub use notes::{
    floor_schedule_notes, height_notes, notes_from_text, notes_to_text, place_notes, NotesLibrary,
    NotesStyle,
};
pub use plot::{
    compose_plot_sheet, compose_plot_sheet_for_area, compose_plot_sheet_for_selection,
    compose_plot_sheet_oriented, document_from_selection, document_in_window,
    drawing_area_for_preset, drawing_area_for_sheet, orientation_from_bbox,
    orientation_from_extents, orientation_from_points, orientation_from_selection,
    PlotContent, PLOT_DRAWING_MARGIN,
};
pub use qa::{QaFinding, QaSeverity, SheetReview};
pub use sheet::{Orientation, PaperSize, Sheet};
pub use titleblock::{
    NameplatePreset, RevisionRow, TitleBlock, TitleBlockLayout, TitleBlockStyle, HT_LABEL,
    HT_SHEET_NO, HT_TITLE, HT_VALUE, STRIP_GAP_MM, STRIP_WIDTH_FRAC,
};

/// Milestone in which sheet production is implemented.
pub const MILESTONE: &str = "M5";
