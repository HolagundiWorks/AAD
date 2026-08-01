//! # aadt-assist — the AI service layer (AI-native Phase 3, ADR-0013)
//!
//! Every service here **proposes**; none of them commit. A service inspects the
//! drawing, emits [`Suggestion`]s with a confidence score and a written
//! rationale, and stops. Applying a suggestion is a separate, explicit step that
//! runs inside the document's undo transaction and writes an audit row.
//!
//! That is the ADR-0010 contract generalised: *suggest → confidence gate →
//! reconcile → DB commits → audit*. It is what "AI never owns data" means in
//! practice — the model's output is a proposal record, and the only thing that
//! ever mutates the drawing is [`apply`], driven by a human decision.
//!
//! Three services ship in Phase 3:
//! - [`dims`] — dimension prediction: walls carrying no dimension.
//! - [`rules`] — deterministic code-check rule packs, optionally citing an
//!   indexed spec corpus.
//! - [`nl`] — natural-language command routing: text in, a *command string* out,
//!   never geometry. The proposed command runs through the existing
//!   deterministic interpreter or not at all.
//!
//! Everything is local, offline, and deterministic: the same drawing yields the
//! same suggestions in the same order.

pub mod dims;
pub mod nl;
pub mod rules;

use aadt_model::{Document, EntityId};
use aadt_object::AuditLog;

pub use dims::suggest_dimensions;
pub use nl::{route, Routed};
pub use rules::{check, Finding, RuleId, Severity};

/// Which service produced a suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SuggestionKind {
    /// A missing dimension annotation ([`dims`]).
    Dimension,
    /// A code / standards rule finding ([`rules`]).
    CodeCheck,
}

impl SuggestionKind {
    /// Stable token used for display and audit rows.
    pub fn as_str(self) -> &'static str {
        match self {
            SuggestionKind::Dimension => "dimension",
            SuggestionKind::CodeCheck => "code-check",
        }
    }
}

impl std::fmt::Display for SuggestionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where a suggestion stands. A suggestion starts [`Pending`](Self::Pending) and
/// only ever moves on a human decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionStatus {
    /// Proposed, awaiting a decision. Nothing has been changed.
    Pending,
    /// A human accepted it and the change was applied.
    Accepted,
    /// A human rejected it. Nothing was changed.
    Rejected,
}

impl SuggestionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SuggestionStatus::Pending => "pending",
            SuggestionStatus::Accepted => "accepted",
            SuggestionStatus::Rejected => "rejected",
        }
    }
}

/// What accepting a suggestion would actually do.
///
/// An action is *data*, not a closure: it can be inspected, displayed, and
/// audited before anything happens. [`Action::None`] covers advisory findings
/// that have no automatic fix — a code violation is reported, not silently
/// "corrected".
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Add this dimension to the drawing.
    AddDimension(Box<aadt_model::Dimension>),
    /// Advisory only: there is nothing to apply.
    None,
}

impl Action {
    /// Whether accepting this suggestion would change the drawing.
    pub fn is_applicable(&self) -> bool {
        !matches!(self, Action::None)
    }
}

/// One proposal from a service.
#[derive(Debug, Clone)]
pub struct Suggestion {
    /// Stable 1-based index within the [`SuggestionSet`].
    pub id: u32,
    pub kind: SuggestionKind,
    /// Confidence in `[0, 1]`. Deterministic rule findings are `1.0`; predicted
    /// annotations carry the predictor's own score.
    pub confidence: f32,
    /// One-line description of the proposal.
    pub summary: String,
    /// Why the service proposed it — shown to the human making the call.
    pub rationale: String,
    /// Entities the suggestion concerns (for selection/highlighting).
    pub targets: Vec<EntityId>,
    /// What accepting would do.
    pub action: Action,
    pub status: SuggestionStatus,
}

impl Suggestion {
    /// A one-line audit summary of this suggestion.
    pub fn summarize(&self) -> String {
        format!("{} conf={:.2} {}", self.kind, self.confidence, self.summary)
    }
}

/// A deterministic, ordered set of proposals.
///
/// Ids are assigned on insertion and never reused, so `SUGGEST ACCEPT 3` refers
/// to the same proposal for as long as the set lives.
#[derive(Debug, Clone, Default)]
pub struct SuggestionSet {
    items: Vec<Suggestion>,
    next_id: u32,
}

impl SuggestionSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a proposal, assigning it the next id. Returns that id.
    pub fn push(
        &mut self,
        kind: SuggestionKind,
        confidence: f32,
        summary: impl Into<String>,
        rationale: impl Into<String>,
        targets: Vec<EntityId>,
        action: Action,
    ) -> u32 {
        self.next_id += 1;
        self.items.push(Suggestion {
            id: self.next_id,
            kind,
            confidence: confidence.clamp(0.0, 1.0),
            summary: summary.into(),
            rationale: rationale.into(),
            targets,
            action,
            status: SuggestionStatus::Pending,
        });
        self.next_id
    }

    /// Merge another set in, renumbering its items to continue this set's ids.
    pub fn extend(&mut self, other: SuggestionSet) {
        for s in other.items {
            self.push(
                s.kind,
                s.confidence,
                s.summary,
                s.rationale,
                s.targets,
                s.action,
            );
        }
    }

    /// Replace every proposal of `kind` with `incoming`, leaving other kinds
    /// untouched.
    ///
    /// This is what lets two services share one set: re-running the dimension
    /// predictor refreshes its own proposals without discarding outstanding code
    /// findings, and vice versa. Ids are never reused, so an id a user has
    /// already seen either still means the same proposal or no longer exists —
    /// it never silently becomes a *different* one.
    pub fn replace_kind(&mut self, kind: SuggestionKind, incoming: SuggestionSet) {
        self.items.retain(|s| s.kind != kind);
        self.extend(incoming);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// All proposals in id order.
    pub fn iter(&self) -> impl Iterator<Item = &Suggestion> {
        self.items.iter()
    }

    pub fn get(&self, id: u32) -> Option<&Suggestion> {
        self.items.iter().find(|s| s.id == id)
    }

    /// Proposals still awaiting a decision, in id order.
    pub fn pending(&self) -> Vec<&Suggestion> {
        self.items
            .iter()
            .filter(|s| s.status == SuggestionStatus::Pending)
            .collect()
    }

    /// Count per kind, in stable order (zero counts omitted).
    pub fn kind_counts(&self) -> Vec<(SuggestionKind, usize)> {
        [SuggestionKind::Dimension, SuggestionKind::CodeCheck]
            .into_iter()
            .filter_map(|k| {
                let n = self.items.iter().filter(|s| s.kind == k).count();
                (n > 0).then_some((k, n))
            })
            .collect()
    }
}

/// Why an accept/reject call could not be honoured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    /// No proposal carries that id.
    NoSuchSuggestion(u32),
    /// The proposal was already accepted or rejected.
    AlreadyDecided(u32, &'static str),
    /// The proposal is advisory — there is nothing to apply.
    NotApplicable(u32),
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyError::NoSuchSuggestion(id) => write!(f, "no suggestion #{id}"),
            ApplyError::AlreadyDecided(id, st) => write!(f, "suggestion #{id} is already {st}"),
            ApplyError::NotApplicable(id) => write!(
                f,
                "suggestion #{id} is advisory — there is nothing to apply automatically"
            ),
        }
    }
}

impl std::error::Error for ApplyError {}

/// Accept a proposal: apply its action inside one undoable transaction, mark it
/// accepted, and record an audit row.
///
/// This is the **only** function in the crate that mutates the drawing, and it
/// runs only when called explicitly. A service producing a suggestion never
/// reaches the document.
pub fn apply(
    set: &mut SuggestionSet,
    doc: &mut Document,
    audit: &mut AuditLog,
    id: u32,
) -> Result<String, ApplyError> {
    let Some(idx) = set.items.iter().position(|s| s.id == id) else {
        return Err(ApplyError::NoSuchSuggestion(id));
    };
    if set.items[idx].status != SuggestionStatus::Pending {
        return Err(ApplyError::AlreadyDecided(
            id,
            set.items[idx].status.as_str(),
        ));
    }
    if !set.items[idx].action.is_applicable() {
        return Err(ApplyError::NotApplicable(id));
    }

    let action = set.items[idx].action.clone();
    let summary = set.items[idx].summarize();
    match action {
        Action::AddDimension(dim) => {
            doc.transact("SUGGEST accept (dimension)", |d| {
                d.add_dimension(*dim);
            });
        }
        Action::None => unreachable!("guarded by is_applicable above"),
    }

    set.items[idx].status = SuggestionStatus::Accepted;
    // Suggestions are drawing-level, not object-level, so the audit row is
    // anchored to the null object id; the summary carries the detail.
    audit.record(
        "suggest-accept",
        aadt_object::ObjectId(0),
        "",
        &summary,
        true,
        true,
        "human accepted",
    );
    Ok(summary)
}

/// Reject a proposal: mark it rejected and audit the decision. Nothing in the
/// drawing changes — recording the *refusal* is the point, so a later reviewer
/// can see the suggestion was considered rather than missed.
pub fn reject(
    set: &mut SuggestionSet,
    audit: &mut AuditLog,
    id: u32,
) -> Result<String, ApplyError> {
    let Some(idx) = set.items.iter().position(|s| s.id == id) else {
        return Err(ApplyError::NoSuchSuggestion(id));
    };
    if set.items[idx].status != SuggestionStatus::Pending {
        return Err(ApplyError::AlreadyDecided(
            id,
            set.items[idx].status.as_str(),
        ));
    }
    set.items[idx].status = SuggestionStatus::Rejected;
    let summary = set.items[idx].summarize();
    audit.record(
        "suggest-reject",
        aadt_object::ObjectId(0),
        &summary,
        "",
        true,
        false,
        "human rejected",
    );
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::Point2;
    use aadt_model::{Dimension, DimensionKind, ModelSpaceId};

    fn dim() -> Dimension {
        Dimension {
            layer: "A-ANNO-DIMS".into(),
            space: ModelSpaceId(0),
            kind: DimensionKind::Linear,
            p1: Point2::new(0.0, 0.0),
            p2: Point2::new(5.0, 0.0),
            vertex: Point2::new(0.0, 0.0),
            text_anchor: Point2::new(2.5, 0.5),
            text: String::new(),
            measurement: 5.0,
        }
    }

    fn set_with_one() -> SuggestionSet {
        let mut set = SuggestionSet::new();
        set.push(
            SuggestionKind::Dimension,
            0.9,
            "dimension a 5.000 m wall",
            "the wall carries no dimension",
            vec![EntityId(1)],
            Action::AddDimension(Box::new(dim())),
        );
        set
    }

    #[test]
    fn ids_are_sequential_and_stable() {
        let mut set = SuggestionSet::new();
        let a = set.push(
            SuggestionKind::Dimension,
            1.0,
            "a",
            "r",
            vec![],
            Action::None,
        );
        let b = set.push(
            SuggestionKind::CodeCheck,
            1.0,
            "b",
            "r",
            vec![],
            Action::None,
        );
        assert_eq!((a, b), (1, 2));
        assert_eq!(set.get(1).unwrap().summary, "a");
        assert_eq!(set.len(), 2);
        assert_eq!(
            set.kind_counts(),
            vec![
                (SuggestionKind::Dimension, 1),
                (SuggestionKind::CodeCheck, 1)
            ]
        );
    }

    #[test]
    fn accepting_applies_once_and_audits() {
        let mut set = set_with_one();
        let mut doc = Document::new();
        let mut audit = AuditLog::new();
        assert_eq!(doc.dimensions().len(), 0);

        apply(&mut set, &mut doc, &mut audit, 1).unwrap();
        assert_eq!(doc.dimensions().len(), 1, "the dimension was added");
        assert_eq!(set.get(1).unwrap().status, SuggestionStatus::Accepted);
        assert_eq!(audit.len(), 1);
        assert_eq!(audit.entries()[0].action, "suggest-accept");
        assert!(audit.entries()[0].ai_suggested);
        assert!(audit.entries()[0].approved);

        // A second accept is refused rather than applied twice.
        let err = apply(&mut set, &mut doc, &mut audit, 1).unwrap_err();
        assert_eq!(err, ApplyError::AlreadyDecided(1, "accepted"));
        assert_eq!(doc.dimensions().len(), 1, "not applied twice");
    }

    #[test]
    fn accepting_is_undoable_as_one_step() {
        let mut set = set_with_one();
        let mut doc = Document::new();
        let mut audit = AuditLog::new();
        apply(&mut set, &mut doc, &mut audit, 1).unwrap();
        assert_eq!(doc.dimensions().len(), 1);
        doc.undo();
        assert_eq!(doc.dimensions().len(), 0, "undo removes the dimension");
    }

    #[test]
    fn rejecting_changes_nothing_but_is_recorded() {
        let mut set = set_with_one();
        let doc = Document::new();
        let mut audit = AuditLog::new();
        reject(&mut set, &mut audit, 1).unwrap();
        assert_eq!(doc.dimensions().len(), 0, "nothing applied");
        assert_eq!(set.get(1).unwrap().status, SuggestionStatus::Rejected);
        assert_eq!(audit.len(), 1);
        assert_eq!(audit.entries()[0].action, "suggest-reject");
        assert!(
            !audit.entries()[0].approved,
            "a rejection is recorded as not approved"
        );
    }

    #[test]
    fn advisory_suggestions_cannot_be_applied() {
        let mut set = SuggestionSet::new();
        set.push(
            SuggestionKind::CodeCheck,
            1.0,
            "room below minimum area",
            "7.2 m² < 9.0 m²",
            vec![EntityId(3)],
            Action::None,
        );
        let mut doc = Document::new();
        let mut audit = AuditLog::new();
        let err = apply(&mut set, &mut doc, &mut audit, 1).unwrap_err();
        assert_eq!(err, ApplyError::NotApplicable(1));
        assert_eq!(set.get(1).unwrap().status, SuggestionStatus::Pending);
        assert!(audit.is_empty(), "a refused apply writes no audit row");
    }

    #[test]
    fn unknown_id_is_an_error() {
        let mut set = set_with_one();
        let mut doc = Document::new();
        let mut audit = AuditLog::new();
        assert_eq!(
            apply(&mut set, &mut doc, &mut audit, 99).unwrap_err(),
            ApplyError::NoSuchSuggestion(99)
        );
    }

    #[test]
    fn extend_renumbers_without_collision() {
        let mut a = set_with_one();
        let b = set_with_one();
        a.extend(b);
        assert_eq!(a.len(), 2);
        let ids: Vec<u32> = a.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![1, 2], "merged ids stay unique and ordered");
    }

    #[test]
    fn replace_kind_refreshes_one_service_without_touching_the_other() {
        let mut set = SuggestionSet::new();
        set.push(
            SuggestionKind::Dimension,
            0.9,
            "dim A",
            "r",
            vec![],
            Action::None,
        );
        set.push(
            SuggestionKind::CodeCheck,
            1.0,
            "finding A",
            "r",
            vec![],
            Action::None,
        );

        let mut fresh = SuggestionSet::new();
        fresh.push(
            SuggestionKind::Dimension,
            0.8,
            "dim B",
            "r",
            vec![],
            Action::None,
        );
        set.replace_kind(SuggestionKind::Dimension, fresh);

        let summaries: Vec<&str> = set.iter().map(|s| s.summary.as_str()).collect();
        assert!(
            summaries.contains(&"finding A"),
            "the code finding must survive, got {summaries:?}"
        );
        assert!(summaries.contains(&"dim B"), "got {summaries:?}");
        assert!(!summaries.contains(&"dim A"), "stale dimension replaced");

        // Ids are never reused: the new proposal gets a fresh one.
        let ids: Vec<u32> = set.iter().map(|s| s.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&3), "new item took a fresh id, got {ids:?}");
    }

    #[test]
    fn pending_tracks_decisions() {
        let mut set = set_with_one();
        let mut audit = AuditLog::new();
        assert_eq!(set.pending().len(), 1);
        reject(&mut set, &mut audit, 1).unwrap();
        assert!(set.pending().is_empty());
    }
}
