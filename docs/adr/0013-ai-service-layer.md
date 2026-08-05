# ADR-0013 — The AI service layer: propose, never commit

- **Status:** Accepted
- **Supersedes nothing.** Extends the ADR-0010 contract from one service
  (`MAPMETA`) to a general layer, under the ADR-0012 governing principle that
  **AI never owns data**.

## Context

[ADR-0012](0012-ai-native-platform-pivot.md) adopted the AI-native direction with
two rules: *geometry is the source of truth*, and *AI suggests, the database
commits*. Phase 1 built the Object DB and Graph DB; Phase 2 built semantic search
and local retrieval. Both are **read** paths — they observe the drawing and
report. Phase 3 is the first set of services that want to **write**.

That is where the governing principle stops being a slogan and needs a
mechanism. `MAPMETA` (ADR-0010) already had one — suggest → confidence gate →
reconcile → commit → audit — but it was welded into the metadata-mapping engine.
Auto-annotation, code checking, and natural-language input each need the same
guarantees, and re-implementing them three times invites three different sets of
mistakes.

## Decision

Add **`aadt-assist`**, a crate whose services propose and never commit.

### The suggestion is a record, not a call

A service returns `Suggestion { id, kind, confidence, summary, rationale,
targets, action, status }`. The `action` is **data** (`Action::AddDimension(..)`
or `Action::None`), not a closure — so a proposal can be listed, explained, and
audited before anything happens, and an advisory finding is structurally
incapable of mutating anything.

A single function, `apply`, mutates the drawing. It runs inside
`Document::transact` (so every accepted suggestion is one undo step) and writes
an append-only audit row. Services themselves never receive `&mut Document`.

### Rejections are recorded

`reject` changes nothing in the drawing but still writes an audit row with
`approved = false`. A reviewer looking at the trail can tell the difference
between a suggestion that was *considered and declined* and one that was never
raised — which is the question that actually matters in a review.

### Services

| Service | Confidence | Applies? |
|---|---|---|
| `dims` — dimension prediction | derived from detection quality | yes, adds a `Dimension` |
| `rules` — code-check rule packs | always `1.0` (deterministic geometry) | **no** — advisory only |
| `compare` — revision-comparison narration | always `1.0` (fingerprint diff + detection) | **no** — advisory only |
| `nl` — natural-language routing | pattern-match strength | **no** — emits a command *string* |

`compare` wraps the M7 fingerprint diff (`aadt_pdf::compare`) with detection
count deltas and optional BM25 citations for changed layers. It never edits
geometry; `COMPARE <before>` registers one `SuggestionKind::RevisionCompare`
proposal that can be rejected (audited) but not accepted.

**Code findings are deliberately not auto-fixable.** Resolving a code violation
is a design decision — move a wall, re-plan the room, or seek relief. Software
that "fixed" it would be making that call silently.

**Natural language never produces geometry.** `nl::route` maps a phrase to a
command string — the same text a user could type — and the caller decides whether
to run it. There is no path from a sentence to the document that bypasses the
deterministic command band, so NL inherits its validation, undo, and audit
rather than needing its own. When a phrase does not match, the router says so and
offers candidates; it never guesses, because a wrong command silently executed is
worse than no answer.

### Citations, not assertions

Rule thresholds are bundled defaults (NBC 2016 / IS 3809 orders of magnitude).
What the AI layer contributes is the **citation**: with a spec corpus indexed
(Phase 2), each finding is paired with the governing passage, retrieved by BM25.
The tool reports what the project's own spec says and where — it does not assert
what the code requires.

## Consequences

- One audited path to mutation. A new service gets undo, audit, and the
  confidence gate by construction; it cannot opt out, because it never holds a
  mutable document.
- `aadt-assist` depends on `aadt-detect`, `aadt-object` (audit), `aadt-vecdb`
  (citations), and `aadt-pdf` (fingerprint compare for revision narration).
  Layering stays acyclic; the CLI wires the surfaces.
- Suggestion sets are **session-scoped and derived** — regenerated from current
  geometry rather than persisted. The *decisions* persist, in the same
  append-only audit log as every other object mutation.
- Ids are never reused. Re-running a service replaces only its own kind
  (`replace_kind`), so refreshing dimension proposals cannot renumber an
  outstanding code finding out from under a user mid-review.

### Honest limitations

- The NL router recognises a small explicit vocabulary. It is a pattern matcher,
  not a language model, and will refuse far more phrasings than it accepts. That
  is the intended failure mode.
- Dimension prediction proposes one linear dimension per undimensioned wall. It
  does not do dimension *strings*, chains, or sheet layout — a drafter still
  decides what a drawing should be dimensioned to.
- Three rules ship. The pack is a frame for adding more, not a compliance claim;
  passing `CHECK` is not a statement that a drawing is code-compliant.
