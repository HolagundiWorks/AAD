//! CSI MasterFormat cost codes and the rules that map an object to one.
//!
//! Revives the ADR-0005 taxonomy against the Object DB. The mapping is a
//! deterministic rules layer — object kind first, then layer-name refinement —
//! so the same object always lands on the same code, and a firm can extend the
//! table without touching the quantity engine.

use aadt_object::{EngObject, ObjectKind};

/// One MasterFormat code and its title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CostCode {
    /// MasterFormat number, e.g. `"04 20 00"`.
    pub code: &'static str,
    /// Section title, e.g. `"Unit Masonry"`.
    pub title: &'static str,
}

impl std::fmt::Display for CostCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.code, self.title)
    }
}

// ── The bundled taxonomy ──────────────────────────────────────────────────────

pub const UNIT_MASONRY: CostCode = CostCode {
    code: "04 20 00",
    title: "Unit Masonry",
};
pub const GYPSUM_BOARD: CostCode = CostCode {
    code: "09 21 16",
    title: "Gypsum Board Assemblies",
};
pub const PLASTER: CostCode = CostCode {
    code: "09 24 00",
    title: "Cement Plastering",
};
pub const PAINTING: CostCode = CostCode {
    code: "09 91 00",
    title: "Painting",
};
pub const FLOORING: CostCode = CostCode {
    code: "09 60 00",
    title: "Flooring",
};
pub const DOORS: CostCode = CostCode {
    code: "08 11 00",
    title: "Metal Doors and Frames",
};
pub const WINDOWS: CostCode = CostCode {
    code: "08 50 00",
    title: "Windows",
};
pub const CONCRETE: CostCode = CostCode {
    code: "03 30 00",
    title: "Cast-in-Place Concrete",
};
pub const STRUCTURAL_STEEL: CostCode = CostCode {
    code: "05 12 00",
    title: "Structural Steel Framing",
};
pub const FURNISHINGS: CostCode = CostCode {
    code: "12 00 00",
    title: "Furnishings",
};
pub const UNCLASSIFIED: CostCode = CostCode {
    code: "00 00 00",
    title: "Unclassified",
};

/// Every bundled code, in ascending code order.
pub const ALL: [CostCode; 11] = [
    UNCLASSIFIED,
    CONCRETE,
    UNIT_MASONRY,
    STRUCTURAL_STEEL,
    DOORS,
    WINDOWS,
    GYPSUM_BOARD,
    PLASTER,
    FLOORING,
    PAINTING,
    FURNISHINGS,
];

/// Map an object to its cost code.
///
/// Object kind decides the family; the layer name refines it where the same kind
/// covers materially different work — a drywall partition and a brick wall are
/// both `Wall` objects but different trades, and pricing them together would be
/// wrong.
pub fn code_for(object: &EngObject) -> CostCode {
    let layer = object.prop("layer").unwrap_or("").to_ascii_uppercase();
    match object.kind {
        ObjectKind::Wall => {
            if layer.contains("GYP") || layer.contains("DRY") || layer.contains("PART") {
                GYPSUM_BOARD
            } else {
                UNIT_MASONRY
            }
        }
        ObjectKind::Room => FLOORING,
        ObjectKind::Door => DOORS,
        ObjectKind::Window => WINDOWS,
        ObjectKind::Column => CONCRETE,
        ObjectKind::Beam => {
            // An `S-` (structural) steel layer reads as steel framing; otherwise
            // assume the cast-in-place concrete that dominates residential work.
            if layer.contains("STL") || layer.contains("STEEL") {
                STRUCTURAL_STEEL
            } else {
                CONCRETE
            }
        }
        ObjectKind::Furniture => FURNISHINGS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn obj(kind: ObjectKind, layer: &str) -> EngObject {
        let mut props = BTreeMap::new();
        props.insert("layer".to_string(), layer.to_string());
        EngObject {
            id: aadt_object::ObjectId(1),
            kind,
            entities: vec![],
            props,
            confidence: 1.0,
            revision: 0,
        }
    }

    #[test]
    fn masonry_and_drywall_walls_price_separately() {
        assert_eq!(code_for(&obj(ObjectKind::Wall, "A-WALL")), UNIT_MASONRY);
        assert_eq!(
            code_for(&obj(ObjectKind::Wall, "A-WALL-GYPS")),
            GYPSUM_BOARD
        );
        assert_eq!(
            code_for(&obj(ObjectKind::Wall, "A-WALL-PART")),
            GYPSUM_BOARD
        );
    }

    #[test]
    fn steel_and_concrete_beams_price_separately() {
        assert_eq!(code_for(&obj(ObjectKind::Beam, "S-BEAM")), CONCRETE);
        assert_eq!(
            code_for(&obj(ObjectKind::Beam, "S-BEAM-STEEL")),
            STRUCTURAL_STEEL
        );
    }

    #[test]
    fn every_kind_maps_somewhere() {
        for kind in ObjectKind::ALL {
            let c = code_for(&obj(kind, "A-GENERIC"));
            assert!(
                ALL.contains(&c),
                "{kind:?} mapped to a code outside the taxonomy: {c}"
            );
        }
    }

    #[test]
    fn mapping_is_case_insensitive() {
        assert_eq!(
            code_for(&obj(ObjectKind::Wall, "a-wall-gyps")),
            GYPSUM_BOARD
        );
    }
}
