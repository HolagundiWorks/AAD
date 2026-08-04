# ShilpiDB, layers, and AI agents

How the HCW layer standard turns a drawing stored in **ShilpiDB** (the `.vdb`
vector database, [ADR-0011](../adr/0011-vector-database-store.md)) into a
queryable, agent-actionable model. The layer is the primary semantic key: it
carries discipline, element type, IFC class, and AI tags (see
[hcw-layer-standard.md](./hcw-layer-standard.md)), so an agent can reason about
**what** an item is — not just where it is — without re-deriving meaning from raw
geometry.

## 1. Why the layer is the join key

Every entity in a `.vdb` record stores its **layer name** and **attributes**
alongside the geometry and the spatial-grid bucket. Because layer names follow
the HCW convention, the layer alone yields:

| From the layer | Via | Used for |
|----------------|-----|----------|
| Discipline (`A/S/C/M/E/…`) | prefix / `Discipline` | filtering, layer sets, routing to the right agent |
| Element type | `ElementType` | IFC class, Uniformat/MasterFormat, AI tags |
| IFC class | `ElementType::ifc()` | BIM export, clash rules |
| AI tags | `ElementType::ai_tags()` | semantic search, recognition priors |
| Phase | `StandardLayer::phase` | existing / new / demolition scoping |

Per-object `OBJMETA` overrides (material, fire rating, phase) refine this where a
single object differs from its layer's default. The layer gives the **default
semantics for free**; the attribute map records the **exceptions**.

## 2. Query surfaces an agent can use

An agent composes three independent selectors, then acts on the result set:

1. **Spatial** — `WITHIN x1 y1 x2 y2` (aliases `VSELECT` / `VWINDOW`), backed by
   the persisted spatial grid (`SpatialGrid::query_bbox` / `query_point`). Answers
   *"what is in this region / room / grid bay?"* in `O(cells + hits)`.
2. **Semantic (layer / discipline / element)** — filter the spatial result by
   layer group. The **layer set** is the coarse switch (see §3); the discipline
   prefix and `ElementType` are the fine ones. Answers *"…that are windows"* or
   *"…on structural layers."*
3. **Similarity (embeddings)** — `aadt-vecdb` holds a per-entity embedding; `FIND`
   / `find_by_text` / `find_similar_to` rank entities by cosine distance, and
   `nearest_class` maps an embedding to an `ElementClass`. Answers *"…that look
   like this detail"* or *"unlabelled items most similar to a known door."*

Layer semantics and embeddings are **complementary**: the layer is the
authoritative label when present; the embedding recovers meaning for imported or
mislabelled geometry, which `MAPMETA` can then reconcile back onto a correct HCW
layer (`set_layer` / `set_class`).

## 3. Acting *based on layer set*

A **layer set** (`CONSTRUCTION`, `STRUCTURAL`, `CIVIL`, `SERVICES`, `ELECTRICAL`,
`PLUMBING`, `FIRE`, `ELV`, `LANDSCAPE`, `INTERIOR`, `DEMOLITION`, `PRESENTATION`,
`IMPORT`) is a named view over disciplines — which are shown at full strength and
which stay as a thin underlay. For an agent it is a **scope contract**: it names
the disciplines the current task is allowed to see and touch.

Typical agent loop, scoped by layer set:

```
LAYERSET STRUCTURAL          # scope: S-* primary, A-* underlay
SELECT <discipline/element>  # e.g. all S-COLUMN in a grid bay (spatial ∩ layer)
OBJMETA MATERIAL C40         # act: annotate, or feed takeoff / clash / export
```

Recommended agent conventions:

- **Read the set as permissions.** Under `PLUMBING`, an agent edits `P-*` and
  reads `A-*`/`S-*` as context; it must not silently modify architecture. The
  underlay is *reference*, not a work surface.
- **Prefer the layer's declared semantics.** Route by discipline (`Discipline`),
  match elements by `ElementType`, export by `ifc()` — never by parsing text or
  colour. Colour is ByLayer and plots black; it is not a semantic signal.
- **Fall back to embeddings only for unlabelled geometry.** Freshly imported
  `ORG-DWG_LAYER-*` layers (the `IMPORT` set) carry no HCW semantics yet; use
  similarity + `nearest_class`, then write the resolved HCW layer with
  `MAPMETA` / `set_layer` so future queries are cheap and deterministic.
- **Respect phase.** Scope demolition work to `X-*` / `D-*` (or the `DEMOLITION`
  set); check `OBJMETA` phase before counting an item as new construction.

## 4. Worked examples

| Task | Composition |
|------|-------------|
| Quantify glazing on the ground floor | `WITHIN` floor bbox → keep `Window` element (`A-WINDOW`/`A-GLAZ`) → sum → BoQ under MasterFormat `08` |
| Structural clash check in a bay | `LAYERSET STRUCTURAL` → `WITHIN` bay → keep `S-*` beams/columns → pairwise test |
| Tag unlabelled imported walls | `LAYERSET IMPORT` → embeddings `find_similar_to` a known `A-WALL` → `MAPMETA` to `A-WALL` |
| Fire-rated wall schedule | select `Wall` elements → `OBJMETA` report → group by `FIRERATING` |
| Export MEP to IFC | union of `SERVICES` + `ELECTRICAL` sets → map each `ElementType::ifc()` → write |

## 5. Determinism and safety

- Queries and write ordering are reproducible; writes go through
  `Document::transact` (undoable), including `OBJMETA` and `set_layer`.
- The `.vdb` format, the embedded `aadt-vdb` API, and every command are unchanged
  by this guidance — it is a **usage contract**, not new storage.
- An agent should treat the active layer set as its blast radius: enumerate the
  set's disciplines, act only within them, and record provenance (which layer /
  which override it changed) so a human can audit and undo.

## Reference

- Layer standard & metadata schema: [hcw-layer-standard.md](./hcw-layer-standard.md)
- Vector store: [ADR-0011](../adr/0011-vector-database-store.md)
- Standalone server design: [shilpidb-standalone-server.md](../plans/shilpidb-standalone-server.md)
- Commands: `WITHIN` / `VSELECT`, `LAYERSET`, `OBJMETA`, `LAYERREPORT`,
  `LAYERLEGEND`, `MAPMETA`, `FIND`.
