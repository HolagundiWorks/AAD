//! # aadt-standards — HCW discipline layer standard, metadata, layer sets
//!
//! Canonical built-in catalog of ~320 discipline-based drawing layers (ISO 13567
//! / AIA derived) with ACI colours and ISO plot lineweights. Each layer carries
//! a semantic [`ElementType`] that yields IFC class, Uniformat / MasterFormat
//! codes, and AI tags. Named [`LayerSet`] presets drive per-discipline
//! visibility. See `docs/standards/hcw-layer-standard.md`.

mod aci;
mod catalog;
mod report;
mod section;
mod sets;
mod space_map;

pub use aci::{
    aci_to_srgb, contrast_ratio, contrast_vs_white, min_lineweight_mm_for_contrast,
    relative_luminance, GRAPHICS_AA_MIN,
};
pub use catalog::{
    apply_standard_layers, find_layer, Discipline, ElementType, Phase, StandardLayer, AIA_LAYERS,
    STANDARD_LAYERS,
};
pub use report::{catalog_csv, catalog_markdown};
pub use section::generate_section;
pub use sets::{apply_layer_set, LayerSet, PRESETS};
pub use space_map::{category_default_layer, category_from_layer};
