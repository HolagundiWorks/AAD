# Phased roadmap — AI-native platform

**Date:** 2026-07-30
**Inputs:** [Vision / guidelines](../vision/ai-native-platform.md) ·
[ADR-0012 (pivot)](../adr/0012-ai-native-platform-pivot.md) ·
[ADR-0010 (embeddings mapping)](../adr/0010-embeddings-metadata-mapping.md) ·
[ADR-0011 (VastuDB)](../adr/0011-vector-database-store.md)

Every phase must pass the four release questions (geometry correct ·
engineering intent accurate · semantically understandable · scales) and keep
`cargo test --workspace` green.

## Where the six stores stand today

| Vision store | Today | Crate(s) | Gap |
|---|---|---|---|
| Geometry DB | ✅ 2D vector records (`.vdb` VastuDB, `.aadt` SQLite) | `aadt-vdb`, `aadt-store`, `aadt-model` | Splines/meshes/solids (Phase 6) |
| Spatial Index | ✅ Uniform-grid bbox index, persisted in `.vdb` | `aadt-vdb` | R-tree option; wire into GUI pick/osnap; perf targets |
| Vector DB | ⚠️ Per-element geometry embeddings + kNN | `aadt-embed`, `aadt-vecdb` | Drawing/text/visual embeddings; PDFs/specs/photos; RAG |
| Object DB | ⚠️ Detection (walls/rooms/openings/structural) is transient | `aadt-detect`, `aadt-model` | Persisted typed objects with engineering properties |
| Metadata DB | ⚠️ DrawingProperties + AIA layer catalog | `aadt-model`, `aadt-standards` | Material libraries, semantic layer defaults, AI tags |
| Graph DB | ❌ Wall graph computed, thrown away | (`aadt-detect` internals) | Persisted relationship store + queries |

Cross-cutting already in place: deterministic core + undo (`aadt-model`,
`aadt-command`); drawing-level history/audit checkpoints (`aadt-store`);
AI-suggests / DB-commits pattern with confidence + reconciliation (`MAPMETA`).

## Phase 1 — Intelligent objects + Graph DB (foundation)

The largest net-new piece; everything later leans on it.

1. **`aadt-object` crate — the Object DB.** Typed engineering objects
   (`Wall`, `Door`, `Window`, `Column`, `Beam`, `Room`, …) with: stable
   `ObjectId`, member `EntityId`s (geometry stays in the Document — source of
   truth), engineering properties (material, thickness, height, fire rating…),
   AI confidence, and revision stamp. Seed objects from `aadt-detect` output
   (promote detection to persisted objects) + `MAPMETA` classifications.
2. **`aadt-graph` crate — the Graph DB.** Persisted typed relationships
   (`Supports`, `BelongsTo`, `ConnectedTo`, `Contains`, `Bounds`) between
   object/entity ids, with forward/reverse queries and traversal. Populate from
   detection (wall adjacency, room containment, opening→wall).
3. **Persistence:** new sections in `.vdb` and additive `.aadt` schema v8
   (objects, properties, relationships). Per-object **audit rows** (who/when/
   prev/new/AI-suggested/approval) extending the ADR-0008 history model.
4. **Surface:** `OBJECTS` / `RELATIONS <id>` commands + LISP queries.

**Verify:** round-trip objects+graph through both stores; detection→object
promotion is deterministic; graph queries match brute force; audit row per
mutation.

## Phase 2 — Semantic engine + retrieval (Vector DB grows up)

1. **Embedding types:** extend `aadt-embed` beyond geometry — text embedding
   (deterministic local model over annotations/metadata), drawing-level
   embedding (aggregate), relationship embedding (graph context). Regenerate on
   approved modification (keyed off `Document::revision`, as today).
2. **Semantic search:** `FIND <text|id>` — cosine search across object/drawing
   embeddings in `aadt-vecdb`; duplicate detection (near-identical clusters).
3. **External corpus (RAG seed):** index sidecar documents (PDF text via
   `aadt-pdf`, specs, standards) into the vector store with source refs;
   `ASKDOCS`-style retrieval returns cited passages. Local-only.
4. **Perf:** meet `<100 ms` search on 100k+ vectors (ANN index if needed).

## Phase 3 — AI service layer (assistive, gated)

All services follow the ADR-0010 contract: suggest → confidence gate →
reconcile → DB commits → audit.

- Auto-annotation + dimension prediction (suggest missing dims from geometry).
- Revision comparison narration (extend `aadt-pdf` compare).
- Standard-detail recommendation (retrieval over Phase-2 corpus).
- Natural-language command routing: NL → the existing deterministic command
  band (`"draw a wall from … to …"` → `POLYLINE`/`WALL`), never NL → geometry.
- Risk/code checks as rule packs (deterministic rules first, IS-code retrieval
  via RAG for citations).

## Phase 4 — Construction intelligence (takeoff returns)

Reinstate quantity extraction on top of **objects** (not raw geometry):
paint/plaster areas, brick/mortar, cost codes, BOQ generation — the M3
machinery redone against the Object DB, with `takeoff_*` tables in `aadt-store`
re-activated. Reports as plugins.

## Phase 5 — Enterprise + APIs

- **API layer:** everything the UI can do, scriptable — extend `aadt-ffi` and
  add a local REST/gRPC service crate (offline drafting never depends on it).
- Enterprise connectors (ERP/BOQ/RFI/document management) as **plugins** over
  that API; site data (photos, reports) ingested into the Vector DB.
- Permissions on objects; signed audit trail.

## Phase 6 — Geometry expansion (re-evaluate)

Splines/meshes/solids, XRefs/paper space — the largest lock reversals
(2D-only). Deliberately last; decide scope when Phases 1–5 have proven the
architecture. May be re-scoped to "2.5D + reinforcement detailing" if the
platform value is already delivered.

## Performance gates (tracked from Phase 1)

Open < 2 s · zoom 60 FPS · selection < 20 ms · snap < 5 ms · search < 100 ms ·
AI suggestion < 2 s · millions of entities. Add benches per phase; the M9.9
frame-time telemetry item becomes the measurement harness.

## Non-negotiables carried through every phase

- Geometry only changes through the deterministic engine; AI suggests, the DB
  commits (with audit).
- Offline drafting keeps working end to end.
- Nothing is deleted; everything is versioned.
- `unsafe_code = forbid`; new heavy dependencies need an ADR.
