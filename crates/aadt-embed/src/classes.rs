//! The curated target-class taxonomy the local model predicts.
//!
//! Each class carries the **predefined metadata** it maps to: an AIA standard
//! layer name (from the `aadt-standards` catalog) and, when unambiguous, a
//! model-space [`SpaceCategory`]. Classes whose natural category is unclear
//! (stairs, annotation) map to a layer only (`category == None`) and leave the
//! element's model space untouched — the reconciliation step handles the rest.

use aadt_model::SpaceCategory;

/// A geometry class the embedder/classifier can predict. Kept deliberately small
/// and 2D-distinguishable; low-confidence elements fall through to reconciliation
/// rather than being forced into a class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementClass {
    Wall,
    Column,
    Beam,
    Lintel,
    Door,
    Glazing,
    Furniture,
    Equipment,
    Stair,
    Annotation,
}

impl ElementClass {
    /// All classes, in stable index order (the model's output order).
    pub const ALL: [ElementClass; 10] = [
        ElementClass::Wall,
        ElementClass::Column,
        ElementClass::Beam,
        ElementClass::Lintel,
        ElementClass::Door,
        ElementClass::Glazing,
        ElementClass::Furniture,
        ElementClass::Equipment,
        ElementClass::Stair,
        ElementClass::Annotation,
    ];

    /// Stable class index (also the softmax output index).
    pub fn index(self) -> usize {
        match self {
            ElementClass::Wall => 0,
            ElementClass::Column => 1,
            ElementClass::Beam => 2,
            ElementClass::Lintel => 3,
            ElementClass::Door => 4,
            ElementClass::Glazing => 5,
            ElementClass::Furniture => 6,
            ElementClass::Equipment => 7,
            ElementClass::Stair => 8,
            ElementClass::Annotation => 9,
        }
    }

    /// Class from its stable index (`None` out of range).
    pub fn from_index(i: usize) -> Option<ElementClass> {
        ElementClass::ALL.get(i).copied()
    }

    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            ElementClass::Wall => "Wall",
            ElementClass::Column => "Column",
            ElementClass::Beam => "Beam",
            ElementClass::Lintel => "Lintel",
            ElementClass::Door => "Door",
            ElementClass::Glazing => "Glazing",
            ElementClass::Furniture => "Furniture",
            ElementClass::Equipment => "Equipment",
            ElementClass::Stair => "Stair",
            ElementClass::Annotation => "Annotation",
        }
    }

    /// The AIA standard layer this class maps to (must exist in the catalog).
    pub fn layer(self) -> &'static str {
        match self {
            ElementClass::Wall => "A-WALL",
            ElementClass::Column => "A-COLS",
            ElementClass::Beam => "S-BEAM",
            ElementClass::Lintel => "A-WALL-LINT",
            ElementClass::Door => "A-DOOR",
            ElementClass::Glazing => "A-GLAZ",
            ElementClass::Furniture => "A-FURN",
            ElementClass::Equipment => "A-EQPM",
            ElementClass::Stair => "A-STAIR",
            ElementClass::Annotation => "A-ANNO-TEXT",
        }
    }

    /// The model-space category to file this class under, or `None` to assign the
    /// layer only and leave the element's current model space unchanged.
    pub fn category(self) -> Option<SpaceCategory> {
        match self {
            ElementClass::Wall => Some(SpaceCategory::Walls),
            ElementClass::Column => Some(SpaceCategory::Structural),
            ElementClass::Beam => Some(SpaceCategory::BeamLayout),
            ElementClass::Lintel => Some(SpaceCategory::Lintel),
            ElementClass::Door => Some(SpaceCategory::Walls),
            ElementClass::Glazing => Some(SpaceCategory::Walls),
            ElementClass::Furniture => Some(SpaceCategory::Furniture),
            ElementClass::Equipment => Some(SpaceCategory::Furniture),
            ElementClass::Stair => None,
            ElementClass::Annotation => None,
        }
    }
}
