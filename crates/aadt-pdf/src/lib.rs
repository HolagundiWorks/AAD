//! # aadt-pdf — PDF Markup & Comparison (substrate, spec §4.5)
//!
//! Bluebeam-class PDF module that stays part of the substrate because architects
//! must mark up and compare consultant PDFs regardless of AADT's takeoff features:
//! - PDF viewing (planned engine: PDFium or MuPDF).
//! - Markup tools (clouds, callouts, text, highlights, freehand, custom stamps),
//!   non-destructive markup layer, filterable/exportable markup list.
//! - Calibrated measurement (length/area/volume/count) on non-native PDFs.
//! - Document comparison (image- or vector-diff of two revisions).
//! - AADT intersection (spec §6.4): after a `COMPARE`, `AADT` narrates the change
//!   set for transmittals / RFI responses.
//!
//! **Status:** skeleton. Real implementation lands in **M7 — PDF Markup &
//! Comparison** (see `docs/ROADMAP.md`).

/// Milestone in which the PDF module is implemented.
pub const MILESTONE: &str = "M7";
