//! # aadt-pdf — PDF Markup & Comparison (substrate, spec §4.5 / §6.4)
//!
//! The parts of the Bluebeam-class PDF module that are computable and testable
//! today, deterministic throughout:
//!
//! - [`writer`] — plot a [`Document`](aadt_model::Document) to a **vector PDF**,
//!   hand-written like the `.xlsx` exporter (no PDF crate). Byte-deterministic.
//! - [`markup`] — a non-destructive markup layer: an annotation list filterable
//!   by kind and author, exportable as a markup report.
//! - [`compare`] — diff two revisions by content fingerprint and **narrate** the
//!   change set for a transmittal (spec §6.4), with no AI needed.
//!
//! Deferred to a display/rendering pass: interactive PDF *viewing*, freehand and
//! stamp markup tools, calibrated on-PDF measurement, and raster diff of scanned
//! sheets. A PDF *reader* (PDFium/MuPDF) is the engine those need; this crate is
//! the write/markup/compare half that stands alone.

pub mod compare;
pub mod markup;
pub mod writer;

pub use compare::{compare, Change, Diff};
pub use markup::{Markup, MarkupKind, MarkupList};
pub use writer::{plot, PageSize};

/// Milestone in which the PDF module is implemented.
pub const MILESTONE: &str = "M7";
