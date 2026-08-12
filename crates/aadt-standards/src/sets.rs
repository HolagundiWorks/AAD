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

/// Built-in discipline visibility presets over the HCW catalog.
pub const PRESETS: &[LayerSet] = &[
    LayerSet {
        name: "CONSTRUCTION",
        primary: &[Architectural, Structural, Annotation, General],
        underlay: &[Existing],
    },
    LayerSet {
        name: "STRUCTURAL",
        primary: &[Structural, Annotation, General],
        underlay: &[Architectural],
    },
    LayerSet {
        name: "CIVIL",
        primary: &[Civil, Survey, Annotation, General],
        underlay: &[Architectural],
    },
    LayerSet {
        name: "SERVICES",
        primary: &[Mechanical, Plumbing, Annotation, General],
        underlay: &[Architectural, Structural],
    },
    LayerSet {
        name: "ELECTRICAL",
        primary: &[Electrical, Telecom, Annotation, General],
        underlay: &[Architectural],
    },
    LayerSet {
        name: "PLUMBING",
        primary: &[Plumbing, Annotation, General],
        underlay: &[Architectural],
    },
    LayerSet {
        name: "FIRE",
        primary: &[FireFighting, Annotation, General],
        underlay: &[Architectural],
    },
    LayerSet {
        name: "ELV",
        primary: &[Telecom, Annotation, General],
        underlay: &[Architectural],
    },
    LayerSet {
        name: "LANDSCAPE",
        primary: &[Landscape, Civil, Annotation, General],
        underlay: &[Architectural],
    },
    LayerSet {
        name: "INTERIOR",
        primary: &[Interior, Architectural, Annotation, General],
        underlay: &[],
    },
    LayerSet {
        name: "DEMOLITION",
        primary: &[Demolition, Existing, Annotation, General],
        underlay: &[Architectural],
    },
    LayerSet {
        name: "PRESENTATION",
        primary: &[Presentation, Architectural, Landscape],
        underlay: &[],
    },
    // CAD import layers retagged as `ORG-DWG_LAYER-*` (see
    // `aadt_model::Document::retag_imported_layers`).
    LayerSet {
        name: "IMPORT",
        primary: &[],
        underlay: &[],
    },
];

/// Seed standard layers (if missing) then apply visibility for `set_name`.
///
/// Presentation hides annotation clutter (dims/grids) even when Architectural is on.
/// Returns an error if the set name is unknown.
pub fn apply_layer_set(doc: &mut Document, set_name: &str) -> Result<String, String> {
    let set = LayerSet::find(set_name).ok_or_else(|| {
        format!(
            "unknown layer set {set_name:?}; try CONSTRUCTION, STRUCTURAL, CIVIL, SERVICES, \
             ELECTRICAL, PLUMBING, FIRE, ELV, LANDSCAPE, INTERIOR, DEMOLITION, PRESENTATION, IMPORT"
        )
    })?;
    apply_standard_layers(doc);

    if set.name == "IMPORT" {
        // OPEN adopts recognized ORG-DWG_LAYER-* names onto AIA shims (A-DOOR…).
        // Turning *all* AIA off would hide those inserts/blocks. Keep every layer
        // that currently hosts geometry (plus remaining ORG-* names) visible.
        let mut used: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for e in doc.iter_entities() {
            used.insert(e.layer.clone());
        }
        for h in doc.hatches() {
            used.insert(h.layer.clone());
        }
        for d in doc.dimensions() {
            used.insert(d.layer.clone());
        }

        let mut on = 0usize;
        let mut off = 0usize;
        for std in AIA_LAYERS {
            let keep = used.contains(std.name);
            doc.set_layer_visible(std.name, keep);
            if keep {
                on += 1;
            } else {
                off += 1;
            }
        }
        let names: Vec<String> = doc.layers().map(|l| l.name.clone()).collect();
        for name in names {
            if name.starts_with(aadt_model::IMPORTED_LAYER_PREFIX) || used.contains(&name) {
                doc.set_layer_visible(&name, true);
                on += 1;
            }
        }
        return Ok(format!(
            "layer set IMPORT applied ({on} layers on with content/import, {off} unused AIA off)"
        ));
    }

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
            if matches!(
                std.discipline,
                Mechanical | Electrical | Plumbing | Structural
            ) {
                visible = false;
            }
        }
        if set.name == "CONSTRUCTION"
            && matches!(
                std.discipline,
                Presentation | Mechanical | Electrical | Plumbing
            )
        {
            visible = false;
        }
        doc.set_layer_visible(std.name, visible);
        if visible {
            on += 1;
        } else {
            off += 1;
        }
    }
    // Keep imported ORG-DWG_LAYER-* layers visible as underlay for non-IMPORT sets.
    let names: Vec<String> = doc.layers().map(|l| l.name.clone()).collect();
    for name in names {
        if name.starts_with(aadt_model::IMPORTED_LAYER_PREFIX) {
            doc.set_layer_visible(&name, true);
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

    #[test]
    fn import_set_shows_org_layers_hides_unused_aia() {
        let mut doc = Document::new();
        doc.add_layer(aadt_model::Layer::new("ORG-DWG_LAYER-WALLS"));
        apply_layer_set(&mut doc, "IMPORT").unwrap();
        assert!(doc.layer("ORG-DWG_LAYER-WALLS").unwrap().visible);
        assert!(!doc.layer("A-WALL").unwrap().visible);
        assert!(LayerSet::names().any(|n| n == "IMPORT"));
    }

    #[test]
    fn import_set_keeps_adopted_aia_layers_with_geometry() {
        use aadt_geometry::Point2;
        use aadt_model::{Geometry, Insert};
        let mut doc = Document::new();
        apply_standard_layers(&mut doc);
        // Simulate post-ADOPT: INSERT on A-DOOR (standard name, no ORG- prefix).
        doc.add_geometry(
            "A-DOOR",
            Geometry::Insert(Insert::new("DOOR-1", Point2::new(0.0, 0.0), 0.0)),
        );
        apply_layer_set(&mut doc, "IMPORT").unwrap();
        assert!(
            doc.layer("A-DOOR").unwrap().visible,
            "adopted insert layer must stay on so blocks render"
        );
        assert!(
            !doc.layer("E-LITE").unwrap().visible,
            "unused AIA layers still hide"
        );
    }
}
