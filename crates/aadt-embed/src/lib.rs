//! # aadt-embed — local, offline geometric ML classifier/embedder
//!
//! Maps a drawing element to AADT's **predefined metadata** — an AIA standard
//! layer and a model-space [`SpaceCategory`] — from the element's geometry alone,
//! using a small locally-trained neural network (see [`model`]). Everything runs
//! offline and deterministically at inference; there is no network, no background
//! compute, and no external dependency.
//!
//! ```
//! use aadt_embed::{default_classifier, Classifier};
//! use aadt_model::{Document, Entity, EntityId, Geometry, ModelSpaceId};
//! use aadt_geometry::{Circle, Point2};
//! use std::collections::HashMap;
//!
//! let doc = Document::new();
//! let column = Entity {
//!     id: EntityId(1),
//!     layer: "0".into(),
//!     space: ModelSpaceId(0),
//!     geometry: Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 0.2)),
//!     attributes: HashMap::new(),
//! };
//! let pred = default_classifier().classify(&column, &doc);
//! assert_eq!(pred.embedding.len(), aadt_embed::EMBED_DIM);
//! ```
//!
//! **Product note:** this crate deliberately reintroduces a *local* ML model, a
//! considered deviation from the M4 "AI removed" lock (see
//! `docs/adr/0010-embeddings-metadata-mapping.md`). It stays local, offline, and
//! deterministic; predictions are advisory and gated by confidence upstream.

mod classes;
pub mod drawing;
mod features;
mod model;
mod rng;
pub mod text;

use aadt_model::{Document, Entity, SpaceCategory};

pub use classes::ElementClass;
pub use drawing::{drawing_embedding, object_embedding};
pub use features::{element_features, features_for_geometry, FEATURE_DIM};
pub use model::{
    default_model, default_prototypes, ClassPrototype, Model, EMBED_DIM, HIDDEN, MODEL_ID,
    NUM_CLASSES,
};
pub use text::{entity_text_label, text_embedding, TEXT_DIM};

/// Per-class prototype (centroid) embeddings paired with their class — the
/// labeled reference vectors a vector database can seed itself with.
pub fn class_prototypes() -> Vec<(ElementClass, Vec<f32>)> {
    default_prototypes()
        .iter()
        .filter_map(|p| ElementClass::from_index(p.class).map(|c| (c, p.vector.clone())))
        .collect()
}

/// A metadata prediction for one element.
#[derive(Debug, Clone)]
pub struct Prediction {
    /// The predicted geometry class.
    pub class: ElementClass,
    /// The AIA standard layer to assign.
    pub layer: &'static str,
    /// The model-space category to file under, or `None` for layer-only.
    pub category: Option<SpaceCategory>,
    /// Softmax confidence of the winning class, in `[0, 1]`.
    pub confidence: f32,
    /// The L2-normalized embedding (length [`EMBED_DIM`]).
    pub embedding: Vec<f32>,
    /// All classes ranked by probability (descending) — reconciliation candidates.
    pub candidates: Vec<(ElementClass, f32)>,
}

/// Something that can embed and classify drawing elements.
pub trait Classifier {
    /// Full prediction (class, layer, category, confidence, embedding).
    fn classify(&self, entity: &Entity, doc: &Document) -> Prediction;

    /// Just the embedding vector for an element.
    fn embed(&self, entity: &Entity, doc: &Document) -> Vec<f32> {
        self.classify(entity, doc).embedding
    }

    /// Identifier of the underlying model (persisted to detect staleness).
    fn model_id(&self) -> &str;
}

/// The default classifier backed by the locally-trained [`Model`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ModelClassifier;

impl Classifier for ModelClassifier {
    fn classify(&self, entity: &Entity, doc: &Document) -> Prediction {
        let feats = element_features(entity, doc);
        let inf = default_model().infer(&feats);
        let class = ElementClass::from_index(inf.class).unwrap_or(ElementClass::Wall);
        let mut candidates: Vec<(ElementClass, f32)> = ElementClass::ALL
            .iter()
            .map(|c| (*c, inf.probs[c.index()]))
            .collect();
        candidates.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.index().cmp(&b.0.index()))
        });
        Prediction {
            class,
            layer: class.layer(),
            category: class.category(),
            confidence: inf.confidence,
            embedding: inf.embedding,
            candidates,
        }
    }

    fn model_id(&self) -> &str {
        MODEL_ID
    }
}

/// A shared default [`ModelClassifier`].
pub fn default_classifier() -> ModelClassifier {
    ModelClassifier
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Arc, Circle, Point2, Polyline, Segment};
    use aadt_model::{EntityId, Geometry, ModelSpaceId, Text};
    use std::collections::HashMap;

    fn entity(g: Geometry) -> Entity {
        Entity {
            id: EntityId(1),
            layer: "0".into(),
            space: ModelSpaceId(0),
            geometry: g,
            attributes: HashMap::new(),
        }
    }

    fn classify(g: Geometry) -> Prediction {
        default_classifier().classify(&entity(g), &Document::new())
    }

    #[test]
    fn embedding_is_unit_length_and_correct_dim() {
        let p = classify(Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 0.25)));
        assert_eq!(p.embedding.len(), EMBED_DIM);
        let norm = p.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-4,
            "embedding should be L2-normalized"
        );
    }

    #[test]
    fn same_element_is_deterministic() {
        let g = Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(5.0, 0.0)));
        let a = classify(g.clone());
        let b = classify(g);
        assert_eq!(a.class, b.class);
        assert_eq!(a.embedding, b.embedding);
        assert!((a.confidence - b.confidence).abs() < 1e-6);
    }

    #[test]
    fn translation_invariant_embedding() {
        // A shape and its shifted copy embed identically (features are normalized).
        let base = Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 0.3));
        let moved = Geometry::Circle(Circle::new(Point2::new(40.0, -12.0), 0.3));
        let a = classify(base).embedding;
        let b = classify(moved).embedding;
        let cos: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        assert!(
            cos > 0.999,
            "translated copy should embed the same, cos={cos}"
        );
    }

    #[test]
    fn distinct_shapes_get_distinct_classes() {
        // Clearly separable archetypes should not all collapse to one class.
        let wall = classify(Geometry::Line(Segment::new(
            Point2::new(0.0, 0.0),
            Point2::new(6.0, 0.0),
        )))
        .class;
        let column = classify(Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 0.2))).class;
        let door = classify(Geometry::Arc(Arc::new(
            Point2::new(0.0, 0.0),
            0.9,
            0.0,
            90.0,
        )))
        .class;
        let text = classify(Geometry::Text(Text::new(
            "ROOM",
            Point2::new(0.0, 0.0),
            0.2,
            0.0,
        )))
        .class;

        assert_eq!(column, ElementClass::Column);
        assert_eq!(door, ElementClass::Door);
        assert_eq!(text, ElementClass::Annotation);
        // Wall archetype should read as a linear architectural element, not a
        // column/door/text.
        assert!(
            !matches!(
                wall,
                ElementClass::Column | ElementClass::Door | ElementClass::Annotation
            ),
            "a long line classified as {:?}",
            wall
        );

        // A big furniture rectangle should not be a column or annotation.
        let furn = classify(Geometry::Polyline(Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.4, 0.0),
                Point2::new(1.4, 0.9),
                Point2::new(0.0, 0.9),
            ],
            true,
        )))
        .class;
        assert!(
            !matches!(furn, ElementClass::Annotation),
            "furniture rectangle classified as {:?}",
            furn
        );
    }

    #[test]
    fn confidence_in_unit_range() {
        let p = classify(Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 0.2)));
        assert!(p.confidence >= 0.0 && p.confidence <= 1.0);
    }
}
