//! Standard-detail recommendation (AI-native Phase 3 deferred service).
//!
//! Retrieval only: every recommendation cites a passage already indexed in the
//! local corpus (typically `libraries/details/*.md` via `ASKDOCS INDEX` or the
//! `RECOMMEND DETAIL` seeder). Nothing is generated, and nothing is inserted
//! into the drawing — [`Action::None`](crate::Action::None).

use aadt_vecdb::CorpusDb;

use crate::{Action, SuggestionKind, SuggestionSet};

/// Default number of detail recommendations to surface.
pub const DEFAULT_TOP_K: usize = 3;

/// Recommend standard-detail passages matching `query` from an indexed corpus.
///
/// Returns an empty set when the corpus has no matching passages — the caller
/// should say so rather than inventing a detail.
pub fn recommend_details(corpus: &CorpusDb, query: &str, k: usize) -> SuggestionSet {
    let mut set = SuggestionSet::new();
    let q = query.trim();
    if q.is_empty() || k == 0 {
        return set;
    }
    for hit in corpus.search(q, k) {
        let summary = format!(
            "detail {}:{} — {}",
            hit.source,
            hit.seq,
            first_line(&hit.text)
        );
        let rationale = format!(
            "matched {q:?} (score {:.2}) — {}:{} says: {}",
            hit.score, hit.source, hit.seq, hit.text
        );
        set.push(
            SuggestionKind::DetailRecommend,
            // BM25 scores are unbounded; clamp a soft confidence for display.
            confidence_from_score(hit.score),
            summary,
            rationale,
            Vec::new(),
            Action::None,
        );
    }
    set
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("(empty passage)")
        .trim_start_matches('#')
        .trim()
        .to_string()
}

fn confidence_from_score(score: f32) -> f32 {
    // Softmap: strong BM25 hits land near 1.0; weak ones stay above 0.4 so the
    // suggestion still reads as a real candidate rather than noise.
    (0.4 + score / (score + 4.0)).clamp(0.4, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_details() -> CorpusDb {
        let mut corpus = CorpusDb::new();
        corpus.index_text(
            "SD-WALL-EXT-CAVITY.md",
            "# External cavity wall\n\n\
             Use this detail for external brick cavity wall construction.\n\n\
             Typical build-up: 230 mm outer brick leaf, 50 mm clear cavity.",
        );
        corpus.index_text(
            "SD-DOOR-FRAME-HEAD.md",
            "# Door frame head\n\n\
             Clear opening width for a single residential leaf is typically 900 mm clear.",
        );
        corpus.index_text(
            "SD-ROOM-FINISH-FLOOR.md",
            "# Habitable room floor finishes\n\n\
             Habitable rooms shall meet the minimum floor area and minimum clear room width.",
        );
        corpus
    }

    #[test]
    fn wall_query_ranks_cavity_wall_detail() {
        let corpus = seed_details();
        let set = recommend_details(&corpus, "external wall cavity brick", 3);
        assert!(!set.is_empty());
        let top = set.get(1).unwrap();
        assert_eq!(top.kind, SuggestionKind::DetailRecommend);
        assert!(!top.action.is_applicable());
        assert!(
            top.summary.contains("SD-WALL-EXT-CAVITY") || top.rationale.contains("cavity"),
            "top: {} / {}",
            top.summary,
            top.rationale
        );
    }

    #[test]
    fn door_query_ranks_door_detail() {
        let corpus = seed_details();
        let set = recommend_details(&corpus, "door clear opening width", 2);
        assert!(!set.is_empty());
        assert!(
            set.get(1).unwrap().summary.contains("SD-DOOR")
                || set.get(1).unwrap().rationale.contains("900 mm"),
            "{}",
            set.get(1).unwrap().summary
        );
    }

    #[test]
    fn empty_query_or_corpus_yields_nothing() {
        let corpus = CorpusDb::new();
        assert!(recommend_details(&corpus, "wall", 3).is_empty());
        let seeded = seed_details();
        assert!(recommend_details(&seeded, "   ", 3).is_empty());
    }

    #[test]
    fn recommendations_are_deterministic() {
        let corpus = seed_details();
        let a = recommend_details(&corpus, "habitable room area width", 3);
        let b = recommend_details(&corpus, "habitable room area width", 3);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.summary, y.summary);
            assert_eq!(x.rationale, y.rationale);
        }
    }
}
