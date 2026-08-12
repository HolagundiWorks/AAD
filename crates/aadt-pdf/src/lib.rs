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
//! - [`reader`] — open/rasterize pages via PDFium ([ADR-0019](../../docs/adr/0019-pdfium-reader.md));
//!   feature `pdfium` + runtime native library.
//! - [`raster_diff`] — pixel compare of two RGBA pages (L3.3).
//!
//! L3.1 sheet preview + MarkupList UI; L3.2 freehand on preview; L3.3 PDF open.

pub mod compare;
pub mod markup;
pub mod raster_diff;
pub mod reader;
pub mod writer;

pub use compare::{compare, Change, Diff};
pub use markup::{Markup, MarkupKind, MarkupList};
pub use raster_diff::{compare_rgba, RasterDiff};
pub use reader::{open_pdf, pdfium_feature_enabled, render_page, OpenedPdf, PdfError, RenderedPage};
pub use writer::{plot, PageSize};

/// Milestone in which the PDF module is implemented.
pub const MILESTONE: &str = "M7";
