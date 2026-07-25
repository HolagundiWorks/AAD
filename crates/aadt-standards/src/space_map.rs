//! Map between model-space categories and AIA layer shims.

use aadt_model::SpaceCategory;

/// Default export / drafting shim layer for a category model space.
pub fn category_default_layer(cat: SpaceCategory) -> &'static str {
    cat.default_layer()
}

/// Heuristic used on CAD import to place geometry into a Ground category space.
pub fn category_from_layer(layer: &str) -> SpaceCategory {
    SpaceCategory::from_layer_name(layer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shim_layers_match_catalog_names() {
        assert_eq!(category_default_layer(SpaceCategory::Walls), "A-WALL");
        assert_eq!(category_default_layer(SpaceCategory::Lintel), "A-WALL-LINT");
        assert_eq!(category_default_layer(SpaceCategory::BeamLayout), "S-BEAM");
        assert_eq!(category_from_layer("A-FURN-FIXD"), SpaceCategory::Furniture);
        assert_eq!(category_from_layer("S-BEAM"), SpaceCategory::BeamLayout);
    }
}
