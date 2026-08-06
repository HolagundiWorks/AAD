# ADR-0012 — AI-native platform pivot

- **Status:** Accepted (direction); implementation phased — see
  [the phased roadmap](../plans/2026-07-ai-native-phases.md)
- **Context:** The product owner adopted the
  [AI-Native CAD Platform guidelines](../vision/ai-native-platform.md) (2026-07)
  as the long-term direction: an engineering-intelligence platform on a
  multi-layer data architecture (geometry / object / metadata / vector / graph /
  spatial-index stores), semantic objects with embeddings + relationships +
  audit, an AI service layer, enterprise integration, and an automation-first
  API surface. Parts of that vision already exist here: the deterministic CAD
  core, ShilpiDB (`aadt-vdb`, geometry store + spatial index, ADR-0011), the
  embeddings vector DB + confidence-gated auto-classification
  (`aadt-embed`/`aadt-vecdb`, `MAPMETA`, ADR-0010), wall/room detection
  (`aadt-detect`), AIA semantic layers (`aadt-standards`), and per-drawing
  history/audit checkpoints (`aadt-store`, ADR-0008).

## Decision

Adopt the AI-native platform guidelines as the **north star**, implemented in
phases that each preserve the four release questions (geometry correct ·
engineering intent accurate · semantically understandable · scales to millions
of objects).

Two principles from the vision are **already law** in this codebase and remain
non-negotiable:

1. **Geometry is the source of truth; the CAD engine stays deterministic.**
   AI proposes; the geometry engine / database validates and commits
   (exactly the `MAPMETA` confidence-gate + reconciliation pattern of ADR-0010).
2. **AI never owns data.** Every AI-suggested change is committed by the
   database layer, undoable, and auditable.

### Product locks carried forward (explicit)

| ROADMAP lock | How the AI-native phases relate |
|---|---|
| Assist proposes, never auto-commits | Phases 3+ ship local assist / retrieval (ADR-0010 / ADR-0013) |
| Object-based takeoff | Phase 4 measures typed objects, not dumb geometry |
| No cloud collab product; offline-first | Phase 5 is local API + connectors — drafting never requires the server |
| **2D CAD only — permanent** | No solids / meshes / BIM authoring / IFC exchange phase |
| Paper space / XREF out of scope | Nameplate PDF / sheets remain the documentation path |

### What does *not* change

- The deterministic spine (`aadt-geometry → aadt-model → aadt-detect`) and
  atomic undo/redo.
- Offline-capable drafting: enterprise features are additive, never a
  precondition for drawing.
- `unsafe_code = forbid`, dependency frugality, and the curated command band
  for daily drafting (platform capabilities arrive as services/plugins, not
  command sprawl).
- **2D plan CAD** — wall height / volume stay attributes; there is no 3D scene.

## Consequences

- The multi-layer data architecture maps onto existing crates and grows from
  them (see the phased roadmap): ShilpiDB is the seed of the Geometry DB +
  Spatial Index; `aadt-vecdb`/`aadt-embed` seed the Vector DB and semantic
  engine; `aadt-detect` + `aadt-standards` seed the Object DB and semantic
  layers; `aadt-store` history seeds the audit/versioning layer. A Graph DB
  (persisted relationships) is the largest net-new store.
- Each phase lands with its own ADR when it makes irreversible format or
  dependency choices; this ADR only fixes the direction and the supersession
  rules.
- README/ROADMAP present the shipping 2D drafting product; the vision doc
  presents the destination. Divergence between the two is expected and managed
  through the phased roadmap.

## Alternatives considered

- **Reject the vision, keep the lean 2D-drafting-only locks.** The owner chose
  the platform direction; the locks were working constraints, not identity.
- **Adopt silently, feature by feature.** Rejected: reversing recorded product
  locks without a decision record would make the ROADMAP/ADR history
  unreliable.
- **Big-bang re-architecture.** Rejected: the four release questions demand
  the deterministic core keep shipping; phases keep every increment green.
