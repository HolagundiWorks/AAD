//! Named layer-set visibility presets (construction / services / electrical / presentation).

use aadt_model::Document;

use crate::catalog::{apply_standard_layers, Discipline, AIA_LAYERS};

/// A named visibility preset over the AIA catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerSet {
    pub name: &'static str,
    /// Disciplines drawn at full strength.
    pub primary: &'static [Discipline],
    /// Disciplines kept as a thin underlay (still visible).
    pub underlay: &'static [Discipline],
}

impl LayerSet {
    pub fn names() -> impl Iterator<Item = &'static str> {
        PRESETS.iter().map(|s| s.name)
    }

    pub fn find(name: &str) -> Option<&'static LayerSet> {
        let u = name.trim().to_ascii_uppercase();
        PRESETS.iter().find(|s| s.name == u)
    }

    fn shows(&self, d: Discipline) -> bool {
        self.primary.contains(&d) || self.underlay.contains(&d)
    }
}

use Discipline::*;

/// Built-in presets from the product plan.
pub const PRESETS: &[LayerSet] = &[
    LayerSet {
        name: "CONSTRUCTION",
        primary: &[Architectural, Structural, Annotation],
        underlay: &[],
    },
    LayerSet {
        name: "SERVICES",
        primary: &[Mechanical, Plumbing, Annotation],
        underlay: &[Architectural, Structural],
    },
    LayerSet {
        name: "ELECTRICAL",
        primary: &[Electrical, Annotation],
        underlay: &[Architectural],
    },
    LayerSet {
        name: "PRESENTATION",
        primary: &[Presentation, Architectural],
        underlay: &[],
    },
];

/// Seed standard layers (if missing) then apply visibility for `set_name`.
///
/// Presentation hides annotation clutter (dims/grids) even when Architectural is on.
/// Returns an error if the set name is unknown.
pub fn apply_layer_set(doc: &mut Document, set_name: &str) -> Result<String, String> {
    let set = LayerSet::find(set_name)
        .ok_or_else(|| format!("unknown layer set {set_name:?}; try CONSTRUCTION, SERVICES, ELECTRICAL, PRESENTATION"))?;
    apply_standard_layers(doc);

    let mut on = 0usize;
    let mut off = 0usize;
    for std in AIA_LAYERS {
        let mut visible = set.shows(std.discipline);
        if set.name == "PRESENTATION" {
            // Hide construction annotation clutter on presentation sheets.
            if matches!(
                std.name,
                "A-DIMS" | "A-GRID" | "A-GRID-IDEN" | "A-ANNO-REFR" | "S-DIMS" | "S-GRID"
            ) {
                visible = false;
            }
            // Keep presentation + walls/doors/glazing/floors; drop MEP noise.
            if matches!(std.discipline, Mechanical | Electrical | Plumbing | Structural) {
                visible = false;
            }
        }
        if set.name == "CONSTRUCTION" {
            if matches!(std.discipline, Presentation | Mechanical | Electrical | Plumbing) {
                visible = false;
            }
        }
        doc.set_layer_visible(std.name, visible);
        if visible {
            on += 1;
        } else {
            off += 1;
        }
    }
    Ok(format!(
        "layer set {} applied ({on} on, {off} off)",
        set.name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_hides_mep() {
        let mut doc = Document::new();
        apply_layer_set(&mut doc, "CONSTRUCTION").unwrap();
        assert!(doc.layer("A-WALL").unwrap().visible);
        assert!(!doc.layer("E-LITE").unwrap().visible);
        assert!(!doc.layer("M-HVAC-SUPP").unwrap().visible);
    }

    #[test]
    fn electrical_keeps_arch_underlay() {
        let mut doc = Document::new();
        apply_layer_set(&mut doc, "ELECTRICAL").unwrap();
        assert!(doc.layer("E-LITE").unwrap().visible);
        assert!(doc.layer("A-WALL").unwrap().visible);
        assert!(!doc.layer("M-HVAC-SUPP").unwrap().visible);
    }

    #[test]
    fn presentation_shows_pres_hides_dims() {
        let mut doc = Document::new();
        apply_layer_set(&mut doc, "PRESENTATION").unwrap();
        assert!(doc.layer("A-PRES-MARK").unwrap().visible);
        assert!(doc.layer("A-WALL").unwrap().visible);
        assert!(!doc.layer("A-DIMS").unwrap().visible);
        assert!(!doc.layer("E-POWR").unwrap().visible);
    }
}
