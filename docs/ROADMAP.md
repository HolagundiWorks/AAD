# AAD Roadmap

This roadmap records how AAD was built as a lean **2D architectural drafting
tool**: keyboard-first CAD with DXF/DWG (incl. IMAGEDEF), PDF plotting, LISP,
Elements/Fills/wall openings, sheets, object-based takeoff, assistive AI, and
the local enterprise API.

Each milestone is a shippable, verifiable increment.

> **Direction (2026-07):** the long-term destination is the
> [AI-Native CAD Platform](vision/ai-native-platform.md) — engineering
> intelligence on a multi-layer data architecture (geometry / object / metadata /
> vector / graph / spatial stores), **still 2D**. See
> [ADR-0012](adr/0012-ai-native-platform-pivot.md) and the
> [phased roadmap](plans/2026-07-ai-native-phases.md). Product locks below govern
> the shipping product.

## Product locks

- **2D CAD only — permanent.** Plan/elevation/section drafting in the plane.
  No solids, meshes, orbit modeling, BIM authoring, or IFC model exchange.
  Wall height and volume in takeoff are **attributes on 2D geometry**, not a
  3D scene. This lock is not scheduled for reversal.
- **Lean commands.** A small daily drafting band people actually use — not an
  AutoCAD command-count chase. See [Curated command band](#curated-command-band).
- **LISP for depth.** Firm routines and automation live in AutoLISP-flavoured
  scripts (`aadt-*` bridge). No second scripting language.
- **WinUI 3 shell (shipping path).** C++/WinRT + Fluent 2 + Direct2D under
  [`native/aad-winui/`](../native/aad-winui/). Rust engine via `aadt-ffi`
  ([ADR-0017](adr/0017-winui-shell-rust-engine.md)). egui (`aadt app`) is
  maintenance fallback only — not a rewrite of the engine.
- **Deterministic core.** Geometry, numbering, and quantities are reproducible
  independent of AI availability. Local on-demand AI only.
- **No cloud collaboration product.** Offline-first drafting; enterprise reach
  is the local API + connectors (M14), not a multi-user sync service.

## Guiding constraints (from the spec §8)

- **Deterministic core.** Geometry, numbering, and quantity computation are
  reproducible and auditable **independent of AI availability**.
- **Local-first, on-demand AI.** No AI runs in the background; actions are
  invoked explicitly and release resources afterward.
- **Offline-capable.** Full drafting, detection/takeoff, and PDF markup work
  with no internet connection.

## Two deliberate deviations from spec §10

1. **LISP is deferred from Phase 1 to M6** (now ✅). The command registry was
   LISP-ready from M1; the interpreter ships and is reachable from the Session
   (`LISP` / bare `(…)`).
2. **DWG/DXF round-trip is deferred behind DXF**, then completed via open-source
   `acadrust` R/W for a 2D subset. See
   [`adr/0002-dwg-dxf-strategy.md`](adr/0002-dwg-dxf-strategy.md).

---

## Curated command band

AAD is a **powerful 2D drafting tool free of bloat**. Depth goes into LISP, not
more command names. Interaction is **AutoCAD-like** (prompts, keywords,
noun/verb + verb/noun, dyn input) — see [`docs/ux/`](ux/README.md).

**Daily (interactive + prominent autocomplete):**

| Kind | Commands |
|---|---|
| Draw | `LINE` `POLYLINE` `CIRCLE` `ARC` `RECTANGLE` `XLINE` `TEXT` `INSERT` |
| Edit | `MOVE` `COPY` `ROTATE` `SCALE` `MIRROR` `OFFSET` `TRIM` `EXTEND` `FILLET` `STRETCH` `ARRAYRECT`/`ARRAYPOLAR`/`ARRAYPATH` `ERASE` |
| Org | `LAYER`/`LAYERS` `LAYERSET` `SELECT` `UNDO`/`REDO` `DIST` `LIST` `OPEN` `SAVE` (`.dxf` / `.dwg` / `.aadt` / `.vdb`) `UNITS` |
| Annotate | `TEXT` `MLEADER` `DIM` / `DIMRADIUS` / `DIMDIAMETER` / `DIMANGULAR` `DIMSTYLE` `REVCLOUD` |
| Arch | `WALL` `DOOR`/`WINDOW` (pick both faces · swing) `HATCH`/Fills `IMAGE` Elements catalog |
| AAD | `SECTION` `STANDARDS` `BLOCKLIB` `DEMO`/`AIDEMO` |
| Intelligence | `OBJECTS`/`RELATIONS` · `TAKEOFF`/`BOQ` · `PERMS` · `CONNECTOR` · `SITE` |
| Assist (proposes, never commits) | `FIND` `ASKDOCS` · `SUGGEST` `CHECK` `ASK` · `COMPARE` (`DIFF`/`REVCOMPARE`) · `RECOMMEND DETAIL` (`DETAIL`/`DETAILS`) |
| Power | `LISP` / bare `(…)` · `LISPLOAD` |

The **Assist** row is bounded on purpose: every command there either retrieves
something already in the drawing or an indexed document, or emits a proposal a
human accepts. None of them write geometry on their own (ADR-0013).

**Click-tools in the egui viewport:** `L` `C` `REC` `A` `XL` `O` `T` `F` `MLEADER` `STRETCH` `BRUSH` ·
`M` `CO` `RO` `SC` `MI` `AR`/`ARRAYPOLAR`/`ARRAYPATH` · `E` · `DIM*` · pick/box select · Ortho (Shift / F8) ·
Osnap endpoint/midpoint (F3) · Polar (F10).

**Keep but de-emphasize** (scripts / DXF fidelity, not GUI chrome): other `DIM*`
variants, `HATCH`, `CHAMFER`, `EXPLODE`, `PURGE`, `GROUP*`/`UNGROUP`,
`AREA`.

**Do not add:** MTEXT sprawl, spline/ellipse without a real need, ribbon/toolbar
forests, 3D commands, BIM authoring, paper-space XREF workflows.

**Presentation rasters (in scope):** PNG/JPEG furniture and logos via
`Geometry::Image` + Elements catalog / `IMAGE` place; DWG/DXF export uses
AutoCAD `IMAGE` + `IMAGEDEF` (see [DWG fidelity matrix](interop/dwg-fidelity-matrix.md)).
Not a freehand paint program — vector drafting stays primary.

**Standards / presentation:** AIA/US NCS layers (`aadt-standards`, WCAG vs white
paper), layer sets (`CONSTRUCTION` / `SERVICES` / `ELECTRICAL` / `PRESENTATION`),
light print-preview GUI, `MLEADER`, `STRETCH`, `SECTION`,
[block / hatch / detail / elements libraries](../libraries/README.md), typed
Fills (`FillKind`), Technical / Presentation view presets.

---

## M1 — CAD Core Substrate ✅ (headless) · interactive shell ✅ (WinUI path · egui fallback)

**Headless foundations — ✅:** atomic undo/redo, command interpreter with the
curated band above, selection sets, dimensions (full DXF set), hatch, layer
state, DXF round-trip, drawlist/camera seam.

**Interactive surface — shipping WinUI / Direct2D** ([`native/aad-winui/`](../native/aad-winui/),
[ADR-0017](adr/0017-winui-shell-rust-engine.md)); **egui fallback ✅:**
- Command-first shell with pick/box select, click-to-draw
  (`L`/`C`/`REC`/`A`/`XL`), noun-verb **and** verb-noun `M`/`CO`/`RO`/`SC`/`MI`/`E`,
  AutoCAD-like keywords (`[U]`/`[C]`/`[D]`/`[R]`), dynamic input, Ortho (F8),
  native file dialogs (`rfd`), Units/Dims managers, 3-point `DIM`.
- `OPEN`/`SAVE`/`LISP`/`LISPLOAD` on the same `Session` the
  CLI shell uses; bare `(…)` evaluates AutoLISP against the live document.
- Thin `aadt-ffi` cdylib — **shipping** WinUI / Direct2D host loads `aadt_ffi.dll`
  ([ADR-0017](adr/0017-winui-shell-rust-engine.md)); egui remains a fallback driver.
- Interaction docs: [`docs/ux/`](ux/README.md).

**Transparent commands — ✅:** an apostrophe prefix (`'ZOOM`, `'PAN`) runs a
view command without cancelling the one in progress; the interrupted command's
tool state, engine step, and prompt are parked and restored verbatim (Esc
during it resumes rather than cancelling). Only view commands qualify — a
transparent command that could edit geometry is refused.

**Remaining (display-bound, not command sprawl):**
- Viewport text glyph quality.
- True `wgpu` 2D canvas when the egui painter is actually too slow (100k+).

**Native format — ✅ v0:** SQLite `.aadt` via `aadt-store` (layers, entities with
stable ids, blocks, hatches, dimensions). `OPEN`/`SAVE` select by extension.
See [`adr/0004`](adr/0004-native-file-format.md).

---

## M2 — Element Detection & Classification ✅

Openings, structural, furniture classification; tags; room-from-wall-graph;
`merged_rooms` dedup; anomaly flagging. **Remaining (display):** interactive
on-canvas block placement polish.

---

### Hatch pattern library ✅ (expanded)

28 architectural/floor `.pat` patterns internalised under
[`libraries/hatches/`](../libraries/hatches/) (brick, block, stone, tile,
insulation, metal, floor grids/staggers, …). `aadt-render` now **parses the
`.pat` line families** and renders them faithfully (multi-family, native
angle + spacing) via an embedded library, instead of the old name→angle map;
legacy builtins keep the simple model as a fallback. Dash arrays are parsed
but drawn solid for now (approximation).

## M5 — Documentation & Sheet Production ✅

`PLOTSETUP`, title block, notes library, QA, compose (`place_drawing`),
`AADT-PRESENT` multi-view layout. Driven by `aadt sheet` / `aadt present`.

---

## M6 — LISP Engine ✅

AutoLISP-flavoured interpreter + `aadt-*` query/create/modify bridge. Session
and CLI shell evaluate `LISP` / bare `(…)` against the live document. Driven by
`aadt lisp` for batch scripts.

---

## M7 — PDF Markup & Comparison ✅ (core)

Vector PDF plot, markup list, narrated compare. **Remaining (needs PDF viewer
UI):** interactive viewing, freehand/stamps, on-PDF measure, raster diff.

---

## Next ✅

The former Next polish items are **done**:

1. **DWG write / round-trip** — `export_dwg_path` via `acadrust` `DwgWriter`;
   `SAVE *.dwg` / GUI filter; best-effort 2D subset (hatches/dims skipped on
   DWG; **IMAGE + IMAGEDEF** shipped — see M15 / fidelity matrix).
2. **DXF multi-path hatch** — `Hatch.boundaries` (outer + islands); DXF `91`/`93`
   round-trip; store codec `HATCHPATHS`.

**Locked out of scope:** paper space / XREF / VP (nameplate PDF plotting is the
sheet path). **UNDO ≠ HISTORY** — keep both; do not merge. **2D only** — no
solids, BIM authoring, or 3D scene.

### Floors + category model spaces (ADR-0009) ✅

ArchiCAD-style **floors** with a shared **reference origin**, and per-floor
**model spaces** that *are* the element categories (Walls / Structural / Lintel /
BeamLayout / Furniture)—not layer filters. `TRACE` ghosts adjacent floors;
Levels strip + `FLOOR` / `CATEGORY` / `ORIGIN` commands; `.aadt` schema v6.

## M9 — Drafting Workflow Parity (BricsCAD audit) ✅

Closes the interaction gaps found in the
[BricsCAD workflow conformance audit](ux/bricscad-conformance-audit.md). The
command lifecycle, spacebar semantics, command line, and modify flow already
conform; M9 is **breadth** on top of that core. Sequenced by daily-drafting
impact.

| # | Item | Spec § | Status | Notes |
|---|------|--------|--------|-------|
| M9.1 | Object snaps: Quadrant · Nearest · Intersection running snaps; **Perpendicular / Tangent** and `END/MID/INT/CEN/QUA/NEA/PER/TAN` keyword overrides + SHIFT+right-click snap menu | §17 | ✅ | Running snaps plus PER/TAN geometry and the one-shot override resolver (`find_override_in_spaces` / `parse_snap_keyword`) live in [`osnap.rs`](../crates/aadt-app/src/osnap.rs), headless-tested. The GUI arms an override from a keyword typed at a point prompt or the SHIFT+right-click menu; PER/TAN use the live drag anchor, and overrides work with running osnap (F3) off |
| M9.2 | Selection: Fence, WPolygon/CPolygon | §12 | ✅ | Headless region-select in [`selection.rs`](../crates/aadt-app/src/selection.rs) (unit-tested); GUI `WP`/`CP`/`FENCE` tools (click corners → Space). Select drag is **box only** (L→R window / R→L crossing) — free-drag lasso is not shipped. See [engineering Vol 6](engineering/vol-06-selection-engine.md) |
| M9.3 | Multi-mode grip editing (Move/Rotate/Scale/Mirror cycled with Space on a hot grip) | §14 | ✅ | `GripMode` (Stretch→Move→Rotate→Scale→Mirror) cycled with Space on a hot grip; each mode transforms the selection about the grip via the tested MOVE/ROTATE/SCALE/MIRROR/GRIPEDIT commands. Preview shows the base→cursor axis (grip ghost for Stretch) |
| M9.4 | Single auto-type `DIM` — `DIM <id>` infers the type from the entity (line→aligned, circle→diameter, arc→radius); coordinate form kept | §22 | ✅ | Command-level auto-type in [`aadt-command`](../crates/aadt-command/src/lib.rs) (headless-tested). GUI **hover-to-auto-dim**: DIM's first click on an entity auto-dimensions it with a live ghost preview (`auto_dimension_for` + `dimension_commands`), empty space keeps the two-point form. **DIMANGULAR from two lines**: pick two lines → the angle at their intersection (`line_line_intersection`), else the 3-point vertex form |
| M9.5 | Live hatch preview on internal-point pick | §21 | ✅ | Was boundary-entity-only. Now `hatch_boundary_at` detects the smallest enclosing closed region under the cursor (AutoCAD internal point); HATCH hovers show a translucent accent fill + boundary preview, click commits (boundary-entity pick kept as fallback) |
| M9.6 | Docking: floating panels + auto-hide | §28 | ✅ | Without `egui_dock`: each manager has `PanelMode::{Docked, Floating, AutoHide}` persisted in prefs. Header buttons float / unpin (auto-hide) / pin (re-dock). Floating is a title-bar-less `egui::Window` with position/size remembered; auto-hide keeps the edge rail and expands a foreground overlay on hover (350 ms grace). Bottom-dock of the managers stays deferred — Drawing Properties already uses a bottom panel |
| M9.7 | Per-command capability trait (`CanUndo`/`SupportsPreview`/`SupportsSnap`/`SupportsDynamicInput`/`SupportsCommandLine`/`SupportsSelectionPreview`) | §33 | ✅ | `Capabilities` + `CommandDef::capabilities()` derive the §33 contract from each command's step interface (query set → `can_undo`); surfaced on hover in the command palette; contract test across the catalog |
| M9.8 | Per-command QA harness asserting the §34 checklist across every command | §34 | ✅ | `qa_checklist_every_command` iterates the whole catalog asserting starts / cancels / repeats (per `repeatable`) / keyboard-line addressable / snap+preview+mixed-input consistency / undo semantics; builds on the M9.7 capability contract |
| M9.9 | Frame-time / latency telemetry to validate the §31 performance budgets | §31 | ✅ | **F7** toggles a performance HUD: rolling frame time (avg / p95 / max) + FPS with a build/paint breakdown, graded live against the §31 **< 16 ms / 60 FPS** redraw budget. Continuous repaint while it's on measures a real sustained loop. `PerfRing` / `PerfTelemetry` in [`gui.rs`](../crates/aadt-app/src/gui.rs) |

**§35 conflict (product lock):** cloud generative “AI command suggestions / PDF
recognition” style features are **not scheduled**. Assist stays local,
propose-only (M12). **Auto layer assignment** is the bounded exception:
deterministic embeddings (`aadt-embed` / `aadt-vecdb`) map elements to AIA
layers + model-space categories with confidence gating (`MAPMETA`) —
[ADR-0010](adr/0010-embeddings-metadata-mapping.md).

**Out of scope for M9:** Ribbon (command-driven by design), View Cube (spec-optional).

## M10 — AI-Native Phase 1: Intelligent objects + Graph DB ✅ (core)

First implementation phase of the [AI-native platform](vision/ai-native-platform.md)
(ADR-0012; [phased roadmap](plans/2026-07-ai-native-phases.md)). Geometry stays
the source of truth in the `Document`; objects reference entities, never replace
them.

| # | Item | Status | Notes |
|---|------|--------|-------|
| M10.1 | `aadt-object` — **Object DB**: typed engineering objects (Wall/Room/Door/Window/Column/Beam/Furniture) with stable `ObjectId`, member entities, engineering properties, AI confidence, revision stamp | ✅ | Seeded by promoting `aadt-detect` output |
| M10.2 | `aadt-object` — **audit log**: append-only per-object entries (user, timestamp, action, prev/new, AI-suggested, approval, reason); nothing deleted | ✅ | Extends the ADR-0008 history model to object granularity |
| M10.3 | `aadt-graph` — **Graph DB**: persisted typed relationships (`ConnectedTo`, `BelongsTo`, `Contains`, `Bounds`, `Supports`) with forward/reverse queries + traversal | ✅ | Door → *belongs to* → Wall; Room → *contains* → Furniture; Wall → *bounds* → Room |
| M10.4 | Persistence: `.aadt` schema **v8** (objects/props/edges/audit tables) + ShilpiDB `.vdb` **v2** sections; carried across resave | ✅ | Both formats carry objects/graph/audit across resave (additive; v7/v1 files still open). `SAVE .vdb` writes the live Object DB via `save_path_with_objects`, and `ObjState::sync` loads it back on OPEN — round-trip proven across processes by a golden test (`OBJECTS LIST` shows the persisted objects without re-promoting). |
| M10.5 | Surface: `OBJECTS` / `RELATIONS` commands + LISP (`aadt-objects`, `aadt-relations`) | ✅ | Promotion is deterministic and undo-safe (objects live beside the document) |

### ShilpiDB known gaps

ShilpiDB (`aadt-vdb`, `.vdb`; [ADR-0011](adr/0011-vector-database-store.md)) ships
as a custom, dependency-free vector store: entities + layers/blocks/hatches/dims/
floors/props at `.aadt` fidelity, a built-in spatial grid (`query_bbox` /
`query_point`, `WITHIN` / `VSELECT`, LISP `aadt-within`), durable temp+rename
writes, and OPEN/SAVE wiring in the CLI and GUI — **now including the Object DB**
(`SAVE .vdb` persists the live `ObjectStore` / `Graph` / `AuditLog`; OPEN loads
them back). Remaining boundaries, both intentional:

- **No history / time-machine** in `.vdb` — checkpoints stay a `.aadt`-only
  feature (ADR-0011).
- **2D vector records only** — no splines / meshes / solids. Geometry stays
  plan-based; height is an attribute where takeoff needs it.

## M11 — AI-Native Phase 2: Semantic search + local retrieval ✅ (core)

Second implementation phase (ADR-0012). Retrieval only: every answer points at
content that already exists in the drawing or an indexed document. Nothing is
generated, and nothing leaves the machine.

| # | Item | Status | Notes |
|---|------|--------|-------|
| M11.1 | `aadt-embed` — **text embeddings**: 64-dim character n-gram (2/3/4-gram) hashing, plus `drawing_embedding` / `object_embedding` mean-pooling | ✅ | No training and no dependencies; `WALL` / `A-WALL` / `WALL-TYPE` cluster |
| M11.2 | `aadt-vecdb::search` — **semantic search**: `find_by_text` over element labels, `find_similar_to` over geometry, `find_duplicates` by all-pairs cosine | ✅ | `FIND` selects its hits, so the result feeds straight into the next command |
| M11.3 | `aadt-vecdb::corpus` — **local corpus + RAG retrieval**: paragraph chunking, **BM25** ranking over word tokens, n-gram cosine fallback | ✅ | BM25 is what stops shared boilerplate ("shall be … mm") from dominating |
| M11.4 | Persistence: `.aadt` schema **v9** (`corpus_source`, `corpus_chunk`); carried across resave | ✅ | Additive; v8 files migrate with geometry/objects/audit intact |
| M11.5 | Surface: `FIND` / `ASKDOCS` commands | ✅ | `ASKDOCS` cites source + paragraph for every passage it returns |

**Deliberately not in M11:** answer *generation*. `ASKDOCS` retrieves and cites
passages; it never composes prose from them. Composition would mean asserting
something no document says — the opposite of "AI never owns data" (ADR-0012).

## M12 — AI-Native Phase 3: the AI service layer ✅

Third implementation phase ([ADR-0013](adr/0013-ai-service-layer.md)). The first
services that want to write, and the mechanism that keeps them gated: a service
returns proposals, and one audited function commits them.

| # | Item | Status | Notes |
|---|------|--------|-------|
| M12.1 | `aadt-assist` — **suggestion framework**: `Suggestion` + `Action` as data, `apply` inside one undo transaction, append-only audit of both accepts and rejects | ✅ | Services never receive `&mut Document` — they structurally cannot commit |
| M12.2 | **Dimension prediction**: propose a linear dimension per undimensioned wall, confidence from detection quality | ✅ | Re-running skips walls already dimensioned |
| M12.3 | **Code-check rule packs**: min room area / min room width / min door width, graded violation vs warning | ✅ | Advisory only — resolving a violation is a design decision, never auto-applied |
| M12.4 | **NL command routing**: phrase → command *string*, never geometry | ✅ | Refuses unmatched phrasings with candidates rather than guessing |
| M12.5 | Surface: `SUGGEST` / `CHECK` / `ASK` commands | ✅ | `ASK` shows the routed command; running it stays the user's call |
| M12.6 | **Revision-comparison narration** — fingerprint diff + detection deltas + optional corpus citations as an advisory suggestion | ✅ | `aadt-assist::narrate_revision` / `COMPARE <before>`; `Action::None` (transmittal note, never auto-edits). CLI `aadt compare` prints the same enriched note |
| M12.7 | **Standard-detail recommendation** — BM25 retrieval over a bundled detail library | ✅ | Seed sheets in [`libraries/details/`](../libraries/details/); `aadt-assist::recommend_details` + `RECOMMEND DETAIL [query]` (auto-seeds corpus; `SuggestionKind::DetailRecommend`, `Action::None`) |

**Deliberately not in M12:** auto-fixing code findings, and any NL path that
reaches geometry without passing through the command band. Both would move a
decision from the engineer to the tool.

## M13 — AI-Native Phase 4: quantity takeoff, on objects ✅ (core)

Fourth implementation phase ([ADR-0014](adr/0014-object-based-takeoff.md)).
Quantity takeoff against the Object DB — and
[ADR-0005](adr/0005-cost-code-standard.md) CSI cost codes with it.

| # | Item | Status | Notes |
|---|------|--------|-------|
| M13.1 | `aadt-takeoff` — **quantity engine over objects**: wall net area / volume / bricks / plaster, room finishes, opening schedule, structural members | ✅ | Reads engineering props off objects, not geometry |
| M13.2 | **Opening deduction via the Graph DB** (`Door —BelongsTo→ Wall`) | ✅ | The reason takeoff waited for Phase 1; an unattributed opening is *not* deducted, and that is reported |
| M13.3 | **CSI MasterFormat cost codes** — kind first, layer-name refinement | ✅ | ADR-0005 revived unchanged; masonry vs drywall bill separately |
| M13.4 | **BOQ generation** — aggregated by cost code, deterministic ordering | ✅ | Zero-quantity lines dropped; identical output across runs so revisions diff |
| M13.5 | Persistence: the surviving `takeoff_*` tables re-activated | ✅ | No schema change — stable `ObjectId`s are the row key the tables always wanted |
| M13.6 | Surface: `TAKEOFF` / `BOQ` commands | ✅ | Reports how many figures were assumed rather than measured |

**Not measurable in 2D, so declared as input:** storey height, and any opening
without drawn dimensions. Both come from `TakeoffConfig` and are flagged in the
report rather than presented as measurements.

**Deliberately not in M13:** pricing. The bill carries quantities and cost codes;
rates belong to the estimator's system.

## M14 — AI-Native Phase 5: enterprise + APIs ✅

Fifth implementation phase ([ADR-0015](adr/0015-local-api-layer.md),
[ADR-0016](adr/0016-m14-enterprise.md)). Phases 1–4 made the drawing
understandable to this program; Phase 5 makes it reachable from others —
without giving anything else authority over the geometry.

| # | Item | Status | Notes |
|---|------|--------|-------|
| M14.1 | `aadt-api` — **local HTTP/JSON API** over a live `Session`: loopback-only, token-gated, std-only (no `tokio`/`hyper`/`serde`). Surface is `aadt serve [drawing] [--port] [--token] [--read-only] [--once]` | ✅ | Eleven routes ([reference](api/README.md)). Reads project the session; the **one** write path is `POST /v1/command` — the same line a user types, so an API caller inherits the same validation, undo record, and history entry |
| M14.2 | Object-level permissions | ✅ | Token → principal + role caps (`--identity` / `--role`); route + command-verb gates; per-object `acl.*` props via `PERMS GRANT/REVOKE/SHOW` ([ADR-0016](adr/0016-m14-enterprise.md)) |
| M14.3 | Signed audit trail | ✅ | Optional HMAC-SHA256 chain on `AuditEntry` (`AADT_AUDIT_KEY`); `OBJECTS AUDIT VERIFY`; `.aadt` schema v10 + `.vdb` v3 |
| M14.4 | Enterprise connectors (ERP / BOQ / RFI / DMS) as plugins over the API | ✅ | `CONNECTOR ERP/RFI/DMS` commands emit sidecar payloads; reachable over `POST /v1/command` (needs `connector` cap) |
| M14.5 | Site data (photos, reports) ingested into the Vector DB | ✅ | `SITE INDEX` / `ASKDOCS INDEX PHOTO\|PDF\|REPORT` → corpus text chunks (caption + PDF literals); retrieval via `ASKDOCS` |

**Deliberately not in M14.1:** a REST-shaped write API (`POST /v1/entities`).
A second door into the document means every invariant the command layer enforces
has to be re-enforced behind it forever, in step — and the first disagreement
between the two makes the drawing only as correct as whichever door the caller
used. Callers compose command strings instead; that is the same bargain LISP
made, and it keeps the deterministic core the sole author of geometry.

## M15 — Architectural plan CAD (market-gap closeout) ✅

Closes the [market-gap case study](audit/2026-08-market-gap-case-study.md)
priorities (P0–P3): 2D plan production that understands walls, openings, fills,
and presentation rasters — without Revit or AutoCAD dyn-block parity claims.
Honest exchange is published in
[DWG / DXF fidelity matrix](interop/dwg-fidelity-matrix.md).

| # | Item | Status | Notes |
|---|------|--------|-------|
| M15.1 | **Elements + typed Fills** — `AADT-ELEM`, `FillKind` (Technical / Floor / Presentation), Elements/Fills UI | ✅ | Schema / hatch attrs; presentation vs technical boards |
| M15.2 | **Wall openings** — dual-face cut; user picks **both** wall lines; jambs → closed hatchable bodies | ✅ | `wall_openings`; units-aware widths (4½″/9″ or metric) |
| M15.3 | **Door / window place** — hinge on centerline; **swing side** (Tab/F / click) like a dyn-block flip | ✅ | `scale_y` flip; INSERT grips 1=`BFLIP`, 2=`WIDTH` |
| M15.4 | **PNG furniture** — `Geometry::Image`, catalog tiles, `IMAGE` place tool | ✅ | Texture paint in GUI |
| M15.5 | **View / plot presets** — Technical Plan / Presentation; plot compose respects hidden layers | ✅ | Canvas + plot dialog |
| M15.6 | **IMAGEDEF export** — DWG `IMAGE`+`IMAGEDEF`+reactor via acadrust; DXF `export_path` when images present | ✅ | Import maps `RasterImage` → `ImageRef` |
| M15.7 | **Interaction trust** — fill erase, rectangle D-mode, RECTANGLE Tab does not steal chrome / quit | ✅ | Daily-band table stakes |
| M15.8 | **AI demo path** — `DEMO` / `AIDEMO` (detect → objects → next steps) | ✅ | Sales 5-minute narrative |

**Deliberately not in M15:** Autodesk Dynamic Block action editing; hatch/dim
DWG write; sheet-set viewports bound to presets; material opacity libraries.
All of those stay optional Post-1.0 polish — still **2D only**.

## Roadmap status summary

| Milestone | Status |
|-----------|--------|
| M1 CAD core + egui | ✅ |
| M2 Detection | ✅ |
| M5 Sheets / plot | ✅ |
| M6 LISP | ✅ |
| M7 PDF markup | ✅ core (viewer UI Post-1.0) |
| M9 BricsCAD parity | ✅ |
| M10 Objects + graph | ✅ |
| M11 Semantic search | ✅ |
| M12 Assist layer | ✅ |
| M13 Object takeoff | ✅ |
| M14 Enterprise API | ✅ |
| M15 Arch plan CAD / market gaps | ✅ |

**Shipping product (1.0 band):** M1–M2, M5–M7, M9–M15. **2D architectural CAD
only** — no BIM / 3D milestone is planned.

## Post-1.0 (optional — not blocking 1.0)

- M1 display: viewport text glyph quality; wgpu canvas if egui is too slow.
- M7 PDF viewer UI: interactive viewing, freehand/stamps, on-PDF measure, raster diff.
- Optional: further WinUI Fluent polish / MSIX packaging over `aadt-ffi`.
  egui `aadt app` is maintenance-only.
- DWG: hatch + dimension write; Autodesk-certified fidelity; hatch arc/ellipse edges.
- Polish: sheet-bound view presets; material / opacity libraries; SpatialGrid in GUI pick; true OTRACK.
