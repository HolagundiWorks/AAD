//! Floors (stories) and per-floor category model spaces.
//!
//! Each floor owns one model space per [`SpaceCategory`]. The active model space
//! is the drafting target; layers are only an export/compat shim derived from the
//! category. See ADR-0009.

use std::collections::BTreeMap;
use std::fmt;

/// Stable floor (storey) id within a [`super::Document`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct LevelId(pub u64);

/// Stable model-space id within a [`super::Document`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ModelSpaceId(pub u64);

/// Element category that a model space represents (not a layer filter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SpaceCategory {
    Walls,
    Structural,
    Lintel,
    BeamLayout,
    Furniture,
    /// User-added space on a floor (not auto-created with floors).
    Custom,
}

impl SpaceCategory {
    /// Built-in categories created with every floor (excludes [`Self::Custom`]).
    pub const ALL: [SpaceCategory; 5] = [
        SpaceCategory::Walls,
        SpaceCategory::Structural,
        SpaceCategory::Lintel,
        SpaceCategory::BeamLayout,
        SpaceCategory::Furniture,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SpaceCategory::Walls => "Walls",
            SpaceCategory::Structural => "Structural",
            SpaceCategory::Lintel => "Lintel",
            SpaceCategory::BeamLayout => "BeamLayout",
            SpaceCategory::Furniture => "Furniture",
            SpaceCategory::Custom => "Custom",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "walls" | "wall" => Some(Self::Walls),
            "structural" | "structure" | "struct" => Some(Self::Structural),
            "lintel" | "lintels" => Some(Self::Lintel),
            "beamlayout" | "beam" | "beams" | "beam_layout" => Some(Self::BeamLayout),
            "furniture" | "furn" => Some(Self::Furniture),
            "custom" | "other" | "user" => Some(Self::Custom),
            _ => None,
        }
    }

    /// Default AIA layer name used when exporting / as a compat shim.
    pub fn default_layer(self) -> &'static str {
        match self {
            SpaceCategory::Walls => "A-WALL",
            SpaceCategory::Structural => "S-COLS",
            SpaceCategory::Lintel => "A-WALL-LINT",
            SpaceCategory::BeamLayout => "S-BEAM",
            SpaceCategory::Furniture => "A-FURN",
            SpaceCategory::Custom => "A-ANNO",
        }
    }

    /// Heuristic: map an imported AIA (or firm) layer name to a category.
    pub fn from_layer_name(layer: &str) -> Self {
        let u = layer.trim().to_ascii_uppercase();
        if u.contains("FURN") {
            return Self::Furniture;
        }
        if u.contains("BEAM") {
            return Self::BeamLayout;
        }
        if u.contains("LINT") {
            return Self::Lintel;
        }
        if u.starts_with("S-") || u.contains("COLS") || u.contains("COLUMN") {
            return Self::Structural;
        }
        if u.contains("WALL") {
            return Self::Walls;
        }
        Self::Walls
    }
}

impl fmt::Display for SpaceCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One building storey. All floors share the document's XY reference origin.
#[derive(Debug, Clone, PartialEq)]
pub struct Floor {
    pub id: LevelId,
    pub name: String,
    /// Order from bottom to top (0 = lowest).
    pub index: u32,
    /// Elevation in metres (for ordering / future section; XY is shared).
    pub elevation_m: f64,
}

/// A drafting model space: one category on one floor.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSpace {
    pub id: ModelSpaceId,
    pub floor: LevelId,
    pub category: SpaceCategory,
    /// Display name, e.g. `Ground · Walls`.
    pub name: String,
}

impl ModelSpace {
    pub fn display_name(floor_name: &str, category: SpaceCategory) -> String {
        format!("{floor_name} · {category}")
    }
}

/// Bootstrap payload returned by [`super::Document::ensure_default_spaces`].
#[derive(Debug, Clone, Copy)]
pub struct DefaultSpaces {
    pub ground: LevelId,
    pub walls: ModelSpaceId,
}

/// Ordered floor list helpers.
pub fn floors_sorted(floors: &BTreeMap<LevelId, Floor>) -> Vec<&Floor> {
    let mut v: Vec<&Floor> = floors.values().collect();
    v.sort_by(|a, b| a.index.cmp(&b.index).then(a.id.0.cmp(&b.id.0)));
    v
}
