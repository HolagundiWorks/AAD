//! Sheet setup (`PLOTSETUP`) — paper size, orientation, and the drawing border.
//!
//! A sheet is described in **millimetres of paper**. Its only geometry is the
//! border rectangle, inset from the paper edge by a margin; a title block
//! ([`crate::titleblock`]) and general notes ([`crate::notes`]) place inside it.

use aadt_geometry::{Point2, Polyline};
use aadt_model::{Document, EntityId, Geometry};

/// Layer the sheet border is drawn on.
pub const SHEET_LAYER: &str = "A-SHEET";

/// A named paper size. Dimensions are returned in **millimetres, landscape**
/// (width ≥ height); [`Orientation`] rotates them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaperSize {
    IsoA4,
    IsoA3,
    IsoA2,
    IsoA1,
    IsoA0,
    AnsiA,
    AnsiB,
    AnsiC,
    AnsiD,
    AnsiE,
    ArchA,
    ArchB,
    ArchC,
    ArchD,
    ArchE,
    /// A custom size in millimetres (width, height); stored landscape.
    Custom(f64, f64),
}

impl PaperSize {
    /// Landscape dimensions in millimetres `(width, height)`, width ≥ height.
    pub fn dimensions(self) -> (f64, f64) {
        match self {
            PaperSize::IsoA4 => (297.0, 210.0),
            PaperSize::IsoA3 => (420.0, 297.0),
            PaperSize::IsoA2 => (594.0, 420.0),
            PaperSize::IsoA1 => (841.0, 594.0),
            PaperSize::IsoA0 => (1189.0, 841.0),
            PaperSize::AnsiA => (279.4, 215.9),
            PaperSize::AnsiB => (431.8, 279.4),
            PaperSize::AnsiC => (558.8, 431.8),
            PaperSize::AnsiD => (863.6, 558.8),
            PaperSize::AnsiE => (1117.6, 863.6),
            PaperSize::ArchA => (304.8, 228.6),
            PaperSize::ArchB => (457.2, 304.8),
            PaperSize::ArchC => (609.6, 457.2),
            PaperSize::ArchD => (914.4, 609.6),
            PaperSize::ArchE => (1219.2, 914.4),
            // Normalise a custom size to landscape so `dimensions` keeps its
            // width ≥ height contract regardless of how it was supplied.
            PaperSize::Custom(w, h) => (w.max(h), w.min(h)),
        }
    }

    /// Parse a size name (case-insensitive), e.g. `A1`, `ANSI-D`, `ARCH C`.
    pub fn from_name(name: &str) -> Option<PaperSize> {
        let n: String = name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_uppercase();
        Some(match n.as_str() {
            "A4" | "ISOA4" => PaperSize::IsoA4,
            "A3" | "ISOA3" => PaperSize::IsoA3,
            "A2" | "ISOA2" => PaperSize::IsoA2,
            "A1" | "ISOA1" => PaperSize::IsoA1,
            "A0" | "ISOA0" => PaperSize::IsoA0,
            "ANSIA" => PaperSize::AnsiA,
            "ANSIB" => PaperSize::AnsiB,
            "ANSIC" => PaperSize::AnsiC,
            "ANSID" => PaperSize::AnsiD,
            "ANSIE" => PaperSize::AnsiE,
            "ARCHA" => PaperSize::ArchA,
            "ARCHB" => PaperSize::ArchB,
            "ARCHC" => PaperSize::ArchC,
            "ARCHD" => PaperSize::ArchD,
            "ARCHE" => PaperSize::ArchE,
            _ => return None,
        })
    }
}

/// Portrait swaps the paper's width and height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Landscape,
    Portrait,
}

/// A sheet to plot onto: a paper size, an orientation, and a border margin (mm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sheet {
    pub size: PaperSize,
    pub orientation: Orientation,
    /// Border inset from the paper edge, in millimetres.
    pub margin: f64,
}

impl Sheet {
    /// An A1 landscape sheet with a 10 mm margin — a common working default.
    pub fn a1_landscape() -> Self {
        Self {
            size: PaperSize::IsoA1,
            orientation: Orientation::Landscape,
            margin: 10.0,
        }
    }

    pub fn new(size: PaperSize, orientation: Orientation, margin: f64) -> Self {
        Self {
            size,
            orientation,
            margin,
        }
    }

    /// Outer paper size in millimetres `(width, height)`, honouring orientation.
    pub fn extent(&self) -> (f64, f64) {
        let (w, h) = self.size.dimensions();
        match self.orientation {
            Orientation::Landscape => (w, h),
            Orientation::Portrait => (h, w),
        }
    }

    /// The usable inner rectangle `(min, max)` inside the margin. Origin is the
    /// bottom-left of the paper.
    pub fn inner(&self) -> (Point2, Point2) {
        let (w, h) = self.extent();
        // Clamp the margin so a too-large value cannot invert the rectangle.
        let m = self.margin.max(0.0).min(w.min(h) * 0.49);
        (Point2::new(m, m), Point2::new(w - m, h - m))
    }

    /// The border as a closed rectangle polyline.
    pub fn border(&self) -> Polyline {
        let (min, max) = self.inner();
        Polyline::new(
            vec![
                Point2::new(min.x, min.y),
                Point2::new(max.x, min.y),
                Point2::new(max.x, max.y),
                Point2::new(min.x, max.y),
            ],
            true,
        )
    }

    /// Draw the sheet border into `doc` as one undoable unit, returning its id.
    pub fn place(&self, doc: &mut Document) -> Vec<EntityId> {
        doc.transact("PLOTSETUP", |d| {
            vec![d.add_geometry(SHEET_LAYER, Geometry::Polyline(self.border()))]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_swaps_the_paper_dimensions() {
        let land = Sheet::new(PaperSize::IsoA3, Orientation::Landscape, 10.0);
        let port = Sheet::new(PaperSize::IsoA3, Orientation::Portrait, 10.0);
        assert_eq!(land.extent(), (420.0, 297.0));
        assert_eq!(port.extent(), (297.0, 420.0));
    }

    #[test]
    fn custom_size_is_stored_landscape() {
        // Supplied portrait, normalised to landscape.
        assert_eq!(PaperSize::Custom(200.0, 500.0).dimensions(), (500.0, 200.0));
    }

    #[test]
    fn border_insets_by_the_margin() {
        let sheet = Sheet::new(PaperSize::IsoA0, Orientation::Landscape, 20.0);
        let (min, max) = sheet.inner();
        assert_eq!(min, Point2::new(20.0, 20.0));
        assert_eq!(max, Point2::new(1189.0 - 20.0, 841.0 - 20.0));
        assert!(sheet.border().closed);
    }

    #[test]
    fn an_oversized_margin_cannot_invert_the_border() {
        let sheet = Sheet::new(PaperSize::IsoA4, Orientation::Landscape, 10_000.0);
        let (min, max) = sheet.inner();
        assert!(
            max.x > min.x && max.y > min.y,
            "border stays non-degenerate"
        );
    }

    #[test]
    fn place_is_one_undoable_border() {
        let mut doc = Document::new();
        let ids = Sheet::a1_landscape().place(&mut doc);
        assert_eq!(ids.len(), 1);
        assert_eq!(doc.len(), 1);
        assert_eq!(doc.undo_label(), Some("PLOTSETUP"));
        doc.undo();
        assert_eq!(doc.len(), 0);
    }

    #[test]
    fn size_names_parse_case_and_separator_insensitively() {
        assert_eq!(PaperSize::from_name("a1"), Some(PaperSize::IsoA1));
        assert_eq!(PaperSize::from_name("ANSI-D"), Some(PaperSize::AnsiD));
        assert_eq!(PaperSize::from_name("arch c"), Some(PaperSize::ArchC));
        assert_eq!(PaperSize::from_name("b7"), None);
    }
}
