//! Semantic search over the vector database.
//!
//! Three queries:
//! - **`find_by_text`** — embed a text query with char n-gram hashing, then cosine
//!   search over each entity's text label (layer name + annotation text). Useful
//!   for `FIND wall` or `FIND bedroom`.
//! - **`find_similar_to`** — find entities whose geometric embedding is closest to
//!   a given entity's stored embedding; skips the query entity itself.
//! - **`find_duplicates`** — all-pairs cosine comparison of geometric embeddings,
//!   returning pairs above a similarity threshold in descending order.

use aadt_embed::{entity_text_label, text_embedding, TEXT_DIM};
use aadt_model::{Document, EntityId};

use crate::{cosine, VectorDb};

/// One result from a text or geometric similarity search.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// The matching entity.
    pub id: EntityId,
    /// Cosine similarity in `[-1, 1]`. Negative means actively dissimilar, so
    /// callers presenting matches should filter to positive scores.
    pub score: f32,
    /// The entity's current layer name (for display).
    pub layer: String,
}

/// A pair of near-duplicate entities (cosine similarity above threshold).
#[derive(Debug, Clone)]
pub struct DuplicatePair {
    pub a: EntityId,
    pub b: EntityId,
    /// Cosine similarity of their geometric embeddings.
    pub similarity: f32,
}

/// Search by text: embed `query` with the character n-gram model and compare
/// against every entity's text label (`layer + annotation text`).
///
/// Returns up to `k` hits in descending score order. Entities not in `doc` are
/// skipped; entities with a blank label after lowercasing score as zero.
pub fn find_by_text(doc: &Document, query: &str, k: usize) -> Vec<SearchHit> {
    if k == 0 || query.trim().is_empty() {
        return Vec::new();
    }
    let q_vec = text_embedding(query);
    if q_vec.iter().all(|x| *x == 0.0) {
        return Vec::new();
    }

    let mut scored: Vec<SearchHit> = doc
        .iter_entities()
        .map(|ent| {
            let label = entity_text_label(ent);
            let e_vec = text_embedding(&label);
            let score = cosine_text(&q_vec, &e_vec);
            SearchHit {
                id: ent.id,
                score,
                layer: ent.layer.clone(),
            }
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.id.0.cmp(&b.id.0))
    });
    scored.truncate(k);
    scored
}

/// Find up to `k` entities whose geometric embedding is most similar to that of
/// entity `id` in `db`. The entity itself is excluded from results.
///
/// Returns an empty list when `id` has no stored embedding.
pub fn find_similar_to(db: &VectorDb, id: EntityId, k: usize) -> Vec<SearchHit> {
    let Some(entry) = db.get(id) else {
        return Vec::new();
    };
    let q = entry.vector.clone();
    db.query(&q, k, Some(id))
        .into_iter()
        .map(|(eid, score)| SearchHit {
            id: eid,
            score,
            layer: db.get(eid).map(|e| e.layer.clone()).unwrap_or_default(),
        })
        .collect()
}

/// Find all pairs of entities in `db` whose geometric embedding similarity is
/// ≥ `threshold`, in descending order. Element features are translation- and
/// scale-normalised, so a high threshold (~0.995) reports geometry that is
/// congruent rather than merely alike.
///
/// O(n²) — fine for typical drawing sizes (< 50 k entities).
pub fn find_duplicates(db: &VectorDb, threshold: f32) -> Vec<DuplicatePair> {
    let entries = db.entries();
    let n = entries.len();
    let mut pairs = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let sim = cosine(&entries[i].vector, &entries[j].vector);
            if sim >= threshold {
                pairs.push(DuplicatePair {
                    a: entries[i].id,
                    b: entries[j].id,
                    similarity: sim,
                });
            }
        }
    }
    pairs.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    pairs
}

/// Cosine similarity between two fixed-size text embedding arrays.
fn cosine_text(a: &[f32; TEXT_DIM], b: &[f32; TEXT_DIM]) -> f32 {
    cosine(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MappingStatus, VecEntry};
    use aadt_embed::{EMBED_DIM, MODEL_ID};
    use aadt_geometry::{Circle, Point2, Segment};
    use aadt_model::Geometry;

    fn make_db() -> VectorDb {
        let mut db = VectorDb::new(MODEL_ID, EMBED_DIM);
        // Two identical geometric embeddings → duplicates.
        let v1 = vec![1.0f32, 0.0, 0.0, 0.0];
        let v2 = vec![1.0f32, 0.0, 0.0, 0.0];
        let v3 = vec![0.0f32, 1.0, 0.0, 0.0]; // orthogonal
        db.upsert(VecEntry {
            id: EntityId(1),
            vector: v1,
            layer: "A-WALL".into(),
            category: None,
            confidence: 0.9,
            status: MappingStatus::Auto,
        });
        db.upsert(VecEntry {
            id: EntityId(2),
            vector: v2,
            layer: "A-WALL".into(),
            category: None,
            confidence: 0.9,
            status: MappingStatus::Auto,
        });
        db.upsert(VecEntry {
            id: EntityId(3),
            vector: v3,
            layer: "A-COLS".into(),
            category: None,
            confidence: 0.85,
            status: MappingStatus::Auto,
        });
        db
    }

    #[test]
    fn find_duplicates_finds_identical_pair() {
        let db = make_db();
        let pairs = find_duplicates(&db, 0.99);
        assert_eq!(pairs.len(), 1);
        let ids: (u64, u64) = (pairs[0].a.0, pairs[0].b.0);
        assert!(ids == (1, 2) || ids == (2, 1));
        assert!((pairs[0].similarity - 1.0).abs() < 1e-5);
    }

    #[test]
    fn find_duplicates_empty_below_threshold() {
        let db = make_db();
        // The orthogonal pair should not appear.
        let pairs = find_duplicates(&db, 1.0);
        // cosine([1,0,0,0], [0,1,0,0]) = 0, so nothing meets 1.0
        assert!(pairs
            .iter()
            .all(|p| (p.a.0, p.b.0) != (1, 3) && (p.a.0, p.b.0) != (3, 1)));
    }

    #[test]
    fn find_similar_to_excludes_self() {
        let db = make_db();
        let hits = find_similar_to(&db, EntityId(1), 5);
        assert!(hits.iter().all(|h| h.id != EntityId(1)));
        // Entity 2 (identical vector) should be first.
        assert_eq!(hits[0].id, EntityId(2));
    }

    #[test]
    fn find_by_text_ranks_layer_match_high() {
        let mut doc = aadt_model::Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(5.0, 0.0))),
        );
        doc.add_geometry(
            "A-COLS",
            Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 0.2)),
        );
        let hits = find_by_text(&doc, "wall", 5);
        assert!(!hits.is_empty());
        // The entity on A-WALL should score higher than the one on A-COLS.
        let wall_score = hits.iter().find(|h| h.layer == "A-WALL").map(|h| h.score);
        let col_score = hits.iter().find(|h| h.layer == "A-COLS").map(|h| h.score);
        if let (Some(ws), Some(cs)) = (wall_score, col_score) {
            assert!(
                ws > cs,
                "A-WALL should score higher for query 'wall': {ws:.3} vs {cs:.3}"
            );
        }
    }
}
