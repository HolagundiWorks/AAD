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
| Vector DB | ✅ Geometry + text + aggregate embeddings, kNN, BM25 corpus (M11) | `aadt-embed`, `aadt-vecdb` | PDF/photo ingestion; relationship embeddings; ANN index for perf |
| Object DB | ✅ Typed objects w/ engineering props + append-only audit (M10) | `aadt-object` | Editable props UI; MAPMETA-driven objects for undetected geometry |
| Metadata DB | ⚠️ DrawingProperties + AIA layer catalog | `aadt-model`, `aadt-standards` | Material libraries, semantic layer defaults, AI tags |
| Graph DB | ✅ Persisted typed relationships + promotion (M10) | `aadt-graph` | `Supports` population (needs slabs); constraint-aware edges |

Cross-cutting already in place: deterministic core + undo (`aadt-model`,
`aadt-command`); drawing-level history/audit checkpoints (`aadt-store`);
AI-suggests / DB-commits pattern with confidence + reconciliation (`MAPMETA`).

## Phase 1 — Intelligent objects + Graph DB (foundation) — ✅ core shipped as **M10**

The largest net-new piece; everything later leans on it. Tracked as
[ROADMAP M10](../ROADMAP.md#m10--ai-native-phase-1-intelligent-objects--graph-db-).

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

## Phase 2 — Semantic engine + retrieval (Vector DB grows up) — ✅ core shipped as **M11**

Tracked as [ROADMAP M11](../ROADMAP.md#m11--ai-native-phase-2-semantic-search--local-retrieval-).

1. **Embedding types:** ✅ `aadt-embed` gained `text_embedding` (64-dim character
   n-gram hashing — no training, no dependencies) and `drawing_embedding` /
   `object_embedding` (mean-pooled aggregates). Relationship (graph-context)
   embeddings remain open.
2. **Semantic search:** ✅ `FIND <text>` over element labels, `FIND SIMILAR id`
   over the geometric embedding space, and `FIND DUPES` for congruent geometry.
   `FIND` sets the selection, so hits feed the next command.
3. **External corpus (RAG seed):** ✅ `ASKDOCS INDEX <path>` chunks a text
   document per paragraph; `ASKDOCS <question>` returns passages with source and
   paragraph number. Ranking is **BM25** over word tokens with an n-gram cosine
   fallback. Local-only. PDF ingestion via `aadt-pdf` is still open — today the
   corpus takes plain text/Markdown.
4. **Perf:** open. Corpus search is O(n·|q|) per query and duplicate detection is
   O(n²); both are fine at drawing scale but neither is the ANN index the
   `<100 ms` on 100k+ vectors gate will need.

**Design note — why BM25 rather than embedding cosine for the corpus.** The
character n-gram vector is 64 hashed buckets. Paragraph-length text fills nearly
every bucket, so all passages look alike (measured: every candidate scored
84–87%, and the correct answer ranked *third*). IDF re-weighting cannot fix this
because the buckets are collided hash bins, not terms — the weights come out
near-uniform and cancel. BM25 over word tokens scores on term rarity and ranks
the right passage first by a wide margin. The n-gram model is kept where it does
work well: short labels, where `FIND wall` cleanly separates walls (75%) from
columns (9%).

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
