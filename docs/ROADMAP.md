# AAD Roadmap

This roadmap records how AAD was built as a lean **2D architectural drafting
tool**. Early milestones proved a deterministic detection/takeoff pipeline on a
minimal substrate; the **takeoff/quantity engine (M0/M3), the AI layer (M4), and
cloud collaboration (M8) have since been removed** (see those sections), leaving
a keyboard-first CAD drafting core with DXF/DWG, PDF plotting, LISP, and sheet
production.

Each milestone is a shippable, verifiable increment.

> **Direction (2026-07):** the long-term destination is now the
> [AI-Native CAD Platform](vision/ai-native-platform.md) — an
> engineering-intelligence platform on a multi-layer data architecture
> (geometry / object / metadata / vector / graph / spatial stores). See
> [ADR-0012](adr/0012-ai-native-platform-pivot.md) for which locks below it
> supersedes (phase by phase, never silently) and the
> [phased roadmap](plans/2026-07-ai-native-phases.md) for sequencing. Until a
> phase ships, the locks below govern the shipping product.

## Product locks

- **2D CAD only.** Plan/elevation/section drafting in the plane. No solids,
  meshes, orbit modeling, or BIM/IFC. Wall height and volume in takeoff are
  **attributes on 2D geometry**, not a 3D scene.
- **Lean commands.** A small daily drafting band people actually use — not an
  AutoCAD command-count chase. See [Curated command band](#curated-command-band).
- **LISP for depth.** Firm routines and automation live in AutoLISP-flavoured
  scripts (`aadt-*` bridge). No second scripting language.
- **egui shell now.** Command line + docked panels + 2D viewport. WinUI (or
  other chrome) is only a future option via a thin FFI seam — not a rewrite of
  the engine.
- **Deterministic core.** Geometry, numbering, and quantities are reproducible
  independent of AI availability. Local on-demand AI only.

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
| Org | `LAYER`/`LAYERS` `LAYERSET` `SELECT` `UNDO`/`REDO` `DIST` `LIST` `OPEN` `SAVE` (`.dxf` / `.aadt`) `UNITS` |
| Annotate | `TEXT` `MLEADER` `DIM` / `DIMRADIUS` / `DIMDIAMETER` / `DIMANGULAR` `DIMSTYLE` `REVCLOUD` |
| AAD | `SECTION` `STANDARDS` `BLOCKLIB` |
| Power | `LISP` / bare `(…)` · `LISPLOAD` |

**Click-tools in the egui viewport:** `L` `C` `REC` `A` `XL` `O` `T` `F` `MLEADER` `STRETCH` `BRUSH` ·
`M` `CO` `RO` `SC` `MI` `AR`/`ARRAYPOLAR`/`ARRAYPATH` · `E` · `DIM*` · pick/box select · Ortho (Shift / F8) ·
Osnap endpoint/midpoint (F3) · Polar (F10).

**Keep but de-emphasize** (scripts / DXF fidelity, not GUI chrome): other `DIM*`
variants, `HATCH`, `CHAMFER`, `EXPLODE`, `PURGE`, `GROUP*`/`UNGROUP`,
`AREA`.

**Do not add:** MTEXT sprawl, spline/ellipse without a real need, ribbon/toolbar
forests, 3D commands. Raster brushes / PNG block libraries are out of scope
(vector-only: DXF + SVG blocks, vector freehand on `A-PRES-*`).

**Standards / presentation:** AIA/US NCS layers (`aadt-standards`, WCAG vs white
paper), layer sets (`CONSTRUCTION` / `SERVICES` / `ELECTRICAL` / `PRESENTATION`),
light print-preview GUI, `MLEADER`, `STRETCH`, `SECTION`, block/hatch libraries,
vector brush.

---

## M0 — Deterministic Takeoff Vertical Slice ⛔ removed

Shipped `DXF import → wall/room detection → deterministic takeoff → CSV export`,
then **removed** with the takeoff module (see below). DXF import and wall/room
detection (`aadt-detect`) remain; quantity takeoff and CSV/XLSX export are gone.

Verify: `cargo test --workspace`.

---

## M1 — CAD Core Substrate ✅ (headless) · interactive shell ✅ (egui)

**Headless foundations — ✅:** atomic undo/redo, command interpreter with the
curated band above, selection sets, dimensions (full DXF set), hatch, layer
state, DXF round-trip, drawlist/camera seam.

**Interactive surface — ✅ (egui):**
- Command-first shell with pick/box select, click-to-draw
  (`L`/`C`/`REC`/`A`/`XL`), noun-verb **and** verb-noun `M`/`CO`/`RO`/`SC`/`MI`/`E`,
  AutoCAD-like keywords (`[U]`/`[C]`/`[D]`/`[R]`), dynamic input, Ortho (F8),
  native file dialogs (`rfd`), Units/Dims managers, 3-point `DIM`.
- `OPEN`/`SAVE`/`LISP`/`LISPLOAD` on the same `Session` the
  CLI shell uses; bare `(…)` evaluates AutoLISP against the live document.
- Thin `aadt-ffi` cdylib for future chrome (e.g. WinUI) without engine rewrite.
- Interaction docs: [`docs/ux/`](ux/README.md).

**Remaining (display-bound, not command sprawl):**
- Viewport text glyph quality.
- True `wgpu` 2D canvas when the egui painter is actually too slow (100k+).
- Transparent mid-command `'ZOOM` / grip-edit (see UX parity matrix).

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

## M3 — Takeoff & Quantity Engine ⛔ removed

Implemented (wall area/volume, opening subtraction, CSI cost codes,
`LiveTakeoff`, `.xlsx` export, in-drawing `SCHEDULE` tables) and then
**removed in full**: the `aadt-takeoff` crate, the `TAKEOFF`/`SCHEDULE`
commands, the `takeoff` CLI command, the GUI Takeoff/BOQ dock, and the
takeoff LISP query functions. `aadt-store` keeps its run tables (unused).
The app is now a 2D drafting tool without quantity takeoff.

---

## M4 — AI Orchestration Layer ⛔ removed

Implemented (Foundry Local / Ollama clients, wall-type inference, NL
interpretation, audit log, lifecycle) and then **removed in full** (commit
`e4d93e7`, "Remove all AI features"): the `aadt-ai` crate, the `infer` command,
the RECOGNIZE/REVIEW/ACCEPT/REJECT/FREEZE/ASK command family, and the Foundry
control panel. Deterministic detection/takeoff (`aadt-detect`) is unaffected.
Any reintroduction (see M9 §35 note) is a product decision, not scheduled work.

---

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

## M8 — Cloud Collaboration ⛔ removed

Implemented (convergent op log + TCP relay via `aadt collab --net`, local
presence cursors) and then **removed in full**: the `aadt-collab` crate and
the `collab` CLI command. No collaboration/sync in the app.

---

## Next ✅

The former Next polish items are **done**:

1. **DWG write / round-trip** — `export_dwg_path` via `acadrust` `DwgWriter`;
   `SAVE *.dwg` / GUI filter; best-effort 2D subset (hatches/dims skipped).
2. **DXF multi-path hatch** — `Hatch.boundaries` (outer + islands); DXF `91`/`93`
   round-trip; store codec `HATCHPATHS`.
3. **Local presence** — join / leave / cursor on the TCP relay (ephemeral; not
   in the markup backlog).

**Locked out of scope:** paper space / XREF / VP (nameplate PDF plotting is the
sheet path). **UNDO ≠ HISTORY** — keep both; do not merge.

### Floors + category model spaces (ADR-0009) ✅

ArchiCAD-style **floors** with a shared **reference origin**, and per-floor
**model spaces** that *are* the element categories (Walls / Structural / Lintel /
BeamLayout / Furniture)—not layer filters. `TRACE` ghosts adjacent floors;
Levels strip + `FLOOR` / `CATEGORY` / `ORIGIN` commands; `.aadt` schema v6.

## M9 — Drafting Workflow Parity (BricsCAD audit) 🚧

Closes the interaction gaps found in the
[BricsCAD workflow conformance audit](ux/bricscad-conformance-audit.md). The
command lifecycle, spacebar semantics, command line, and modify flow already
conform; M9 is **breadth** on top of that core. Sequenced by daily-drafting
impact.

| # | Item | Spec § | Status | Notes |
|---|------|--------|--------|-------|
| M9.1 | Object snaps: Quadrant · Nearest · Intersection running snaps; **Perpendicular / Tangent** and `END/MID/INT/CEN/QUA/NEA/PER/TAN` keyword overrides + SHIFT+right-click snap menu | §17 | ✅ | Running snaps plus PER/TAN geometry and the one-shot override resolver (`find_override_in_spaces` / `parse_snap_keyword`) live in [`osnap.rs`](../crates/aadt-app/src/osnap.rs), headless-tested. The GUI arms an override from a keyword typed at a point prompt or the SHIFT+right-click menu; PER/TAN use the live drag anchor, and overrides work with running osnap (F3) off |
| M9.2 | Selection: Fence, WPolygon/CPolygon, Lasso | §12 | ✅ | Headless region-select in [`selection.rs`](../crates/aadt-app/src/selection.rs) (unit-tested); GUI `WP`/`CP`/`FENCE` tools (click corners → Space) **and free-drag lasso** — a curved Select drag becomes a lasso (window L→R / crossing R→L), a straight drag stays a box |
| M9.3 | Multi-mode grip editing (Move/Rotate/Scale/Mirror cycled with Space on a hot grip) | §14 | ✅ | `GripMode` (Stretch→Move→Rotate→Scale→Mirror) cycled with Space on a hot grip; each mode transforms the selection about the grip via the tested MOVE/ROTATE/SCALE/MIRROR/GRIPEDIT commands. Preview shows the base→cursor axis (grip ghost for Stretch) |
| M9.4 | Single auto-type `DIM` — `DIM <id>` infers the type from the entity (line→aligned, circle→diameter, arc→radius); coordinate form kept | §22 | ✅ | Command-level auto-type in [`aadt-command`](../crates/aadt-command/src/lib.rs) (headless-tested). GUI **hover-to-auto-dim**: DIM's first click on an entity auto-dimensions it with a live ghost preview (`auto_dimension_for` + `dimension_commands`), empty space keeps the two-point form. **DIMANGULAR from two lines**: pick two lines → the angle at their intersection (`line_line_intersection`), else the 3-point vertex form |
| M9.5 | Live hatch preview on internal-point pick | §21 | ✅ | Was boundary-entity-only. Now `hatch_boundary_at` detects the smallest enclosing closed region under the cursor (AutoCAD internal point); HATCH hovers show a translucent accent fill + boundary preview, click commits (boundary-entity pick kept as fallback) |
| M9.6 | Docking: floating panels + auto-hide | §28 | ☐ | Assess egui docking feasibility |
| M9.7 | Per-command capability trait (`CanUndo`/`SupportsPreview`/`SupportsSnap`/`SupportsDynamicInput`/`SupportsCommandLine`/`SupportsSelectionPreview`) | §33 | ✅ | `Capabilities` + `CommandDef::capabilities()` derive the §33 contract from each command's step interface (query set → `can_undo`); surfaced on hover in the command palette; contract test across the catalog |
| M9.8 | Per-command QA harness asserting the §34 checklist across every command | §34 | ✅ | `qa_checklist_every_command` iterates the whole catalog asserting starts / cancels / repeats (per `repeatable`) / keyboard-line addressable / snap+preview+mixed-input consistency / undo semantics; builds on the M9.7 capability contract |
| M9.9 | Frame-time / latency telemetry to validate the §31 performance budgets | §31 | ☐ | Currently unmeasured |

**§35 conflict (product lock):** the spec's "Future Extensions" list (AI command
suggestions, AI takeoff, PDF recognition, auto layer assignment) reintroduces the
AI removed in M4. Mostly **not scheduled** — resolve product direction first.
**Exception — auto layer assignment is now implemented** via a *local, offline,
deterministic* ML model: the embeddings vector DB (`aadt-embed` / `aadt-vecdb`)
maps each element to its AIA layer + model-space category, confidence-gated with
reconciliation (`MAPMETA`). This is a deliberate, bounded deviation from the M4
lock — see [ADR-0010](adr/0010-embeddings-metadata-mapping.md).

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
| M10.4 | Persistence: `.aadt` schema **v8** (objects/props/edges/audit tables) + VastuDB `.vdb` **v2** sections; carried across resave | ✅ | Additive; v7/v1 files still open |
| M10.5 | Surface: `OBJECTS` / `RELATIONS` commands + LISP (`aadt-objects`, `aadt-relations`) | ✅ | Promotion is deterministic and undo-safe (objects live beside the document) |

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

## M12 — AI-Native Phase 3: the AI service layer ✅ (core)

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

**Deliberately not in M12:** auto-fixing code findings, and any NL path that
reaches geometry without passing through the command band. Both would move a
decision from the engineer to the tool.

**Still open from the phase plan:** revision-comparison narration and
standard-detail recommendation (both retrieval-shaped, deferred to a later pass).

## M13 — AI-Native Phase 4: quantity takeoff, on objects ✅ (core)

Fourth implementation phase ([ADR-0014](adr/0014-object-based-takeoff.md)).
Reinstates what M3 removed, rebuilt against the Object DB — and revives
[ADR-0005](adr/0005-cost-code-standard.md) with it.

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

## Post-1.0

- M1 display: viewport text glyph quality; wgpu canvas if egui is too slow;
  transparent mid-command `'ZOOM` / grip-edit.
- M7 PDF viewer UI: interactive viewing, freehand/stamps, on-PDF measure, raster diff.
- Optional: WinUI over `aadt-ffi`.
- DWG: Autodesk-certified fidelity / hatch+dim DWG mapping; hatch arc/ellipse edges.
