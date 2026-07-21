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
  comments, tab-completion, and script execution. Draw (`LINE`, `POLYLINE`,
  `RECTANGLE`, `CIRCLE`, `ARC`, `TEXT`, `INSERT`), modify (`MOVE`, `COPY`,
  `ROTATE`, `SCALE`, `MIRROR`, `OFFSET`, `TRIM`, `EXTEND`, `FILLET`, `CHAMFER`,
  `ARRAY`, `EXPLODE`, `ERASE`), organize (`LAYER`/`LAYERS`), annotate
  (`DIM`, `HATCH`), select (`SELECT`, `GROUP`, `GROUPS`, `UNGROUP`), inspect
  (`LIST`, `DIST`, `AREA`), and history (`UNDO`/`REDO`) built-ins, plus
  CLI-registered `OPEN`/`SAVE`/`TAKEOFF`/`SCHEDULE`. Driven by `aadt run`/`aadt
  shell`. See `aadt-command`, `aadt-cli`.
- **Selection sets** — a current selection (`SELECT ALL|LAYER|WINDOW|ids`) and
  named `GROUP`s; MOVE/COPY/ERASE/ROTATE/SCALE/MIRROR take an id, a group, `ALL`,
  or `SELECTION` and edit the whole set in one undoable transaction.
- **Dimensions** — the full DXF set: linear/aligned, **angular** (`DIMANGULAR`),
  **radius** (`DIMRADIUS`), **diameter** (`DIMDIAMETER`), and **ordinate**
  (`DIMORDINATE` X/Y), each undoable, drawn by the renderer, and round-tripped
  through DXF (type codes 0/1/2/3/4/6 + the 15/25 vertex point).
- **Undoable annotation** — `add_hatch`/`add_dimension` record an
  `Op::AddHatch`/`Op::AddDimension` inside a transaction (interactive `HATCH`/
  `DIM`) and stay untracked on bulk load, so annotations undo/redo and
  round-trip through DXF.
- **Line-editing geometry** — `Vec2::perp`, segment/line and circle/line
  intersection, arc angle containment, and `Geometry::offset` (mitred
  poly-lines) in the kernel. TRIM/EXTEND handle lines, arcs, and open
  poly-lines against line/arc/circle cutters; FILLET/CHAMFER round or bevel a
  corner between two lines.
- **Geometry transforms** (rotate/scale/mirror, all id-preserving and undoable)
  in `aadt-model`, shared by the commands and the LISP bridge.
- **Layer state** — ACI colour, on/off, and freeze/thaw, undoable
  (`Document::set_layer_color/visible/frozen`); the `LAYER` command sets it,
  the renderer hides off/frozen layers, and it round-trips through the DXF
  `TABLES`/`LAYER` table.
- **DXF round-trip** for the `LAYER` table plus `LINE`, `LWPOLYLINE`, legacy
  `POLYLINE`/`VERTEX`, `CIRCLE`, `ARC`, `TEXT`, `INSERT`, `BLOCK`, `HATCH`, and
  `DIMENSION`. See `aadt-dxf`.
- Rendering seam: [`build_drawlist`] flattens all entities (block inserts
  instantiated), hatch outlines, and dimensions into a backend-agnostic list;
  the GPU/window backends live behind `--features gpu`/`gui`/`app`.

**Remaining (display-bound):**
- The live `wgpu` 2D canvas at 60fps on 100k+ entities and a custom egui viewport
  (§4.3) — the drawlist and camera math are proven headlessly; the GPU path needs
  a screen to verify.
- Broadening the interactive command set further toward the ~60–80 curated band
  (LEADER/MTEXT annotate tools, PURGE, STRETCH, spline/ellipse geometry).

## M2 — Element Detection & Classification (deterministic)

Spec §5.1 steps 3–5, deterministic. **Mostly implemented:**
- ✅ Openings (attributed door/window blocks), structural elements (circles as
  columns; structural-layer rectangles as columns/beams by aspect ratio),
  furniture (block-name patterns) — all via `ClassifyConfig`. See `aadt-detect`.
- ✅ Sequential numbering per category; tag points at element centroid/insertion
  (`Detection::tags`).
- ✅ Room derivation from the wall graph's bounded faces (`WallGraph::faces`,
  `Detection::graph_rooms`), alongside M0's explicit closed polylines.
- ✅ Merge/dedup policy between explicit and graph-derived rooms
  (`Detection::merged_rooms`).
- ✅ Block instantiation in the engine: `Geometry::instanced` bakes a block
  reference into world-space geometry (the `EXPLODE` command and `aadt-explode`),
  the same placement the renderer draws.
- **Remaining (display-bound):** interactive on-canvas block placement/editing.

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

Layer AI on the proven deterministic base (spec §6). **Core implemented** (in
`aadt-ai`):
- ✅ Structured-data extraction (`extract_wall_facts` — no raw geometry, §6.2)
  and a deterministic JSON-only prompt builder.
- ✅ Wall-type inference (§5.1 step 6): `ModelClient` trait, response parser,
  and the `infer_wall_types` orchestrator, exercised end-to-end with a
  `MockClient`.
- ✅ **Graceful degradation** — an unavailable model yields `AiError::Unavailable`
  and never blocks the deterministic pipeline (§6.1 step 7).
- ✅ Per-invocation **audit log** of exactly what structured data was sent (§6.3).
- ✅ `OllamaClient` — a dependency-free HTTP client to `localhost:11434` over
  `std::net` (requires a running Ollama; not unit-tested).
- **Remaining:** subprocess spawn / warm-idle / auto-unload lifecycle; applying
  results as undoable ops in the app; anomaly flagging; schedule formatting;
  notes suggestion.

## M5 — Documentation & Sheet Production ✅ (core implemented)

Spec §5.3. **Implemented** in `aadt-docs`: `PLOTSETUP` sheet setup (ISO/ANSI/ARCH
sizes + border), title-block placement as geometry, a firm general-notes library
with rule-based (deterministic) suggestion from detected elements, and
documentation QA that gates issue. Driven by `aadt sheet`. **Remaining:** fitting
model space into the sheet viewport at scale, and `AADT-PRESENT` layout.

## M6 — LISP Engine ✅ (complete)

Spec §4.4. **Implemented** in `aadt-lisp`: an AutoLISP-flavoured interpreter
(reader, lexical evaluator, standard library) with the special forms `setq`,
`defun`, `lambda`, `if`, `cond`, `while`, `repeat`, `foreach`, and **read access
to AADT Engine outputs** via `aadt-*` query functions. Driven by `aadt lisp`.
Now also: **dotted pairs** `(a . b)`, lambda **`&rest`** tails and `/`-locals,
and entity **creation/modification** from LISP (`aadt-add-line`/`circle`/`arc`/
`text`/`polyline`, `aadt-move`/`erase`) through the undo-tracked engine bridge.

## M7 — PDF Markup & Comparison ✅ (core implemented)

Spec §4.5 / §6.4. **Implemented** in `aadt-pdf`: a dependency-free vector-PDF
plotter, a non-destructive markup layer (filterable list), and document
comparison by content fingerprint with a **narrated** change set for
transmittals. Driven by `aadt pdf` and `aadt compare`. **Remaining (needs a PDF
reader engine + display):** interactive viewing, freehand/stamp tools, on-PDF
measurement, raster diff.

## M8 — Cloud Collaboration (separate track) ✅ (implemented)

Spec §11 Phase 7. **Implemented** in `aadt-collab`: a convergent operation log
(Lamport-ordered, OR-Set semantics) for multi-user markup — `Replica::merge` /
`Replica::integrate` is the sync primitive, proven commutative, idempotent, and
order-independent so any peers that exchange the same operations converge. The
**network transport** now ships too: `aadt-collab::sync` is a dependency-free TCP
relay (`RelayServer` + `Client`, one line per op via `aadt-collab::wire`),
verified over a loopback socket in `tests/network.rs` and runnable as
`aadt collab --net`. **Remaining (needs a running deployment + display):**
presence/cursors and a hosted session service.
