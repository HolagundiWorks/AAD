# Phased roadmap — AI-native platform

**Date:** 2026-07-30
**Inputs:** [Vision / guidelines](../vision/ai-native-platform.md) ·
[ADR-0012 (pivot)](../adr/0012-ai-native-platform-pivot.md) ·
[ADR-0010 (embeddings mapping)](../adr/0010-embeddings-metadata-mapping.md) ·
[ADR-0011 (ShilpiDB)](../adr/0011-vector-database-store.md)

Every phase must pass the four release questions (geometry correct ·
engineering intent accurate · semantically understandable · scales) and keep
`cargo test --workspace` green.

## Where the six stores stand today

| Vision store | Today | Crate(s) | Gap |
|---|---|---|---|
| Geometry DB | ✅ 2D vector records (`.vdb` ShilpiDB, `.aadt` SQLite) | `aadt-vdb`, `aadt-store`, `aadt-model` | Splines/meshes/solids (Phase 6) |
| Spatial Index | ✅ Uniform-grid bbox index, persisted in `.vdb` | `aadt-vdb` | R-tree option; wire into GUI pick/osnap; perf targets |
| Vector DB | ✅ Geometry + text + aggregate embeddings, kNN, BM25 corpus (M11) | `aadt-embed`, `aadt-vecdb` | PDF/photo ingestion; relationship embeddings; ANN index for perf |
| Object DB | ✅ Typed objects w/ engineering props + append-only audit (M10); measured for takeoff (M13) | `aadt-object`, `aadt-takeoff` | Editable props UI; MAPMETA-driven objects for undetected geometry |
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

## Phase 3 — AI service layer (assistive, gated) — ✅ core shipped as **M12**

Tracked as [ROADMAP M12](../ROADMAP.md#m12--ai-native-phase-3-the-ai-service-layer-);
recorded in [ADR-0013](../adr/0013-ai-service-layer.md). All services follow the
ADR-0010 contract, now generalised into `aadt-assist`: suggest → confidence gate
→ reconcile → DB commits → audit.

- ✅ **Dimension prediction** — one linear dimension proposed per undimensioned
  wall; confidence from detection quality; already-dimensioned walls skipped.
- ✅ **Revision comparison narration** (extend `aadt-pdf` compare) — shipped as
  M12.6: `aadt-assist::narrate_revision` wraps the M7 fingerprint diff with
  detection count deltas and optional BM25 citations; surface is
  `COMPARE <before>` (advisory `SuggestionKind::RevisionCompare`).
- ✅ **Standard-detail recommendation** (retrieval over the Phase-2 corpus) —
  shipped as M12.7: bundled `libraries/details/*.md` sheets, auto-seeded by
  `RECOMMEND DETAIL [query]` into the session corpus; advisory citations only.
- ✅ **Natural-language command routing** — `"draw a wall from 0,0 to 5,0"` →
  `LINE 0,0 5,0`, shown but not executed. NL never reaches geometry directly.
- ✅ **Risk/code checks as rule packs** — three deterministic geometric rules,
  citing indexed spec passages via the Phase-2 BM25 search.

**The mechanism, not just the services.** The reason this phase needed a crate
rather than three functions: an `Action` is *data*, not a closure, and services
never receive `&mut Document`. A service therefore cannot commit even by
accident — only `apply` can, and it always transacts and always audits. That
also makes the awkward cases fall out correctly: a code finding carries
`Action::None`, so "auto-fix the violation" is not a thing the type system will
express.

**Rejections are audited too.** A trail that records only accepted suggestions
cannot distinguish "considered and declined" from "never raised" — which is the
question a reviewer actually asks.

## Phase 4 — Construction intelligence (takeoff returns) — ✅ core shipped as **M13**

Tracked as [ROADMAP M13](../ROADMAP.md#m13--ai-native-phase-4-quantity-takeoff-on-objects-);
recorded in [ADR-0014](../adr/0014-object-based-takeoff.md).

- ✅ Quantity extraction on top of **objects**, not raw geometry: paint/plaster
  areas, brick/mortar volumes, room finishes, opening schedule.
- ✅ Cost codes — [ADR-0005](../adr/0005-cost-code-standard.md) revived unchanged
  (CSI MasterFormat), mapped by object kind and refined by layer name.
- ✅ BOQ generation, aggregated by cost code with deterministic ordering.
- ✅ `takeoff_*` tables in `aadt-store` re-activated — no schema change needed.
- ⬜ Reports as plugins — deferred; the engine and the surfaces ship, the plugin
  packaging does not.

**Why this phase had to come after Phase 1.** The M3 engine measured raw
geometry, and a bag of lines and block inserts cannot say which door pierces
which wall. It could report a *gross* wall area honestly, or guess deductions by
proximity — neither is good enough to price from. `Door —BelongsTo→ Wall` in the
Graph DB turns the deduction into a lookup. Where the graph attributes nothing,
the opening is measured and scheduled but subtracted from nothing, and the
command says so: under-deducting is conservative and visible, guessing is
neither.

**What 2D cannot measure is declared, not invented.** Storey height is a config
input (a plan has no height). Openings without drawn dimensions use defaults and
are flagged `measured: false`, and `TAKEOFF` reports the count — so an estimator
confirms exactly the assumed figures instead of discovering one mid-tender.

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
