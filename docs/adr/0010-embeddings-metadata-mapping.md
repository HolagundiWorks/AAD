# ADR-0010 — Embeddings vector DB → predefined-metadata mapping

- **Status:** Accepted
- **Context:** The drawing is vector geometry, but the only way an element reaches
  its **predefined metadata** — an AIA/NCS standard layer (`aadt-standards`
  `AIA_LAYERS`) and a model-space [`SpaceCategory`] (Walls / Structural / Lintel /
  BeamLayout / Furniture, ADR-0009) — was a **layer-name string heuristic**
  (`SpaceCategory::from_layer_name`, `Document::category_for_layer`,
  `assign_imported_by_layer`). Nothing mapped an element by *what its geometry
  is*, so imported or loose geometry on `0`/non-standard layers stayed
  unclassified. The ask: **store the drawing as a vector database** and use it to
  map each element to that metadata.

## Decision

Add an **embeddings vector database** over drawing elements and a
**confidence-gated mapping engine** that assigns the predefined metadata from
geometry.

- **`aadt-embed`** — a small, local **ML model** (an MLP classifier) trained
  deterministically at first use on seeded synthetic exemplars. It turns each
  element into a fixed geometric feature vector, predicts one of a curated set of
  classes (Wall, Column, Beam, Lintel, Door, Glazing, Furniture, Equipment,
  Stair, Annotation), and emits an L2-normalized embedding + confidence. Each
  class carries its **(AIA layer, SpaceCategory)** so one prediction yields both.
- **`aadt-vecdb`** — the custom vector database: `(EntityId, embedding, layer,
  category, confidence, status)` with cosine k-NN, plus the mapping engine. Above
  a confidence threshold an element is **auto-assigned** to its layer + category
  (undoable, one transaction); below it the element is left **pending** for
  reconciliation (`Accept` / `Set`). Manual (`Confirmed`/`Overridden`) decisions
  are preserved and re-applied on later passes.
- **Persistence (`.aadt` schema v7).** Two additive tables — `vec_meta` and
  `vec_embedding` — store each element's vector plus its predicted metadata and
  reconciliation status, so the drawing is literally *stored as a vector
  database*. Embeddings are a recomputable cache; the reconciliation status is
  the durable state. A plain `SAVE` carries the tables forward.
- **Surfaces.** `MAPMETA` (aliases `CLASSIFY`/`VECTORDB`/`VDB`) command:
  `[ALL] | RECONCILE | ACCEPT id | SET id <class|layer> | INFO id`; LISP
  `(aadt-classify id)` / `(aadt-map-metadata)`; an egui reconcile panel.

### Deliberate product deviation (ROADMAP M4 / §35)

M4 removed all AI and §35 lists "auto layer assignment" as an unscheduled future
extension. This ADR **knowingly reintroduces a local ML model** to implement that
extension. It is bounded to stay within the product locks:

- **Local + offline + deterministic at inference.** No network, no background
  compute; fixed seeded training → the same drawing yields the same predictions.
- **No new runtime dependencies.** Pure-Rust `f32` inference (std only); the
  workspace's `unsafe_code = "forbid"` is preserved.
- **Advisory, never silent.** Only high-confidence predictions auto-apply;
  uncertain elements wait in reconciliation rather than being mis-filed. Every
  change is undoable.

## Consequences

- Geometry-based auto-classification replaces name-only heuristics on import and
  on demand; loose geometry can be filed onto standard layers/categories.
- `aadt-store` gains an `aadt-vecdb` dependency; `aadt-lisp` gains `aadt-embed` /
  `aadt-vecdb`. Layering stays acyclic
  (`aadt-geometry ← aadt-model ← {aadt-standards, aadt-embed} ← aadt-vecdb`).
- The bundled model is small and trained on synthetic data: similar-looking
  classes (wall/beam/glazing/lintel) are weakly separated and often land in
  reconciliation. The model is a swappable component (`Classifier` trait) and can
  be retrained without changing the DB or the command surface.

## Alternatives considered

- **Deterministic hand-tuned feature scoring (no model).** Explainable and
  dependency-free, but the request specified a learned model; kept as the
  `Classifier` trait's fallback baseline.
- **Bundled pretrained weights via `include_bytes!` / a build-script trainer.**
  Rejected for train-at-first-use to avoid a committed binary asset and
  build-script fragility while keeping determinism.
- **A separate on-disk vector-store format.** Rejected — reusing SQLite `.aadt`
  (ADR-0004) keeps one file, crash recovery, and the existing save path.

[`SpaceCategory`]: ../../crates/aadt-model/src/floors.rs
