# AADT Roadmap

This roadmap sequences AADT from a proven deterministic takeoff slice up to the
full product. It reorders the spec's suggested phases (§10) so the **element
detection and takeoff pipeline — the product's reason for existing — is proven
first**, on the minimum substrate it needs, rather than after a full CAD core.

Each milestone is a shippable, verifiable increment. Crate skeletons for later
milestones already exist and document their intended responsibility.

## Guiding constraints (from the spec §8)

- **Deterministic core.** Geometry, numbering, and quantity computation are
  reproducible and auditable **independent of AI availability**.
- **Local-first, on-demand AI.** No AI runs in the background; actions are
  invoked explicitly and release resources afterward.
- **Offline-capable.** Full drafting, detection/takeoff, and PDF markup work
  with no internet connection.

## Two deliberate deviations from spec §10

1. **LISP is deferred from Phase 1 to M6.** The AutoLISP-compatible interpreter
   is a large sub-project. The command registry (`aadt-command`) is designed
   LISP-ready from M1 so firm-routine migration is not architecturally blocked,
   but a full interpreter is not on the critical path to the takeoff
   differentiator. See ADR references in [`adr/`](adr/).
2. **DWG read/write is deferred behind DXF.** ASCII DXF is tractable now and
   covers file exchange for the slice; binary DWG needs the ODA SDK or a
   LibreDWG FFI binding and gets its own milestone. See
   [`adr/0002-dwg-dxf-strategy.md`](adr/0002-dwg-dxf-strategy.md).

---

## M0 — Deterministic Takeoff Vertical Slice ✅ (implemented)

**Goal:** prove the differentiator end-to-end, headless, with no GUI and no AI.

`DXF import → wall/room detection → deterministic takeoff → CSV export`, driven
by the `aadt` CLI and covered by a golden-file test.

Delivered:
- `aadt-geometry` — the subset of the 2D kernel the detector needs (points,
  vectors, segments, polylines, tolerance, bbox, parallel/offset/overlap).
- `aadt-model` — in-memory entity store with layers and attributes.
- `aadt-dxf` — dependency-free reader for `LINE` and `LWPOLYLINE`.
- `aadt-detect` — deterministic wall pairing, wall graph, room extraction.
- `aadt-takeoff` — wall length/thickness/footprint, room area/perimeter, CSV.
- `aadt-cli` — `aadt takeoff <input.dxf> [--csv <out>]`.

Verify: `cargo test --workspace` (golden + reproducibility), and
`cargo run -p aadt-cli -- takeoff fixtures/sample-floorplan.dxf --csv out.csv`.

---

## M1 — CAD Core Substrate

Turn the M0 skeletons into a usable drafting surface (spec §4).

**Headless foundations — ✅ implemented:**
- Document model with **atomic undo/redo transactions** (§4.1) — `Document::transact`
  groups changes into a single undoable unit; bulk machine edits (file load) stay
  untracked. See `aadt-model`.
- Command interpreter (§4.2): case-insensitive registry with aliases, history,
  comments, and script execution; drawing built-ins (`LINE`/`LAYER`/`ERASE`/`UNDO`/
  `REDO`/`LIST`) plus CLI-registered `OPEN`/`SAVE`/`TAKEOFF`. Driven by `aadt run
  <script>`. See `aadt-command`, `crates/aadt-cli`.
- **DXF export** and round-trip, plus legacy `POLYLINE`/`VERTEX` import. See
  `aadt-dxf`.

**Remaining:**
- Rendering: `wgpu` 2D canvas targeting 60fps on 100k+ entities (§4.3). *(Needs a
  display; verified interactively rather than headlessly.)*
- Interactive draw/modify/annotate/organize commands (curated ~60–80 set) and the
  `aadt-app` shell with an interactive prompt.
- Tab-completion; remaining DXF entity types (arcs, circles, text, hatch, blocks,
  dimensions).

## M2 — Element Detection & Classification (deterministic)

Spec §5.1 steps 3–5, deterministic. **Mostly implemented:**
- ✅ Openings (attributed door/window blocks), structural elements (circles as
  columns; structural-layer rectangles as columns/beams by aspect ratio),
  furniture (block-name patterns) — all via `ClassifyConfig`. See `aadt-detect`.
- ✅ Sequential numbering per category; tag points at element centroid/insertion
  (`Detection::tags`).
- ✅ Room derivation from the wall graph's bounded faces (`WallGraph::faces`,
  `Detection::graph_rooms`), alongside M0's explicit closed polylines.
- **Remaining:** a merge/dedup policy between explicit and graph-derived rooms;
  block/insert support in the interactive UI.

## M3 — Takeoff & Quantity Engine

Spec §5.2. **Core implemented:**
- ✅ Wall gross area & volume from configured height; **opening subtraction**
  (net wall area). See `aadt-takeoff`.
- ✅ **Cost-code mapping** — CSI MasterFormat defaults + firm overrides
  (`CostCodeMap`), carried into the CSV. See
  [`adr/0005-cost-code-standard.md`](adr/0005-cost-code-standard.md).
- ✅ Opening/structural counts in the takeoff CSV.
- ✅ **Live-linked** table (`LiveTakeoff`) — the link-and-update mechanism
  (spec §5.2 "technical heart"): recomputes only when the document's revision
  changes, so editing a wall updates the quantities.
- **Remaining:** true `.xlsx` export and in-drawing schedule tables.

## M4 — AI Orchestration Layer

Layer AI on the proven deterministic base (spec §6):
- On-demand Ollama subprocess with warm-idle lifecycle and auto-unload.
- Structured JSON prompting; structured JSON responses applied as undoable ops.
- Wall-type inference (§5.1 step 6), anomaly flagging, schedule formatting.
- Graceful degradation when Ollama is unavailable; never blocks drafting.
- Per-invocation audit log of exactly what structured data was sent (§6.3).

## M5 — Documentation & Sheet Production

Spec §5.3: title-block templates, general notes library, `PLOTSETUP` workflow,
AI-assisted notes suggestion (overridable), `AADT-PRESENT`, documentation QA.

## M6 — LISP Engine

Spec §4.4: embedded AutoLISP-syntax interpreter — entity create/query/modify,
selection sets, command invocation, sandboxed file I/O, and **read access to
AADT Engine outputs** (wall graph, detected elements, takeoff data).

## M7 — PDF Markup & Comparison

Spec §4.5 / §6.4: PDF viewing, markup and measurement tools, markup list,
document comparison, batch tools, and AADT-narrated change summaries after a
`COMPARE`.

## M8 — Cloud Collaboration (separate track)

Spec §11 Phase 7: opt-in real-time multi-user markup, architected independently
from the local-first core.
