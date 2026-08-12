//! Sheet-bound view presets (L6) — layer mute rules without mutating the live model.
//!
//! Interactive GUI can still apply presets to the session document; plot / present
//! should call [`document_with_view_preset`] so Technical vs Presentation sheets
//! do not thrash authoring visibility.

use aadt_model::{Document, Layer};

/// Viewport / sheet presentation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewPreset {
    #[default]
    TechnicalPlan,
    Presentation,
}

impl ViewPreset {
    pub fn label(self) -> &'static str {
        match self {
            Self::TechnicalPlan => "Technical",
            Self::Presentation => "Presentation",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_uppercase().as_str() {
            "TECHNICAL" | "TECH" | "T" | "TECHNICALPLAN" => Some(Self::TechnicalPlan),
            "PRESENTATION" | "PRES" | "P" => Some(Self::Presentation),
            _ => None,
        }
    }
}

fn ensure_visible(doc: &mut Document, name: &str, visible: bool) {
    if doc.layer(name).is_none() {
        doc.add_layer(Layer::new(name));
    }
    doc.set_layer_visible(name, visible);
}

/// Apply Technical / Presentation layer visibility on `doc` (in place).
pub fn apply_view_preset(doc: &mut Document, preset: ViewPreset) {
    match preset {
        ViewPreset::TechnicalPlan => {
            for name in ["A-HATCH-TECH", "A-FLOOR-FIN", "A-WALL", "A-DOOR", "A-GLAZ"] {
                ensure_visible(doc, name, true);
            }
            for name in ["A-ANNO-PRES", "A-PRES", "A-PRES-HATCH", "A-FURN-PRES"] {
                ensure_visible(doc, name, false);
            }
        }
        ViewPreset::Presentation => {
            for name in [
                "A-ANNO-PRES",
                "A-PRES",
                "A-PRES-HATCH",
                "A-FURN-PRES",
                "A-FLOOR-FIN",
            ] {
                ensure_visible(doc, name, true);
            }
            ensure_visible(doc, "A-HATCH-TECH", false);
        }
    }
}

/// Clone `model` and apply `preset` so compose/plot never mutates the live drawing.
pub fn document_with_view_preset(model: &Document, preset: ViewPreset) -> Document {
    let mut cloned = model.clone();
    apply_view_preset(&mut cloned, preset);
    cloned
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Point2, Segment};
    use aadt_model::Geometry;

    #[test]
    fn clone_preset_hides_presentation_layers_without_touching_source() {
        let mut src = Document::new();
        src.add_layer(Layer::new("A-PRES"));
        src.set_layer_visible("A-PRES", true);
        src.add_geometry(
            "A-PRES",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        src.add_layer(Layer::new("A-HATCH-TECH"));
        src.set_layer_visible("A-HATCH-TECH", true);

        let tech = document_with_view_preset(&src, ViewPreset::TechnicalPlan);
        assert!(!tech.layer_shown("A-PRES"));
        assert!(tech.layer_shown("A-HATCH-TECH"));
        // Source unchanged.
        assert!(src.layer_shown("A-PRES"));
        assert!(src.layer_shown("A-HATCH-TECH"));

        let pres = document_with_view_preset(&src, ViewPreset::Presentation);
        assert!(pres.layer_shown("A-PRES"));
        assert!(!pres.layer_shown("A-HATCH-TECH"));
        assert!(src.layer_shown("A-HATCH-TECH"));
    }
}
