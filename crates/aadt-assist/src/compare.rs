//! Revision-comparison narration (AI-native Phase 3 deferred service).
//!
//! Wraps the deterministic M7 fingerprint diff ([`aadt_pdf::compare`]) with an
//! engineering-intent section from detection and optional corpus citations for
//! the layers that changed. The result is always advisory
//! ([`Action::None`](crate::Action::None)): a transmittal note, not a geometry
//! edit. Accepting is refused; rejecting is audited like every other suggestion.
//!
//! The change set itself stays in `aadt-pdf` — this module never replaces it.

use std::collections::BTreeSet;

use aadt_detect::{detect, Detection, DetectionConfig};
use aadt_model::Document;
use aadt_pdf::{compare, Diff};
use aadt_vecdb::CorpusDb;

use crate::{Action, SuggestionKind, SuggestionSet};

/// Narrate the change from `before` to `after` as one advisory suggestion.
///
/// When `corpus` is provided, each distinct changed layer contributes at most
/// one BM25 citation (same retrieval path as [`crate::rules::check`]).
pub fn narrate_revision(
    before: &Document,
    after: &Document,
    corpus: Option<&CorpusDb>,
) -> SuggestionSet {
    let diff = compare(before, after);
    let det_before = detect(before, &DetectionConfig::default());
    let det_after = detect(after, &DetectionConfig::default());

    let summary = summary_line(&diff);
    let mut rationale = diff.narrate();
    if !rationale.ends_with('\n') {
        rationale.push('\n');
    }
    rationale.push_str(&detection_delta(&det_before, &det_after));

    if let Some(c) = corpus {
        let cites = cite_changed_layers(c, &diff);
        if !cites.is_empty() {
            rationale.push_str("\nRelated passages (indexed corpus):\n");
            for line in cites {
                rationale.push_str(&line);
                rationale.push('\n');
            }
        }
    }

    let mut set = SuggestionSet::new();
    set.push(
        SuggestionKind::RevisionCompare,
        1.0,
        summary,
        rationale,
        Vec::new(),
        Action::None,
    );
    set
}

/// Compact one-line summary for `SUGGEST LIST` / the proposal header.
fn summary_line(diff: &Diff) -> String {
    if diff.is_unchanged() {
        "no geometric changes between revisions".into()
    } else {
        format!(
            "{} entit{} added, {} removed, {} unchanged",
            diff.added_count(),
            if diff.added_count() == 1 { "y" } else { "ies" },
            diff.removed_count(),
            diff.unchanged,
        )
    }
}

/// Detection counts before → after, only for categories that differ.
fn detection_delta(before: &Detection, after: &Detection) -> String {
    let pairs = [
        ("walls", before.walls.len(), after.walls.len()),
        (
            "rooms",
            before.merged_rooms().len(),
            after.merged_rooms().len(),
        ),
        ("openings", before.openings.len(), after.openings.len()),
        (
            "structural",
            before.structural.len(),
            after.structural.len(),
        ),
        ("furniture", before.furniture.len(), after.furniture.len()),
    ];
    let mut lines = Vec::new();
    for (label, a, b) in pairs {
        if a == b {
            continue;
        }
        let delta = b as i64 - a as i64;
        lines.push(format!("  {label}: {a} → {b} ({delta:+})"));
    }
    if lines.is_empty() {
        "\nEngineering intent (detection): no count changes.\n".into()
    } else {
        format!("\nEngineering intent (detection):\n{}\n", lines.join("\n"))
    }
}

/// BM25 citations for layers that gained or lost entities.
fn cite_changed_layers(corpus: &CorpusDb, diff: &Diff) -> Vec<String> {
    let mut layers = BTreeSet::new();
    for c in diff.added.iter().chain(diff.removed.iter()) {
        layers.insert(c.layer.clone());
    }
    let mut out = Vec::new();
    for layer in layers {
        let query = citation_query_for_layer(&layer);
        if let Some(hit) = corpus.search(&query, 1).into_iter().next() {
            out.push(format!(
                "  [{layer}] {}:{} — {}",
                hit.source, hit.seq, hit.text
            ));
        }
    }
    out
}

fn citation_query_for_layer(layer: &str) -> String {
    let upper = layer.to_ascii_uppercase();
    let topic = if upper.contains("WALL") {
        "wall construction revision"
    } else if upper.contains("DOOR") {
        "door opening revision"
    } else if upper.contains("WIND") || upper.contains("GLAZ") {
        "window glazing revision"
    } else if upper.contains("ROOM") || upper.contains("AREA") {
        "room area revision"
    } else if upper.contains("GRID") {
        "structural grid revision"
    } else if upper.contains("DIM") || upper.contains("ANNO") {
        "dimension annotation revision"
    } else {
        "drawing revision change"
    };
    format!("{topic} {layer}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Point2, Segment};
    use aadt_model::Geometry;

    fn line_doc(lines: &[(&str, f64, f64, f64, f64)]) -> Document {
        let mut doc = Document::new();
        for (layer, x1, y1, x2, y2) in lines {
            doc.add_geometry(
                *layer,
                Geometry::Line(Segment::new(Point2::new(*x1, *y1), Point2::new(*x2, *y2))),
            );
        }
        doc
    }

    #[test]
    fn unchanged_revisions_produce_one_advisory() {
        let a = line_doc(&[("A-WALL", 0.0, 0.0, 10.0, 0.0)]);
        let b = line_doc(&[("A-WALL", 0.0, 0.0, 10.0, 0.0)]);
        let set = narrate_revision(&a, &b, None);
        assert_eq!(set.len(), 1);
        let s = set.get(1).unwrap();
        assert_eq!(s.kind, SuggestionKind::RevisionCompare);
        assert!(!s.action.is_applicable());
        assert_eq!(s.summary, "no geometric changes between revisions");
        assert!(s.rationale.contains("identical"));
        assert!(s.rationale.contains("no count changes"));
    }

    #[test]
    fn added_lines_appear_in_summary_and_rationale() {
        let before = line_doc(&[("A-WALL", 0.0, 0.0, 10.0, 0.0)]);
        let after = line_doc(&[
            ("A-WALL", 0.0, 0.0, 10.0, 0.0),
            ("A-WALL", 0.0, 8.0, 10.0, 8.0),
            ("A-WALL", 0.0, 4.0, 10.0, 4.0),
        ]);
        let set = narrate_revision(&before, &after, None);
        let s = set.get(1).unwrap();
        assert!(s.summary.contains("2 entities added"));
        assert!(s.rationale.contains("+ 2 lines on A-WALL"));
        // Fingerprint diff is authoritative; detection delta is additive.
        assert!(s.rationale.contains("Engineering intent (detection)"));
    }

    #[test]
    fn narration_is_deterministic() {
        let before = line_doc(&[("A-WALL", 0.0, 0.0, 1.0, 0.0)]);
        let after = line_doc(&[
            ("A-WALL", 0.0, 0.0, 1.0, 0.0),
            ("A-GRID", 0.0, 0.0, 0.0, 1.0),
        ]);
        let a = narrate_revision(&before, &after, None);
        let b = narrate_revision(&before, &after, None);
        assert_eq!(a.get(1).unwrap().summary, b.get(1).unwrap().summary);
        assert_eq!(a.get(1).unwrap().rationale, b.get(1).unwrap().rationale);
    }

    #[test]
    fn corpus_citations_attach_for_changed_layers() {
        let mut corpus = CorpusDb::new();
        corpus.index_text(
            "spec.md",
            "Wall construction revisions shall be clouded on the affected plan.\n\n\
             Door width changes require a door schedule update.",
        );
        let before = line_doc(&[("A-WALL", 0.0, 0.0, 10.0, 0.0)]);
        let after = line_doc(&[
            ("A-WALL", 0.0, 0.0, 10.0, 0.0),
            ("A-WALL", 0.0, 8.0, 10.0, 8.0),
        ]);
        let set = narrate_revision(&before, &after, Some(&corpus));
        let rationale = &set.get(1).unwrap().rationale;
        assert!(
            rationale.contains("Related passages"),
            "expected citations, got:\n{rationale}"
        );
        assert!(rationale.contains("[A-WALL]"));
    }
}
