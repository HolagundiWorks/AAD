# ADR-0007 — Propose → Review → Freeze (classification)

> ⛔ **Superseded (2026-07): the AI classification workflow was removed** along
> with the AI layer — RECOGNIZE/REVIEW/ACCEPT/REJECT/FREEZE and wall-type
> inference no longer exist. Retained as historical record.

- **Status:** Superseded — AI classification removed (was: Accepted, walls-first, 2026-07)
- **Extends:** [ADR-0003](0003-bundled-ai-model.md)
- **Context:** AI wall typing previously wrote `AADT-WALL-TYPE` with no confidence,
  status, or drafter gate. That invited silent “rewrites” of meaning even when
  geometry stayed untouched. Spec principle: AI assists; the drafter owns the
  contractual classification.

## Decision

Classification is a three-phase workflow on attributes only (no geometry edits):

1. **Propose** (`RECOGNIZE` / `aadt infer`) — detect walls, infer types, write
   `AADT-WALL-TYPE`, `AADT-CLASS-CONF`, `AADT-CLASS-STATUS`, `AADT-CLASS-REV`,
   `AADT-CLASS-SOURCE`. Confidence ≥ **0.98** → status `accepted`; otherwise
   `proposed`. Entities already `frozen` are skipped.
2. **Review** (`REVIEW`, CLI/session; GUI panel tracked) — list proposed /
   low-confidence rows; `ACCEPT` / `REJECT` / type override (`SOURCE=user`,
   rev bump). An egui AI panel is planned (see industry audit gap G4) but is
   **not** required for the attribute workflow to work.
3. **Freeze** (`FREEZE`) — explicit lock to `frozen`. Freeze is contractual:
   re-infer never overwrites frozen types.

Takeoff remains deterministic: quantities stay; rows may carry `wall_type` /
`class_status` (unfrozen / missing → `unclassified` for BOQ discipline).

## Consequences

- Geometry draft loop unchanged; AI never mutates coordinates.
- CLI/session share the same commands and attribute schema; GUI should call the
  same commands when the review panel lands.
- Multi-object catalogs, OCR, and online learning from overrides are out of scope
  for this slice (override pairs may be audited later).

## Alternatives considered

- **Immediate apply** (legacy): fastest, but no review trail — rejected for
  production takeoff discipline.
- **Auto-freeze on high confidence:** convenient, but freeze must stay an explicit
  user action so BOQ lock is intentional.
