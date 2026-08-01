//! Deterministic code-check rule packs.
//!
//! Every rule is plain geometry against a stated threshold — no model, no
//! inference, confidence always `1.0`. That is deliberate: a code check that
//! guesses is worse than no check, because a reviewer would have to re-verify
//! it anyway. What the AI layer contributes is the **citation**: when a spec
//! corpus is indexed ([`aadt_vecdb::CorpusDb`]), each finding is paired with the
//! passage that governs it, retrieved by the Phase-2 BM25 search.
//!
//! Findings are advisory ([`Action::None`]): they report a violation and never
//! "fix" it. Choosing how to resolve a code violation is a design decision.
//!
//! The bundled thresholds follow common Indian residential practice (NBC 2016 /
//! IS 3809 orders of magnitude). They are **defaults, not law** — a project's
//! own spec, once indexed, is what a reviewer should read; the citation exists
//! so they can.

use aadt_detect::{detect, DetectionConfig, OpeningKind};
use aadt_model::{Document, EntityId};
use aadt_vecdb::CorpusDb;

use crate::{Action, SuggestionKind, SuggestionSet};

/// Which rule produced a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuleId {
    /// A habitable room is below the minimum floor area.
    MinRoomArea,
    /// A room's shorter side is below the minimum clear width.
    MinRoomWidth,
    /// A door leaf is narrower than the minimum clear opening.
    MinDoorWidth,
}

impl RuleId {
    /// All rules, in stable evaluation order.
    pub const ALL: [RuleId; 3] = [
        RuleId::MinRoomArea,
        RuleId::MinRoomWidth,
        RuleId::MinDoorWidth,
    ];

    /// Stable token used for display and for `CHECK <rule>`.
    pub fn as_str(self) -> &'static str {
        match self {
            RuleId::MinRoomArea => "min-room-area",
            RuleId::MinRoomWidth => "min-room-width",
            RuleId::MinDoorWidth => "min-door-width",
        }
    }

    /// What the rule enforces, in one line.
    pub fn description(self) -> &'static str {
        match self {
            RuleId::MinRoomArea => "habitable rooms meet a minimum floor area",
            RuleId::MinRoomWidth => "rooms meet a minimum clear width",
            RuleId::MinDoorWidth => "doors meet a minimum clear opening width",
        }
    }

    /// The query used to retrieve a governing passage from an indexed corpus.
    fn citation_query(self) -> &'static str {
        match self {
            RuleId::MinRoomArea => "minimum room area habitable",
            RuleId::MinRoomWidth => "minimum room width dimension",
            RuleId::MinDoorWidth => "door width clear opening",
        }
    }

    /// Parse a rule token (case-insensitive).
    pub fn parse(s: &str) -> Option<RuleId> {
        let t = s.trim();
        RuleId::ALL
            .into_iter()
            .find(|r| r.as_str().eq_ignore_ascii_case(t))
    }
}

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Clearly below the threshold.
    Violation,
    /// Within 10% of the threshold — worth a look, not necessarily wrong.
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Violation => "violation",
            Severity::Warning => "warning",
        }
    }
}

/// One rule finding.
#[derive(Debug, Clone)]
pub struct Finding {
    pub rule: RuleId,
    pub severity: Severity,
    /// What was measured.
    pub measured: f64,
    /// The threshold it was measured against.
    pub threshold: f64,
    /// Units of both figures ("m²", "m").
    pub units: &'static str,
    /// Human-readable subject ("room #2", "door #1").
    pub subject: String,
    /// Entities involved.
    pub targets: Vec<EntityId>,
    /// Governing passage from the indexed corpus, when one was found.
    pub citation: Option<Citation>,
}

/// A passage from the indexed corpus that governs a finding.
#[derive(Debug, Clone)]
pub struct Citation {
    pub source: String,
    pub paragraph: u32,
    pub text: String,
}

// ── Thresholds ────────────────────────────────────────────────────────────────

/// Minimum floor area of a habitable room, m².
const MIN_ROOM_AREA: f64 = 9.0;
/// Minimum clear width of a room, m.
const MIN_ROOM_WIDTH: f64 = 2.4;
/// Minimum clear width of a door, m.
const MIN_DOOR_WIDTH: f64 = 0.75;
/// Within this fraction of a threshold, a finding is a warning not a violation.
const WARN_BAND: f64 = 0.10;

/// Run the rule pack over `doc`, optionally citing `corpus`.
///
/// `only` restricts the run to one rule; `None` runs all of them. Findings come
/// back in `RuleId::ALL` order, then detection order — deterministic.
pub fn check(doc: &Document, corpus: Option<&CorpusDb>, only: Option<RuleId>) -> Vec<Finding> {
    let det = detect(doc, &DetectionConfig::default());
    let rooms = det.merged_rooms();
    let mut out = Vec::new();

    for rule in RuleId::ALL {
        if only.is_some_and(|r| r != rule) {
            continue;
        }
        match rule {
            RuleId::MinRoomArea => {
                for room in &rooms {
                    if let Some(sev) = grade(room.area, MIN_ROOM_AREA) {
                        out.push(Finding {
                            rule,
                            severity: sev,
                            measured: room.area,
                            threshold: MIN_ROOM_AREA,
                            units: "m²",
                            subject: format!("room #{}", room.id),
                            targets: vec![room.source],
                            citation: None,
                        });
                    }
                }
            }
            RuleId::MinRoomWidth => {
                for room in &rooms {
                    let w = short_side(room);
                    if let Some(sev) = grade(w, MIN_ROOM_WIDTH) {
                        out.push(Finding {
                            rule,
                            severity: sev,
                            measured: w,
                            threshold: MIN_ROOM_WIDTH,
                            units: "m",
                            subject: format!("room #{}", room.id),
                            targets: vec![room.source],
                            citation: None,
                        });
                    }
                }
            }
            RuleId::MinDoorWidth => {
                for op in &det.openings {
                    if op.kind != OpeningKind::Door {
                        continue;
                    }
                    // Only openings that declare a width can be checked; one that
                    // doesn't is not a violation, it is unmeasured.
                    let Some(w) = op.width else { continue };
                    if let Some(sev) = grade(w, MIN_DOOR_WIDTH) {
                        out.push(Finding {
                            rule,
                            severity: sev,
                            measured: w,
                            threshold: MIN_DOOR_WIDTH,
                            units: "m",
                            subject: format!("door #{}", op.number),
                            targets: vec![op.source],
                            citation: None,
                        });
                    }
                }
            }
        }
    }

    if let Some(c) = corpus {
        for f in &mut out {
            f.citation = cite(c, f.rule);
        }
    }
    out
}

/// Grade a measurement against a threshold: `None` when it comfortably passes.
fn grade(measured: f64, threshold: f64) -> Option<Severity> {
    if measured < threshold {
        Some(Severity::Violation)
    } else if measured < threshold * (1.0 + WARN_BAND) {
        Some(Severity::Warning)
    } else {
        None
    }
}

/// The shorter side of a room's axis-aligned bounding box — a conservative
/// stand-in for clear width that needs no shape analysis.
fn short_side(room: &aadt_detect::Room) -> f64 {
    let pts = &room.boundary.vertices;
    if pts.is_empty() {
        return 0.0;
    }
    let (mut min_x, mut max_x) = (pts[0].x, pts[0].x);
    let (mut min_y, mut max_y) = (pts[0].y, pts[0].y);
    for p in pts {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_y = min_y.min(p.y);
        max_y = max_y.max(p.y);
    }
    (max_x - min_x).min(max_y - min_y)
}

/// Retrieve the passage governing `rule` from the corpus, if one matches.
fn cite(corpus: &CorpusDb, rule: RuleId) -> Option<Citation> {
    corpus
        .search(rule.citation_query(), 1)
        .into_iter()
        .next()
        .map(|r| Citation {
            source: r.source,
            paragraph: r.seq,
            text: r.text,
        })
}

/// Run the rule pack and express the findings as advisory suggestions.
pub fn check_as_suggestions(
    doc: &Document,
    corpus: Option<&CorpusDb>,
    only: Option<RuleId>,
) -> SuggestionSet {
    let mut set = SuggestionSet::new();
    for f in check(doc, corpus, only) {
        let cited = match &f.citation {
            Some(c) => format!(" — {}:{} says: {}", c.source, c.paragraph, c.text),
            None => String::new(),
        };
        set.push(
            SuggestionKind::CodeCheck,
            1.0,
            format!(
                "[{}] {} {}: {:.3} {} (minimum {:.3} {})",
                f.severity.as_str(),
                f.rule,
                f.subject,
                f.measured,
                f.units,
                f.threshold,
                f.units
            ),
            format!("rule {} — {}{}", f.rule, f.rule.description(), cited),
            f.targets.clone(),
            // Advisory: how to resolve a code violation is a design decision.
            Action::None,
        );
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Point2, Polyline};
    use aadt_model::Geometry;

    /// A closed rectangular room of `w` × `h` metres.
    fn room_doc(w: f64, h: f64) -> Document {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-AREA",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(w, 0.0),
                    Point2::new(w, h),
                    Point2::new(0.0, h),
                ],
                true,
            )),
        );
        doc
    }

    #[test]
    fn a_compliant_room_produces_no_findings() {
        // 5 × 4 = 20 m², short side 4 m — comfortably over both thresholds.
        let findings = check(&room_doc(5.0, 4.0), None, None);
        assert!(
            findings.is_empty(),
            "expected no findings, got {findings:?}"
        );
    }

    #[test]
    fn an_undersized_room_violates_area_and_width() {
        // 2 × 2 = 4 m² (< 9) and short side 2 m (< 2.4).
        let findings = check(&room_doc(2.0, 2.0), None, None);
        let rules: Vec<RuleId> = findings.iter().map(|f| f.rule).collect();
        assert!(rules.contains(&RuleId::MinRoomArea), "got {rules:?}");
        assert!(rules.contains(&RuleId::MinRoomWidth), "got {rules:?}");
        assert!(findings.iter().all(|f| f.severity == Severity::Violation));
    }

    #[test]
    fn a_marginal_room_warns_rather_than_violates() {
        // Short side 2.5 m: over the 2.4 m minimum but inside the 10% band.
        let findings = check(&room_doc(4.0, 2.5), None, Some(RuleId::MinRoomWidth));
        assert_eq!(findings.len(), 1, "got {findings:?}");
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn only_filter_runs_a_single_rule() {
        let doc = room_doc(2.0, 2.0);
        let findings = check(&doc, None, Some(RuleId::MinRoomArea));
        assert!(findings.iter().all(|f| f.rule == RuleId::MinRoomArea));
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn findings_are_deterministic() {
        let doc = room_doc(2.0, 2.0);
        let a = check(&doc, None, None);
        let b = check(&doc, None, None);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.rule, y.rule);
            assert_eq!(x.subject, y.subject);
            assert!((x.measured - y.measured).abs() < 1e-12);
        }
    }

    #[test]
    fn corpus_supplies_a_citation() {
        let mut corpus = CorpusDb::new();
        corpus.index_text(
            "spec.txt",
            "Room Areas\nEvery habitable room shall have a minimum floor area of 9.5 square metres.\n\n\
             Door Schedule\nDoors shall have a clear opening width of not less than 900 mm.",
        );
        let findings = check(
            &room_doc(2.0, 2.0),
            Some(&corpus),
            Some(RuleId::MinRoomArea),
        );
        assert_eq!(findings.len(), 1);
        let c = findings[0]
            .citation
            .as_ref()
            .expect("a citation should be attached");
        assert_eq!(c.source, "spec.txt");
        assert!(
            c.text.contains("habitable room"),
            "should cite the room-area clause, got: {}",
            c.text
        );
    }

    #[test]
    fn findings_become_advisory_suggestions() {
        let set = check_as_suggestions(&room_doc(2.0, 2.0), None, None);
        assert!(!set.is_empty());
        for s in set.iter() {
            assert_eq!(s.kind, SuggestionKind::CodeCheck);
            assert_eq!(s.confidence, 1.0, "rule checks are deterministic");
            assert!(
                !s.action.is_applicable(),
                "code findings are advisory, never auto-fixed"
            );
        }
    }

    #[test]
    fn rule_tokens_round_trip() {
        for r in RuleId::ALL {
            assert_eq!(RuleId::parse(r.as_str()), Some(r));
        }
        assert_eq!(RuleId::parse("MIN-ROOM-AREA"), Some(RuleId::MinRoomArea));
        assert_eq!(RuleId::parse("nope"), None);
    }
}
