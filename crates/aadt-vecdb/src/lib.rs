//! # aadt-vecdb — custom vector database over drawing-element embeddings
//!
//! A drawing is *stored as* a set of vectors: each entity gets an embedding (from
//! [`aadt_embed`]) plus the **predefined metadata** it was mapped to (an AIA
//! standard layer and a model-space category) and a **reconciliation status**.
//! The database supports cosine similarity queries and seeds itself with the
//! model's per-class prototype vectors, so it works even on a drawing with no
//! labeled examples yet.
//!
//! The [`mapping`] engine embeds and classifies every element, **auto-assigns**
//! confident predictions, and leaves uncertain ones **pending** for
//! reconciliation — never silently mis-filing geometry.
//!
//! Two retrieval layers sit on top (AI-native Phase 2):
//! - [`search`] — semantic search over the drawing: by text label, by geometric
//!   similarity to a given element, or for near-duplicate geometry.
//! - [`corpus`] — a local BM25-ranked text corpus over sidecar spec documents,
//!   returning cited passages. Retrieval only: nothing is generated.

pub mod corpus;
mod mapping;
pub mod search;

use aadt_embed::ElementClass;
use aadt_model::{EntityId, SpaceCategory};

pub use corpus::{CorpusChunk, CorpusDb, CorpusResult};
pub use mapping::{accept, map_metadata, set_class, set_layer, MappingReport, Thresholds};
pub use search::{find_by_text, find_duplicates, find_similar_to, DuplicatePair, SearchHit};

/// Reconciliation state of an element's metadata mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingStatus {
    /// Auto-assigned because the model was confident.
    Auto,
    /// Low confidence — waiting for a human decision.
    Pending,
    /// A human accepted the prediction.
    Confirmed,
    /// A human overrode the prediction with a different metadata.
    Overridden,
}

impl MappingStatus {
    /// Stable lowercase token used for `.aadt` persistence.
    pub fn as_str(self) -> &'static str {
        match self {
            MappingStatus::Auto => "auto",
            MappingStatus::Pending => "pending",
            MappingStatus::Confirmed => "confirmed",
            MappingStatus::Overridden => "overridden",
        }
    }

    /// Parse a persisted token (defaults to [`MappingStatus::Auto`]).
    pub fn parse(s: &str) -> MappingStatus {
        match s.trim().to_ascii_lowercase().as_str() {
            "pending" => MappingStatus::Pending,
            "confirmed" => MappingStatus::Confirmed,
            "overridden" => MappingStatus::Overridden,
            _ => MappingStatus::Auto,
        }
    }

    /// Whether this status reflects an explicit human decision (never auto-remapped).
    pub fn is_manual(self) -> bool {
        matches!(self, MappingStatus::Confirmed | MappingStatus::Overridden)
    }
}

/// One stored element: its embedding plus the metadata it maps to.
#[derive(Debug, Clone)]
pub struct VecEntry {
    /// The entity this vector belongs to.
    pub id: EntityId,
    /// L2-normalized embedding.
    pub vector: Vec<f32>,
    /// Predicted (or overridden) AIA standard layer.
    pub layer: String,
    /// Predicted (or overridden) model-space category, if any.
    pub category: Option<SpaceCategory>,
    /// Model confidence in `[0, 1]` at the time it was mapped.
    pub confidence: f32,
    /// Reconciliation status.
    pub status: MappingStatus,
}

/// The custom vector database: one [`VecEntry`] per mapped entity.
#[derive(Debug, Clone)]
pub struct VectorDb {
    model_id: String,
    dim: usize,
    entries: Vec<VecEntry>,
}

impl VectorDb {
    /// Empty database for a given model id and embedding dimension.
    pub fn new(model_id: impl Into<String>, dim: usize) -> Self {
        Self {
            model_id: model_id.into(),
            dim,
            entries: Vec::new(),
        }
    }

    /// The model identifier these vectors were produced with.
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Embedding dimensionality.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Number of stored entities.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the database holds no entities.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// All entries, ascending by entity id.
    pub fn entries(&self) -> &[VecEntry] {
        &self.entries
    }

    /// Look up a stored entry by entity id.
    pub fn get(&self, id: EntityId) -> Option<&VecEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Insert `entry`, replacing any existing entry with the same id. Entries are
    /// kept sorted by id so iteration and queries are deterministic.
    pub fn upsert(&mut self, entry: VecEntry) {
        match self.entries.binary_search_by(|e| e.id.0.cmp(&entry.id.0)) {
            Ok(i) => self.entries[i] = entry,
            Err(i) => self.entries.insert(i, entry),
        }
    }

    /// Remove the entry for `id`. Returns whether one was present.
    pub fn remove(&mut self, id: EntityId) -> bool {
        if let Ok(i) = self.entries.binary_search_by(|e| e.id.0.cmp(&id.0)) {
            self.entries.remove(i);
            true
        } else {
            false
        }
    }

    /// Drop every stored entity (prototypes are not stored here).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Entities still awaiting reconciliation (status `Pending`), ascending by id.
    pub fn pending(&self) -> Vec<&VecEntry> {
        self.entries
            .iter()
            .filter(|e| e.status == MappingStatus::Pending)
            .collect()
    }

    /// The `k` stored entities most similar to `q` by cosine similarity,
    /// descending, with `id` breaking ties for determinism. A `skip` id (e.g. the
    /// query element itself) is excluded.
    pub fn query(&self, q: &[f32], k: usize, skip: Option<EntityId>) -> Vec<(EntityId, f32)> {
        let mut scored: Vec<(EntityId, f32)> = self
            .entries
            .iter()
            .filter(|e| Some(e.id) != skip)
            .map(|e| (e.id, cosine(q, &e.vector)))
            .collect();
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0 .0.cmp(&b.0 .0))
        });
        scored.truncate(k);
        scored
    }
}

/// Cosine similarity between two vectors (`0.0` if either is degenerate or the
/// lengths differ). Embeddings are already unit-length, so this is usually a dot.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom <= 1e-12 {
        0.0
    } else {
        dot / denom
    }
}

/// Nearest class by cosine to the model's per-class prototype vectors — a
/// pure-similarity classification path over the labeled reference set.
pub fn nearest_class(q: &[f32]) -> Option<(ElementClass, f32)> {
    aadt_embed::class_prototypes()
        .into_iter()
        .map(|(c, v)| (c, cosine(q, &v)))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(id: u64, v: Vec<f32>, status: MappingStatus) -> VecEntry {
        VecEntry {
            id: EntityId(id),
            vector: v,
            layer: "A-WALL".into(),
            category: Some(SpaceCategory::Walls),
            confidence: 0.9,
            status,
        }
    }

    #[test]
    fn upsert_keeps_sorted_and_replaces() {
        let mut db = VectorDb::new("m", 2);
        db.upsert(e(3, vec![1.0, 0.0], MappingStatus::Auto));
        db.upsert(e(1, vec![0.0, 1.0], MappingStatus::Auto));
        db.upsert(e(2, vec![1.0, 1.0], MappingStatus::Pending));
        assert_eq!(
            db.entries().iter().map(|e| e.id.0).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        // Replace id 2.
        db.upsert(e(2, vec![0.5, 0.5], MappingStatus::Confirmed));
        assert_eq!(
            db.get(EntityId(2)).unwrap().status,
            MappingStatus::Confirmed
        );
        assert_eq!(db.len(), 3);
    }

    #[test]
    fn query_ranks_by_cosine_and_breaks_ties_by_id() {
        let mut db = VectorDb::new("m", 2);
        db.upsert(e(1, vec![1.0, 0.0], MappingStatus::Auto));
        db.upsert(e(2, vec![0.0, 1.0], MappingStatus::Auto));
        db.upsert(e(3, vec![1.0, 0.0], MappingStatus::Auto)); // identical to 1
        let out = db.query(&[1.0, 0.0], 2, None);
        assert_eq!(out[0].0, EntityId(1), "tie broken by ascending id");
        assert_eq!(out[1].0, EntityId(3));
        // Skip the query element itself.
        let out = db.query(&[1.0, 0.0], 2, Some(EntityId(1)));
        assert_eq!(out[0].0, EntityId(3));
    }

    #[test]
    fn pending_filters_status() {
        let mut db = VectorDb::new("m", 2);
        db.upsert(e(1, vec![1.0, 0.0], MappingStatus::Auto));
        db.upsert(e(2, vec![0.0, 1.0], MappingStatus::Pending));
        assert_eq!(db.pending().len(), 1);
        assert_eq!(db.pending()[0].id, EntityId(2));
    }

    #[test]
    fn status_round_trips_through_tokens() {
        for s in [
            MappingStatus::Auto,
            MappingStatus::Pending,
            MappingStatus::Confirmed,
            MappingStatus::Overridden,
        ] {
            assert_eq!(MappingStatus::parse(s.as_str()), s);
        }
    }
}
