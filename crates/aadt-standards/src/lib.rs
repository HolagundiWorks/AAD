//! # aadt-standards — AIA / US NCS layers, WCAG audit, layer sets
//!
//! Hardcoded catalog of architectural drawing layers with ACI colours and ISO
//! plot lineweights checked for WCAG 2.1 AA graphical contrast against white
//! paper. Named [`LayerSet`] presets drive construction / services / electrical /
//! presentation visibility.

mod aci;
mod catalog;
mod section;
mod sets;

pub use aci::{
    aci_to_srgb, contrast_ratio, contrast_vs_white, min_lineweight_mm_for_contrast,
    relative_luminance, GRAPHICS_AA_MIN,
};
pub use catalog::{
    apply_standard_layers, find_layer, Discipline, StandardLayer, AIA_LAYERS,
};
pub use section::generate_section;
pub use sets::{apply_layer_set, LayerSet, PRESETS};
