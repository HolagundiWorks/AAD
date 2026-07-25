//! Curated AIA / US NCS layer catalog (~100 layers).

use aadt_model::{Layer, LineType};

use crate::aci::{aci_to_srgb, contrast_vs_white};

/// Discipline bucket for filtering and documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Discipline {
    Architectural,
    Structural,
    Mechanical,
    Electrical,
    Plumbing,
    Annotation,
    Presentation,
}

/// One standard layer definition.
#[derive(Debug, Clone, Copy)]
pub struct StandardLayer {
    pub name: &'static str,
    pub aci: u8,
    pub linetype: LineType,
    pub lineweight_mm: f32,
    pub discipline: Discipline,
}

impl StandardLayer {
    /// Materialise as a document [`Layer`].
    pub fn to_layer(self) -> Layer {
        Layer::new(self.name)
            .with_color(self.aci)
            .with_linetype(self.linetype)
            .with_lineweight_mm(self.lineweight_mm)
    }

    /// sRGB for print/screen preview.
    pub fn srgb(self) -> (u8, u8, u8) {
        aci_to_srgb(self.aci)
    }

    /// Contrast ratio vs white paper.
    pub fn contrast_vs_paper(self) -> f64 {
        let (r, g, b) = self.srgb();
        contrast_vs_white(r, g, b)
    }
}

macro_rules! L {
    ($name:literal, $aci:expr, $lt:expr, $lw:expr, $disc:expr) => {
        StandardLayer {
            name: $name,
            aci: $aci,
            linetype: $lt,
            lineweight_mm: $lw,
            discipline: $disc,
        }
    };
}

use Discipline::*;
use LineType::*;

/// Hardcoded AIA/NCS curated set. Colours and weights are WCAG-audited in tests.
pub const AIA_LAYERS: &[StandardLayer] = &[
    // --- Architectural ---
    L!("A-WALL", 250, Continuous, 0.50, Architectural),
    L!("A-WALL-PRHT", 251, Continuous, 0.35, Architectural),
    L!("A-WALL-PATT", 8, Continuous, 0.35, Architectural),
    L!("A-WALL-FIRE", 1, Continuous, 0.50, Architectural),
    L!("A-WALL-LINT", 252, Continuous, 0.35, Architectural),
    L!("A-DOOR", 30, Continuous, 0.35, Architectural),
    L!("A-DOOR-IDEN", 31, Continuous, 0.35, Architectural),
    L!("A-GLAZ", 140, Continuous, 0.35, Architectural),
    L!("A-GLAZ-IDEN", 141, Continuous, 0.35, Architectural),
    L!("A-FLOR", 80, Continuous, 0.35, Architectural),
    L!("A-FLOR-IDEN", 81, Continuous, 0.35, Architectural),
    L!("A-FLOR-PATT", 80, Continuous, 0.50, Architectural),
    L!("A-FLOR-HRAL", 80, Continuous, 0.50, Architectural),
    L!("A-CEIL", 150, Continuous, 0.35, Architectural),
    L!("A-CEIL-GRID", 150, Continuous, 0.25, Architectural),
    L!("A-ROOF", 40, Continuous, 0.35, Architectural),
    L!("A-ROOF-OTLN", 41, Continuous, 0.50, Architectural),
    L!("A-FURN", 60, Continuous, 0.35, Architectural),
    L!("A-FURN-FIXD", 61, Continuous, 0.35, Architectural),
    L!("A-EQPM", 90, Continuous, 0.35, Architectural),
    L!("A-AREA", 110, Dashed, 0.35, Architectural),
    L!("A-CLINE", 210, Center, 0.25, Architectural),
    L!("A-COLS", 20, Continuous, 0.50, Architectural),
    L!("A-CASE", 100, Continuous, 0.35, Architectural),
    L!("A-STAIR", 50, Continuous, 0.35, Architectural),
    L!("A-RAIL", 50, Continuous, 0.35, Architectural),
    L!("A-RAMP", 50, Continuous, 0.50, Architectural),
    L!("A-EVTR", 70, Continuous, 0.35, Architectural),
    L!("A-ESCL", 71, Continuous, 0.35, Architectural),
    L!("A-SITE", 120, Continuous, 0.35, Architectural),
    L!("A-SITE-WALK", 120, Continuous, 0.35, Architectural),
    L!("A-SITE-ROAD", 120, Continuous, 0.50, Architectural),
    L!("A-ANNO-NOTE", 5, Continuous, 0.25, Annotation),
    L!("A-ANNO-TEXT", 5, Continuous, 0.25, Annotation),
    L!("A-ANNO-LEGEND", 5, Continuous, 0.25, Annotation),
    L!("A-ANNO-REFR", 160, Continuous, 0.25, Annotation),
    L!("A-ANNO-SYMB", 5, Continuous, 0.50, Annotation),
    L!("A-ANNO-KEYN", 1, Continuous, 0.35, Annotation),
    L!("A-ANNO-TABL", 5, Continuous, 0.25, Annotation),
    L!("A-ANNO-IDEN", 1, Continuous, 0.35, Annotation),
    L!("A-ANNO-REVS", 1, Continuous, 0.35, Annotation),
    L!("A-ANNO-DIMS", 5, Continuous, 0.25, Annotation),
    L!("A-DIMS", 5, Continuous, 0.25, Annotation),
    L!("A-GRID", 8, Center, 0.35, Annotation),
    L!("A-GRID-IDEN", 8, Continuous, 0.35, Annotation),
    L!("A-PATT", 251, Continuous, 0.25, Architectural),
    L!("A-TTLB", 250, Continuous, 0.50, Annotation),
    L!("A-TTLB-TEXT", 250, Continuous, 0.25, Annotation),
    // Professional title-block layers (fixed layout; attributes vary per sheet).
    L!("BORDER", 250, Continuous, 0.70, Annotation),
    L!("A-SHEET", 250, Continuous, 0.70, Annotation),
    L!("TITLEBLOCK", 250, Continuous, 0.35, Annotation),
    L!("TB_TEXT", 250, Continuous, 0.25, Annotation),
    L!("TB_ATTR", 7, Continuous, 0.25, Annotation),
    // In-drawing nameplate template layers (model space only).
    L!("TMPL_BORDER", 250, Continuous, 0.70, Annotation),
    L!("TMPL_VIEW", 8, Continuous, 0.35, Annotation),
    L!("TMPL_TEXT", 7, Continuous, 0.25, Annotation),
    L!("REVISION", 1, Continuous, 0.35, Annotation),
    L!("LOGO", 250, Continuous, 0.25, Annotation),
    L!("A-SECT", 1, Continuous, 0.50, Architectural),
    L!("A-SECT-IDEN", 1, Continuous, 0.35, Architectural),
    L!("A-SECT-PATT", 251, Continuous, 0.25, Architectural),
    L!("A-ELEV", 30, Continuous, 0.35, Architectural),
    L!("A-ELEV-OTLN", 250, Continuous, 0.50, Architectural),
    L!("A-DETL", 40, Continuous, 0.35, Architectural),
    L!("A-DETL-IDEN", 41, Continuous, 0.35, Architectural),
    // --- Presentation ---
    L!("A-PRES", 250, Continuous, 0.35, Presentation),
    L!("A-PRES-MARK", 1, Continuous, 0.50, Presentation),
    L!("A-PRES-HATCH", 80, Continuous, 0.35, Presentation),
    L!("A-PRES-NOTE", 5, Continuous, 0.25, Presentation),
    L!("A-PRES-ENTOURAGE", 110, Continuous, 0.35, Presentation),
    // --- Structural ---
    L!("S-GRID", 8, Center, 0.35, Structural),
    L!("S-COLS", 1, Continuous, 0.50, Structural),
    L!("S-BEAM", 30, Continuous, 0.50, Structural),
    L!("S-JOIST", 31, Continuous, 0.35, Structural),
    L!("S-FNDN", 40, Continuous, 0.50, Structural),
    L!("S-FNDN-FTNG", 41, Continuous, 0.50, Structural),
    L!("S-SLAB", 80, Continuous, 0.35, Structural),
    L!("S-WALL", 250, Continuous, 0.50, Structural),
    L!("S-BRAC", 60, Continuous, 0.35, Structural),
    L!("S-ANNO", 5, Continuous, 0.25, Structural),
    L!("S-DIMS", 5, Continuous, 0.25, Structural),
    // --- Mechanical ---
    L!("M-HVAC-SUPP", 1, Continuous, 0.35, Mechanical),
    L!("M-HVAC-RETR", 5, Continuous, 0.35, Mechanical),
    L!("M-HVAC-EXHS", 5, Continuous, 0.50, Mechanical),
    L!("M-HVAC-CDFF", 30, Continuous, 0.35, Mechanical),
    L!("M-EQPM", 40, Continuous, 0.35, Mechanical),
    L!("M-ANNO", 5, Continuous, 0.25, Mechanical),
    L!("M-CTRL", 140, Dashed, 0.35, Mechanical),
    L!("M-PIPING", 80, Continuous, 0.35, Mechanical),
    // --- Electrical ---
    L!("E-LITE", 50, Continuous, 0.35, Electrical),
    L!("E-LITE-IDEN", 50, Continuous, 0.35, Electrical),
    L!("E-LITE-CIRC", 50, Continuous, 0.50, Electrical),
    L!("E-POWR", 1, Continuous, 0.35, Electrical),
    L!("E-POWR-CIRC", 30, Continuous, 0.35, Electrical),
    L!("E-POWR-PANL", 31, Continuous, 0.35, Electrical),
    L!("E-COMM", 140, Continuous, 0.35, Electrical),
    L!("E-FIRE", 1, Continuous, 0.35, Electrical),
    L!("E-ANNO", 5, Continuous, 0.25, Electrical),
    L!("E-EQPM", 40, Continuous, 0.35, Electrical),
    L!("E-GRND", 8, Continuous, 0.35, Electrical),
    // --- Plumbing ---
    L!("P-SANR", 80, Continuous, 0.35, Plumbing),
    L!("P-SANR-PIPE", 80, Continuous, 0.35, Plumbing),
    L!("P-DOMW", 5, Continuous, 0.35, Plumbing),
    L!("P-DOMW-CWTR", 140, Continuous, 0.35, Plumbing),
    L!("P-DOMW-HWTR", 1, Continuous, 0.35, Plumbing),
    L!("P-STRM", 110, Continuous, 0.35, Plumbing),
    L!("P-FIXT", 40, Continuous, 0.35, Plumbing),
    L!("P-EQPM", 30, Continuous, 0.35, Plumbing),
    L!("P-ANNO", 5, Continuous, 0.25, Plumbing),
    L!("P-FIRE", 1, Continuous, 0.35, Plumbing),
];

/// Look up a catalog entry by exact layer name (case-sensitive AIA form).
pub fn find_layer(name: &str) -> Option<&'static StandardLayer> {
    AIA_LAYERS.iter().find(|l| l.name == name)
}

/// Seed any missing standard layers onto `doc` (does not overwrite existing).
/// Returns how many layers were added.
pub fn apply_standard_layers(doc: &mut aadt_model::Document) -> usize {
    let mut added = 0;
    for std in AIA_LAYERS {
        if doc.layer(std.name).is_none() {
            doc.add_layer(std.to_layer());
            added += 1;
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aci::{min_lineweight_mm_for_contrast, GRAPHICS_AA_MIN};

    #[test]
    fn catalog_size_is_in_band() {
        let n = AIA_LAYERS.len();
        assert!(n >= 80, "expected ≥80 layers, got {n}");
        assert!(n <= 120, "expected ≤120 layers, got {n}");
    }

    #[test]
    fn names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for l in AIA_LAYERS {
            assert!(seen.insert(l.name), "duplicate layer {}", l.name);
        }
    }

    #[test]
    fn every_layer_meets_wcag_graphics_aa_vs_white() {
        let mut failures = Vec::new();
        for l in AIA_LAYERS {
            let c = l.contrast_vs_paper();
            let floor = min_lineweight_mm_for_contrast(c);
            if c < GRAPHICS_AA_MIN {
                failures.push(format!(
                    "{} ACI {} contrast {c:.2} < {GRAPHICS_AA_MIN}",
                    l.name, l.aci
                ));
            }
            if l.lineweight_mm + 1e-4 < floor {
                failures.push(format!(
                    "{} lineweight {:.2} mm < floor {:.2} mm (contrast {c:.2})",
                    l.name, l.lineweight_mm, floor
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "WCAG / lineweight regressions:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn apply_seeds_missing_only() {
        let mut doc = aadt_model::Document::new();
        doc.add_layer(Layer::new("A-WALL").with_color(3));
        let n = apply_standard_layers(&mut doc);
        assert!(n >= 80);
        assert_eq!(doc.layer("A-WALL").unwrap().color, 3, "existing preserved");
        assert!(doc.layer("E-LITE").is_some());
    }
}
