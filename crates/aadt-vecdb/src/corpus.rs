//! A simple local text corpus for RAG-style retrieval.
//!
//! The corpus indexes plain-text or Markdown sidecar files (specs, codes,
//! standards) as a set of chunks — paragraph-granularity, embedded with the
//! character n-gram text model. `ASKDOCS <query>` searches the corpus and
//! returns cited passages.
//!
//! No external dependencies; all embedding is done by [`aadt_embed::text`].
//! Chunks persist in the `.aadt` file (schema v9 tables `corpus_source` /
//! `corpus_chunk`); the in-memory `CorpusDb` is the runtime representation.

use aadt_embed::text::{text_embedding, TEXT_DIM};

use crate::cosine;

// ── Types ─────────────────────────────────────────────────────────────────────

/// One indexed paragraph from a source document.
#[derive(Debug, Clone)]
pub struct CorpusChunk {
    /// Stable id within the corpus (assigned at index time, persisted).
    pub id: u64,
    /// Human-readable source label (file path or title).
    pub source: String,
    /// Zero-based paragraph index within the source.
    pub seq: u32,
    /// The paragraph text (trimmed, non-empty).
    pub text: String,
    /// Text embedding of `text`.
    pub vector: [f32; TEXT_DIM],
}

/// One hit from a corpus search.
#[derive(Debug, Clone)]
pub struct CorpusResult {
    /// Relevance of this chunk to the query. Normally a BM25 score — unbounded
    /// above, comparable only *within* one result set, so present it as a rank
    /// or a relative figure rather than a percentage. Falls back to a cosine
    /// similarity in `[-1, 1]` when no query term occurs in the corpus.
    pub score: f32,
    /// Source label.
    pub source: String,
    /// Zero-based paragraph index.
    pub seq: u32,
    /// Paragraph text.
    pub text: String,
}

// ── CorpusDb ──────────────────────────────────────────────────────────────────

/// In-memory corpus: a collection of embedded text chunks from sidecar documents.
#[derive(Debug, Clone, Default)]
pub struct CorpusDb {
    pub(crate) chunks: Vec<CorpusChunk>,
    pub(crate) next_id: u64,
}

impl CorpusDb {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of indexed chunks.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// All chunks, in index order.
    pub fn chunks(&self) -> &[CorpusChunk] {
        &self.chunks
    }

    /// Index a text string (plain text or Markdown) under `source`. Splits on
    /// blank lines into paragraphs; empty paragraphs are skipped. Returns the
    /// number of new chunks added.
    pub fn index_text(&mut self, source: &str, text: &str) -> usize {
        let start = self.chunks.len();
        let mut seq: u32 = 0;
        for para in paragraphs(text) {
            if para.is_empty() {
                continue;
            }
            self.next_id += 1;
            let vector = text_embedding(&para);
            self.chunks.push(CorpusChunk {
                id: self.next_id,
                source: source.to_string(),
                seq,
                text: para,
                vector,
            });
            seq += 1;
        }
        self.chunks.len() - start
    }

    /// Restore a persisted chunk (keeps the id sequence correct).
    pub fn restore(&mut self, chunk: CorpusChunk) {
        self.next_id = self.next_id.max(chunk.id);
        self.chunks.push(chunk);
    }

    /// Search the corpus by text query. Returns up to `k` chunks in descending
    /// relevance order. Empty when the corpus has no chunks or query is blank.
    ///
    /// Ranking is **BM25** over word tokens — the standard lexical retrieval
    /// function. It weights each query term by how rare it is across the corpus,
    /// so shared boilerplate ("shall be", "mm", "minimum") contributes almost
    /// nothing and a passage wins on the terms that actually distinguish it.
    ///
    /// When no query term appears anywhere in the corpus, scoring falls back to
    /// cosine over the character n-gram embeddings, which still matches
    /// morphological variants ("stair" ~ "staircase") that BM25 would miss.
    pub fn search(&self, query: &str, k: usize) -> Vec<CorpusResult> {
        if k == 0 || query.trim().is_empty() || self.chunks.is_empty() {
            return Vec::new();
        }

        let mut scored = self.bm25_scores(query);
        if scored.iter().any(|(s, _)| *s > 0.0) {
            // Keep only passages that actually matched — a zero-scored chunk
            // shares no query term and would just pad the answer.
            scored.retain(|(s, _)| *s > 0.0);
        } else {
            // No lexical overlap at all → fall back to fuzzy embedding similarity.
            let q = text_embedding(query);
            if q.iter().all(|x| *x == 0.0) {
                return Vec::new();
            }
            scored = self
                .chunks
                .iter()
                .enumerate()
                .map(|(i, c)| (cosine(&q, &c.vector), i))
                .collect();
        }

        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.cmp(&b.1))
        });
        scored.truncate(k);
        scored
            .into_iter()
            .map(|(score, i)| {
                let c = &self.chunks[i];
                CorpusResult {
                    score,
                    source: c.source.clone(),
                    seq: c.seq,
                    text: c.text.clone(),
                }
            })
            .collect()
    }

    /// BM25 relevance of every chunk against `query`, as `(score, index)` pairs.
    fn bm25_scores(&self, query: &str) -> Vec<(f32, usize)> {
        // BM25 free parameters at their standard values: `K1` controls term
        // frequency saturation, `B` how strongly to normalise by passage length.
        const K1: f32 = 1.5;
        const B: f32 = 0.75;

        let docs: Vec<Vec<String>> = self.chunks.iter().map(|c| tokenize(&c.text)).collect();
        let n = docs.len() as f32;
        let avgdl = if docs.is_empty() {
            0.0
        } else {
            docs.iter().map(|d| d.len() as f32).sum::<f32>() / n
        };

        let q_terms = tokenize(query);
        docs.iter()
            .enumerate()
            .map(|(i, doc)| {
                let dl = doc.len() as f32;
                let mut score = 0.0f32;
                for term in &q_terms {
                    let f = doc.iter().filter(|t| *t == term).count() as f32;
                    if f == 0.0 {
                        continue;
                    }
                    let df = docs.iter().filter(|d| d.contains(term)).count() as f32;
                    let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
                    let denom = f + K1 * (1.0 - B + B * dl / avgdl.max(1e-6));
                    score += idf * (f * (K1 + 1.0)) / denom.max(1e-6);
                }
                (score, i)
            })
            .collect()
    }

    /// Whether a source (by label) has already been indexed.
    pub fn has_source(&self, source: &str) -> bool {
        self.chunks.iter().any(|c| c.source == source)
    }

    /// Sources (labels) of all indexed documents, deduplicated.
    pub fn sources(&self) -> Vec<&str> {
        let mut seen = Vec::new();
        for c in &self.chunks {
            if !seen.contains(&c.source.as_str()) {
                seen.push(c.source.as_str());
            }
        }
        seen
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Split text into lowercase word tokens for BM25. Single characters are dropped
/// — they carry no retrieval signal and inflate passage length.
///
/// Tokens are lightly stemmed by stripping a plural `s`, so a query for
/// "dimension" matches a clause headed "Room Dimensions". Without it, exact
/// token matching misses the singular/plural pairs that spec prose is full of
/// (wall/walls, door/doors, room/rooms).
fn tokenize(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 1)
        .map(|t| stem(&t.to_ascii_lowercase()))
        .collect()
}

/// Strip a trailing plural `s` (but not `ss`, and never below three letters, so
/// "less" and "is"/"as" survive intact).
fn stem(t: &str) -> String {
    if t.len() > 3 && t.ends_with('s') && !t.ends_with("ss") {
        t[..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

/// Split `text` into non-empty paragraphs on blank lines, normalising whitespace.
fn paragraphs(text: &str) -> impl Iterator<Item = String> + '_ {
    // Collect lines and split on blank-line boundaries.
    let mut result = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            let para = current.trim().to_string();
            if !para.is_empty() {
                result.push(para);
            }
            current.clear();
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(trimmed);
        }
    }
    let para = current.trim().to_string();
    if !para.is_empty() {
        result.push(para);
    }
    result.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "\
Wall Construction
External walls shall be 230 mm brick with 50 mm cavity.

Internal Partitions
Internal walls shall be 115 mm brick or 100 mm blockwork.

Fire Ratings
All walls shall achieve a minimum fire rating of 120 minutes.
";

    #[test]
    fn index_and_search_basic() {
        let mut db = CorpusDb::new();
        let n = db.index_text("spec.txt", SPEC);
        assert_eq!(n, 3, "three paragraphs");

        let results = db.search("wall brick", 2);
        assert_eq!(results.len(), 2);
        // The first paragraph is the best hit for "wall brick".
        assert!(
            results[0].score >= results[1].score,
            "results in descending order"
        );
    }

    #[test]
    fn has_source_works() {
        let mut db = CorpusDb::new();
        db.index_text("spec.txt", SPEC);
        assert!(db.has_source("spec.txt"));
        assert!(!db.has_source("other.txt"));
    }

    #[test]
    fn empty_query_returns_empty() {
        let mut db = CorpusDb::new();
        db.index_text("spec.txt", SPEC);
        assert!(db.search("", 5).is_empty());
        assert!(db.search("   ", 5).is_empty());
    }

    #[test]
    fn restore_advances_id_sequence() {
        let mut db = CorpusDb::new();
        db.restore(CorpusChunk {
            id: 99,
            source: "a".into(),
            seq: 0,
            text: "hello".into(),
            vector: [0.0; TEXT_DIM],
        });
        db.index_text("b", "world");
        assert!(db.chunks.last().unwrap().id > 99);
    }

    #[test]
    fn fire_rating_query_finds_fire_paragraph() {
        let mut db = CorpusDb::new();
        db.index_text("spec.txt", SPEC);
        let results = db.search("fire rating", 3);
        assert!(
            results[0].text.contains("fire rating"),
            "the fire paragraph should rank first, got: {:?}",
            results[0].text
        );
    }

    #[test]
    fn bm25_beats_shared_boilerplate() {
        // Every paragraph shares "shall be … mm"; BM25's rarity weighting is what
        // stops that common phrasing from dominating the distinctive term.
        let mut db = CorpusDb::new();
        db.index_text(
            "spec.txt",
            "External walls shall be 230 mm brick.\n\n\
             Columns shall be 230 mm x 450 mm concrete.\n\n\
             Staircase treads shall be 300 mm deep.",
        );
        let results = db.search("staircase tread depth", 3);
        assert!(
            results[0].text.contains("Staircase"),
            "the staircase paragraph should rank first, got: {:?}",
            results[0].text
        );
    }

    #[test]
    fn external_wall_query_ranks_the_answer_first() {
        let mut db = CorpusDb::new();
        db.index_text("spec.txt", SPEC);
        let results = db.search("external wall thickness", 3);
        assert!(
            results[0].text.contains("External walls"),
            "the wall-construction paragraph should rank first, got: {:?}",
            results[0].text
        );
    }

    #[test]
    fn plural_and_singular_match_each_other() {
        let mut db = CorpusDb::new();
        db.index_text(
            "spec.txt",
            "Room Areas\nEvery room shall have a minimum floor area of 9 square metres.\n\n\
             Room Dimensions\nNo room shall have a clear width of less than 2.4 metres.",
        );
        // Singular "dimension" must reach the "Dimensions" clause.
        let results = db.search("room dimension width", 2);
        assert!(
            results[0].text.contains("clear width"),
            "singular query should reach the plural heading, got: {:?}",
            results[0].text
        );
    }

    #[test]
    fn zero_scoring_passages_are_not_returned() {
        let mut db = CorpusDb::new();
        db.index_text("spec.txt", SPEC);
        // Only the partitions paragraph mentions "blockwork"; the rest score zero
        // and must not pad the result set.
        let results = db.search("blockwork", 3);
        assert_eq!(results.len(), 1, "only the matching passage is returned");
        assert!(results[0].text.contains("blockwork"));
    }

    #[test]
    fn falls_back_to_fuzzy_match_when_no_term_appears() {
        // "cavitys" appears nowhere verbatim, so BM25 scores zero everywhere and
        // the embedding fallback still surfaces the closest passage.
        let mut db = CorpusDb::new();
        db.index_text("spec.txt", SPEC);
        let results = db.search("cavitys", 2);
        assert!(!results.is_empty(), "fallback should still return passages");
    }
}
