# Changelog

## Unreleased

### Lock amended: raster placement is in scope, raster drafting is not

- The curated-command-band lock read "raster brushes / PNG block libraries are
  out of scope (vector-only)". Presentation furniture broke it: a PNG sofa is
  what an architect already has, and redrawing it as vectors buys nothing a
  plan uses. `Geometry::Image` plus PNG/JPEG/WebP entries under
  `libraries/elements/` are therefore supported and export as `IMAGE` +
  `IMAGEDEF`.
- The half that still holds is written down rather than left implied: **drafted
  geometry stays vector**, rasters are placed content only, and nothing snaps to
  or measures them. Raster brushes remain out of scope.
- `IMAGE` and `DEMO` added to the roadmap's command band; `libraries/README.md`
  documents the new `elements/` directory.

### Lint: clippy and rustfmt clean across the workspace

- `PERMS` / `OBJECTS` capability parsing dropped three redundant `trim()` calls
  before `split_whitespace()` (`clippy::trim_split_whitespace`).
- GUI shell: `clamp` in the overlay sizer, a needless borrow in the INSERT grip
  width path, a duplicated `PlaceKind` note branch, a collapsible `if` in Tab
  handling, and justified `too_many_arguments` allows on the two paint helpers.
- `cargo fmt --all` applied; `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -D warnings`, and the same with
  `--features gui` all pass.

### IMAGEDEF on DWG / DXF export

- Raster `Geometry::Image` exports as AutoCAD `IMAGE` + `IMAGEDEF` (+ reactor)
  via acadrust — no more LwPolyline footprint on DWG.
- `export_path` for `.dxf` with images uses the same CadDocument path so OBJECTS
  handles are valid; hatches/dims still prefer image-free DXF or `.aadt`.
- DWG import maps `RasterImage` back to `ImageRef`.
- Fidelity matrix and market-gap P3 updated.

### Market gaps implemented (P0–P2)

- **DWG fidelity matrix:** [docs/interop/dwg-fidelity-matrix.md](interop/dwg-fidelity-matrix.md);
  DWG export returns/prints skip warnings (hatches, dims, missing files).
- **Plot View presets:** Plot dialog Technical / Presentation; `place_drawing`
  omits hidden layers so sheets match the canvas preset.
- **Elements catalog tiles:** VEC/PNG/DYN place tiles; dynamic door sample in
  `libraries/elements/catalog.json`.
- **Dynamic Element grips:** selected INSERT grip 1 = flip (`BFLIP`), grip 2 =
  width (`WIDTH` via EATTEDIT).
- **`DEMO` / `AIDEMO`:** promote detection → Object/Graph summary → next steps
  (TAKEOFF / ASK).

### Fix: fill (hatch) delete + rectangle D-mode

- **Delete / ERASE** now removes entities, fills, and dimensions together (no
  early return after entity erase). Edge-clicks on a hatch boundary select the
  fill; box select includes fills; ERASE collecting accepts fill-only sets.
- **RECTANGLE** ignores clicks while typing D-mode length/width so dims are not
  discarded / wrong opposite-corner places.

### Display deep dive (drafting UI)

- Properties dock and status **Options** expose LWT + Technical / Presentation
  view presets with short guidance on lineweight scaling.

### Market gap case study (review)

- [docs/audit/2026-08-market-gap-case-study.md](audit/2026-08-market-gap-case-study.md)
  — competitor map, six gaps, scorecard, P0–P3 asks.

### Architectural workflow: Elements, typed Fills, PNG, DWG & dynamics

- **Elements** manager (formerly Blocks): catalog entries (`libraries/elements/catalog.json`),
  PNG/JPEG/WebP import into `libraries/elements/furniture/`, place via IMAGE tool.
- **Fills** manager (formerly Hatches): Technical / Floor / Presentation tabs;
  `FillKind` on hatch (persisted in `.aadt` schema v11); pattern heuristics
  (`AR-*`/`ANSI*` → Technical, `FLOOR-*` → Floor, `SOLID` → Presentation).
- **View presets:** Technical Plan vs Presentation (layer visibility + presentation fill alpha ~0.35).
- **Geometry::Image** for model-space raster furniture; texture paint in GUI;
  DXF/DWG export as rectangle footprint until IMAGEDEF exists.
- **Nested INSERTs** kept inside block definitions on DXF import; recursive expand
  on render; DWG export writes block defs + inserts.
- **Dynamic Elements:** `BFLIP`, WIDTH stretch on expand, visibility UI on selected
  INSERT; AutoCAD dyn-block actions remain non-editable (warning on import).

### Door / window place cuts the wall

- Placing **DOOR** / **WINDOW** (`DO` / `WI`) near a wall snaps the insert onto
  the wall, aligns rotation, and cuts a `WIDTH` opening (block default: 0.9 m
  single door, 1.2 m window).
- **Double-line walls** (parallel faces at **4½″** or **9″** when units are
  ft-in; **100 / 115 / 150 / 200 / 230 mm** when units are metres): both faces
  are trimmed and closed with jambs into **closed polylines** so hatch stays
  inside the wall body. Insert sits on the centerline between faces.
- Opening `WIDTH` follows drawing units (block params are metres; converted for
  ft-in). Switch via Units manager / `UNITS m` or `UNITS ft-in`.
- **Single centerline** walls: the line/polyline still splits into residual
  open pieces (same layer/attributes).
- Helper: `aadt_command::{place_opening_pose, cut_wall_opening}`.

### AutoCAD workflow parity (daily band)

- **Verb-noun select:** MOVE/COPY/ROTATE/SCALE/MIRROR/ARRAY*/ERASE collecting
  accept window/crossing boxes (add to set), not click-only.
- **MOVE:** empty Enter at destination uses the base as displacement from origin.
- **SCALE:** pick after base = factor from distance; `R` arms Reference properly.
- **OFFSET / FILLET / CIRCLE / TEXT:** remember last distance/radius/height;
  empty Enter accepts the default (FILLET default `0`).
- **EXTEND:** Quick mode by default (all boundaries), `[B]`oundary like TRIM's
  `[C]`; Shift+click trims.
- **Idle Backspace** no longer erases the selection (Delete still does).
- **RECTANGLE `D`** after the first corner keeps that corner.
- **CIRCLE `[D]`** works before the center; last radius on empty Enter after center.
- **STRETCH** only replays a *crossing* pre-select window (not L→R window).
- **TEXT:** insertion → height → value (height default from last / dim style).
- **ERASE** with a pre-selection erases immediately when started from the engine.
- Docs: [command-workflows.md](ux/command-workflows.md) aligned (ARC 3-point,
  POLYLINE `[U]/[C]`, FILLET Multiple-on).

### STRETCH: crossing window required; drag defines it

- STRETCH no longer falls back to moving the whole selection when there is no
  stretch window (click-preselect used to feel like MOVE). It asks for a
  crossing window; only endpoints **inside** that window move.
- Dragging while STRETCH is collecting defines the crossing window (always
  crossing / dashed green), matching the documented workflow.

### Selection: free-drag Select lasso removed

- Select drag is **box only** again (L→R window / R→L crossing). Free-drag
  curved lasso (`drag_is_lasso` / `finish_lasso`) is gone. Polygon and fence
  selection remain via GUI `WP` / `CP` / `FENCE` and headless
  `select_in_polygon` / `select_by_fence`. Docs: ROADMAP M9.2, engineering Vol 6.

### AI-native Phase 5 complete (M14): enterprise + APIs

- **M14.1** — `aadt-api` / `aadt serve`: loopback-only, token-gated HTTP/JSON
  over a live `Session`. Reads project the session; the one write path is
  `POST /v1/command` ([ADR-0015](adr/0015-local-api-layer.md)).
- **M14.2** — Object-level permissions: `--identity` / `--role` on serve;
  capability gates on routes and command verbs; `PERMS GRANT|REVOKE|SHOW` with
  per-object `acl.*` props.
- **M14.3** — Signed audit trail: optional HMAC-SHA256 chain on `AuditEntry`
  (`AADT_AUDIT_KEY`); `OBJECTS AUDIT VERIFY`; `.aadt` schema **v10** and
  `.vdb` format **v3**.
- **M14.4** — Enterprise connectors as command plugins: `CONNECTOR ERP|RFI|DMS`
  emit sidecar CSV/text/JSON (no second HTTP write door).
- **M14.5** — Site data ingest: `SITE INDEX` / `ASKDOCS INDEX PHOTO|PDF|REPORT`
  → corpus text chunks (captions + PDF literals); retrieval via `ASKDOCS`.
- Docs: [endpoint reference](api/README.md), [ADR-0016](adr/0016-m14-enterprise.md),
  ROADMAP M14 ✅.

### Transparent commands: `'ZOOM` and `'PAN` mid-command

- An apostrophe prefix now runs a view command **without cancelling the command
  already in progress** (AutoCAD's transparent-command convention). Typing
  `'ZOOM` while `LINE` waits for its next point reframes the view and hands the
  prompt straight back.
- The interrupted command's tool state, engine step position, and prompt are
  parked in a `Suspended` record and restored verbatim, so no picked points are
  lost. Esc during a transparent command resumes the interrupted prompt instead
  of cancelling everything.
- `'ZOOM E` / `'ZOOM P` / `'ZOOM <factor>` complete in one line; bare `'ZOOM`
  waits for a window pick and stays transparent until it gets one. Zoom factors
  accept AutoCAD's `2x` / `0.5X` form, and reject zero or non-finite values that
  would strand the camera.
- Only view commands are transparent — anything that could edit geometry is
  refused with a message rather than run non-transparently.
- Closes the last M1 display item that did not depend on the wgpu canvas, and
  the final non-locked "No" in the AutoCAD parity matrix.

### Documentation refresh for M9 / M12 completion

- Architecture overview gained the AI-native crate band (`aadt-object`,
  `aadt-graph`, `aadt-embed`, `aadt-vecdb`, `aadt-assist`, `aadt-takeoff`), a
  data-flow diagram that shows assist and retrieval alongside the deterministic
  spine, and the M9.6 panel-docking model.
- README documents the assist commands, the bundled-library layout, and the
  corrected `aadt-assist` responsibility; new [`libraries/README.md`](../libraries/README.md)
  indexes blocks / hatches / details / fonts.
- ROADMAP's curated command band now carries the Intelligence and Assist rows
  (`OBJECTS`/`RELATIONS`, `TAKEOFF`/`BOQ`, `FIND`/`ASKDOCS`, `SUGGEST`/`CHECK`/
  `ASK`, `COMPARE`, `RECOMMEND DETAIL`) and the full `OPEN`/`SAVE` format list.
- BricsCAD conformance audit re-audited after M9: §12/§13/§14/§17/§21/§22/§31
  moved to conforming, the gap backlog is closed, and the §35 AI conflict is
  recorded as resolved by ADR-0012/0013 with a bounded local-only scope.
- Industry audit refreshed to the current product (overall **A−**), with G8–G10
  added for docking / revision narration / detail recommendation and the
  withdrawn hosted-collaboration item removed.
- Command workflows gained Assist & retrieval and Panel docking sections; the
  parity matrix records the M9 items and drops the removed Foundry Local path.
- Vision doc now separates shipped capabilities from future ones; the ShilpiDB
  architecture map moves relationships and semantic services to built-subset;
  the July gap-closure plan is marked historical where M4/M3 superseded it.
- Residual `AiADT` naming replaced with `AAD` across the docs tree.

### M12.7 — Standard-detail recommendation

- Bundled detail sheets under `libraries/details/` (external cavity wall,
  internal partition, door head/threshold, habitable room floor finishes).
- `aadt-assist::recommend_details` BM25-searches an indexed corpus and emits
  advisory `SuggestionKind::DetailRecommend` proposals (`Action::None`).
- Session command **`RECOMMEND DETAIL [query]`** (aliases `DETAIL` / `DETAILS`)
  auto-seeds the bundled sheets, then cites matches. Empty query infers topics
  from the selection's layers. NL router maps "recommend a detail" → the same
  command.

### M12.6 — Revision-comparison narration

- New `aadt-assist::compare` service wraps the M7 fingerprint diff
  (`aadt_pdf::compare`) with detection count deltas (walls / rooms / openings /
  …) and optional BM25 citations for changed layers from an indexed corpus.
- Session command **`COMPARE <before-path>`** (aliases `DIFF`, `REVCOMPARE`)
  diffs a saved revision against the live drawing and registers one advisory
  `SuggestionKind::RevisionCompare` proposal (`Action::None` — rejectable /
  auditable, never auto-applied). CLI `aadt compare` prints the same enriched
  narration.
- Marks the deferred Phase-3 revision-narration item done.

### M9.6 — Floating panels + auto-hide (§28 docking)

- Left and right managers now support three presentation modes, persisted in
  `prefs.json`: **Docked** (edge `SidePanel`), **Floating** (free
  `egui::Window` with remembered position/size), and **Auto-hide** (edge rail
  that expands a foreground overlay on hover, collapses after a 350 ms grace).
- Panel headers expose Float / Auto-hide (unpin) / Pin (re-dock) / Collapse
  controls (Fluent pin + window icons vendored). Commands and chrome that open
  a manager (`LA`, `BLOCKS`, units chip, …) call `reveal_*_mgr` so they work
  in every mode.
- BricsCAD §28 marked conforming; M9 (Drafting Workflow Parity) is complete.
  Bottom-dock of the managers stays deferred — Drawing Properties already uses
  a bottom panel.

### Align on ShilpiDB's protocol: removed the parallel `aadt-vdb-proto`

- AADT talks to a `shilpid` server through `VdbClient` → `shilpi-client` →
  ShilpiDB's `shilpi-protocol`, and embeds the `shilpidb` engine (codec +
  `SpatialGrid`) for `.vdb` files. `aadt-vdb-proto` was a second, AADT-native
  wire protocol ("VDBP") scaffolded before `shilpi-client` existed; it was
  never wired into anything and duplicated ShilpiDB's own protocol. Removed
  the crate and its workspace member/dependency entries.
- Fixed the stale `aadt-vdb/lib.rs` comment: the codec is re-exported so the
  on-disk `.vdb` shares ShilpiDB's encoding; the wire protocol is
  `shilpi-protocol` (via `shilpi-client`/`VdbClient`), not this crate.
- [docs/plans/shilpidb-standalone-server.md](plans/shilpidb-standalone-server.md)
  marked superseded — the server shipped in the ShilpiDB repo itself
  (`shilpid`/`shilpi`/`shilpi-protocol`/`shilpi-client`) — and the planned
  `aadt-*` crate names are mapped to their real homes. New
  [docs/architecture/aadt-shilpidb-architecture.md](architecture/aadt-shilpidb-architecture.md)
  is the current developer-facing spec for how AADT and ShilpiDB fit together.
- `cargo fmt --all` applied workspace-wide to clear accumulated rustfmt drift
  (CI runs `cargo fmt --check`; local verification had relied on build/test/
  clippy only).

### New/Open no longer discard unsaved work unrecoverably

- Ctrl+N and Ctrl+O (and their toolbar buttons) replace the whole `Document` —
  undo history included — with no way back once they run. Both previously did
  this on a single keystroke/click with zero confirmation; now a dirty
  document raises the same **Save / Don't Save / Cancel** prompt already used
  on window-close (`draw_replace_confirm`) before either proceeds. This was
  the mechanism behind "deleted the whole drawing and UNDO couldn't bring it
  back" — UNDO had nothing to restore to because the old document (and its
  op log) was already gone.

### STRETCH: noun-verb pre-selection now replays the crossing window

- Pre-selecting objects with a rubber-band box, then invoking STRETCH (as
  opposed to drawing the crossing window inside the STRETCH prompt itself),
  discarded the box and always moved the whole selection — matching AutoCAD's
  fallback for *non*-box pre-selection, but not for a box one, where AutoCAD
  replays it as the stretch window. The app now remembers the last plain box
  selection's rectangle and reuses it for a noun-verb STRETCH, but only when
  re-running that box query still reproduces the live selection exactly
  (`selection_matches_box`) — a selection that changed since (a Shift-click,
  say) safely falls back to the old move-everything behavior instead of
  guessing.

### RECTANGLE: Tab after first value no longer breaks the command

- Typing a width then Tab (to enter height) was also handing Tab to egui, which
  moved keyboard focus into the command line / toolbar. Further digits landed in
  chrome instead of the Y field; Space could activate a focused control and
  abort or replace the drawing. Tab is now consumed while drafting, focus is
  surrendered, and Space on a half-finished X→Y pair asks for the second value
  instead of distance-entry / command dispatch.

### RECTANGLE: fixed dynamic-input second-value / opposite-corner entry

- The `D` (Dimensions) flow's length/width text entry was misclassified as
  raw point-coordinate entry (`expects_point_coords`). Once the width field
  had *any* input too (e.g. after pressing Tab), the two typed numbers were
  read back as an absolute X,Y point pick instead of length/width, silently
  discarding the width. Tab now also confirms-and-advances the current field
  like Space already did, instead of toggling an X/Y coordinate focus that
  doesn't apply in this mode.
- Separately, the plain opposite-corner prompt ("opposite corner or @W,H")
  read a typed `width Tab height` pair as an *absolute* world point rather
  than the promised width/height relative to the first corner — landing the
  corner nowhere near the first one. A bare typed pair there is now treated
  the same as `@W,H`.

### Layer manager: bulk Show all / Thaw all / Unlock all

- New "Bulk" row above the layer grid, scoped to whatever the active
  Scope/Discipline/search filter currently lists — turns every listed layer
  on, thawed, and unlocked in one action. Meant for getting an entire
  imported drawing (which lands on `ORG-DWG_LAYER-*` layers, off/frozen/
  locked at the source's discretion) back to a scalable, editable state
  without hunting down each layer individually.

### OVERKILL now cleans up every entity kind, not just LINE

- Previously only exact-duplicate `LINE` entities were detected. Extended to
  `POLYLINE` (direction-independent — a reversed copy still counts),
  `CIRCLE`, `ARC`, `TEXT`, and `INSERT`, all keyed per-layer with the same
  1e-6 quantization that already absorbed import/export float noise for
  lines.

### New annotation commands: TEXTBOX, TEXTCIRCLE, and a TCOUNT alias

- **`TEXTBOX target [margin]`** / **`TEXTCIRCLE target [margin]`** draw a
  rectangle or circle around each selected TEXT entity — a closed polyline or
  circle sized from the text's estimated glyph extents (same
  `chars × height × 0.6` width estimate already used by `aadt-embed`/
  `aadt-vdb`) and rotated with the text, with a default margin of a quarter
  of each text's own height.
- `AUTONUM` (bulk sequential TEXT numbering) now also answers to `TCOUNT`,
  AutoCAD's name for the same job.
- `MATCHPROP` and `OVERKILL` were already present (`essentials.rs`) —
  reported missing, but present and covered by tests; no action needed there
  beyond the OVERKILL extension above.

### ShilpiDB is now a standalone crate/repo AAD depends on

- Extracted the domain-agnostic storage engine — the little-endian binary codec
  and the uniform-grid spatial index — into a **separate public repository**,
  [`shilpidb`](https://github.com/HolagundiWorks/shilpidb) (zero dependencies,
  `#![forbid(unsafe_code)]`). `aadt-vdb` now depends on it via a pinned git
  dependency (`tag = "v0.1.1"`) and keeps only AAD's CAD-specific `.vdb` layout
  (the `Document`/Object-DB codec + the `ShilpiDb` wrapper).
- The engine codec is scalar-only; AAD adds the `Point2` codec as a `PointRead` /
  `PointWrite` extension trait. `SpatialGrid` is generic over `u64` ids +
  `shilpidb::Bbox`; `aadt-vdb` converts at the boundary (`query_index_bbox` for
  raw-index callers like `WITHIN` / LISP `aadt-within`). The `.vdb` file format
  is unchanged.

### Rename: VastuDB → ShilpiDB

- Renamed the vector database from **VastuDB** to **ShilpiDB** across the code
  (the `ShilpiDb` type), docs, and file-dialog labels. The file magic is now
  `SHILPIDB`; the pre-rename `VASTUDB\0` magic is **still accepted on read**, so
  existing `.vdb` files keep opening (guarded by a test). The neutral `aadt-vdb`
  crate name and the `.vdb` extension are unchanged. The standalone-server draft
  and its `shilpid` daemon / `shilpi` shell names were renamed to match.

### ShilpiDB persists the Object DB (M10.4 gap closed)

- `SAVE *.vdb` now writes the live **Object DB / Graph DB / audit log** into the
  `.vdb` v2 sections (via `save_path_with_objects`), and OPEN loads them back
  (`ObjState::sync` reads objects from `.vdb` as well as `.aadt`). Previously the
  wired `SAVE .vdb` wrote those sections empty, so objects never round-tripped.
- Proven end-to-end by a golden test: promote → `SAVE .vdb`, then in a **fresh
  process** OPEN + `OBJECTS LIST` shows the persisted objects and audit entries
  without re-promoting.

### Performance telemetry HUD (M9.9)

- **F7** toggles a performance HUD (top-right) with rolling **frame time**
  (avg / p95 / max), **FPS**, and a **build / paint** breakdown, graded live
  against the §31 redraw budget (**< 16 ms / 60 FPS** → OK / TIGHT / OVER).
- While the HUD is on, the shell drives continuous repaints so the numbers
  reflect a real sustained redraw loop rather than the idle throttle. Fixed-size
  `PerfRing` sampling (~2 s window); no allocation on the hot path.

### GUI auto-dimensioning (M9.4 complete)

- **Hover-to-auto-dim:** with the `DIM` tool, clicking an entity now
  auto-dimensions it — line/polyline → aligned, circle → diameter, arc → radius
  — with a live **ghost preview** on hover. Clicking empty space keeps the
  two-point coordinate form. Wires the command-level `DIM <id>` auto-type into
  the viewport (`auto_dimension_for` + `dimension_commands` are now public).
- **Angular from two lines:** `DIMANGULAR`'s first click on a line starts the
  two-line form — pick a second line and it dimensions the angle at their
  intersection (`line_line_intersection`); parallel lines are rejected. Clicking
  empty space still uses the 3-point vertex form.

### Dimension selection & erase (M9 drafting parity)

- Dimensions are now **selectable** in the viewport — click a dimension's drawn
  geometry to pick it, box/crossing and lasso to multi-select. Selected
  dimensions highlight in selection blue. Hit-testing and highlight run against
  exactly what the renderer draws (`aadt_render::dimension_commands`, now public).
- **Delete / `E`** erases selected dimensions in one undoable step via the new
  `Document::remove_dimension` (`Op::RemoveDimension`, index-based so a
  middle-of-table dimension round-trips through undo/redo).
- Restored the M9.2 free-drag **lasso**, which in-progress dimension-selection
  work had accidentally reverted.

### AI-Native Phase 4 (M13): quantity takeoff returns, built on the Object DB

Fourth implementation phase ([ADR-0014](adr/0014-object-based-takeoff.md)).
Reinstates the quantity engine removed in M3 — and with it
[ADR-0005](adr/0005-cost-code-standard.md), which had been marked superseded only
because takeoff was gone.

- **`aadt-takeoff` — the quantity engine over objects.** Wall length, thickness,
  gross/net area, volume, brick count and plaster area; room floor, ceiling,
  wall-finish and paint areas; opening schedule; structural members.
- **Opening deduction is a graph lookup, not a guess.** This is the whole reason
  takeoff waited for Phase 1. Raw geometry cannot say which door pierces which
  wall, so the M3 engine could measure a *gross* wall area honestly or guess
  deductions by proximity. The Graph DB records `Door —BelongsTo→ Wall`, so a
  net area is now derived. An opening the graph does not attribute is measured
  and scheduled but **deducted from nothing**, and the command says so —
  under-deducting is conservative and visible; guessing is neither.
- **CSI MasterFormat cost codes (ADR-0005, revived).** Mapped by object kind,
  refined by layer name — a drywall partition and a brick wall are both `Wall`
  objects but different trades, and billing them as one line would be wrong.
- **BOQ generation.** Quantities aggregated by cost code into a sorted bill.
  Zero-quantity lines are dropped rather than inviting a reader to price nothing.
- **Run persistence.** The `takeoff_*` tables that survived the M3 removal are
  re-activated as-is — no schema change. They key rows by integer id, which is
  what stable `ObjectId`s now supply and geometry never could.
- **Surface.** `TAKEOFF [WALLS|ROOMS|OPENINGS|SAVE]` (aliases `QTO`/
  `QUANTITIES`) and `BOQ` (alias `BILL`).

**Assumptions are labelled, not hidden.** AADT is 2D: a plan has no storey
height, so wall height is a config *input*. Openings without drawn dimensions
use defaults and are flagged; `TAKEOFF` reports how many figures were assumed
rather than measured, so an estimator confirms exactly those.

**Honest limitations.** Walls are assumed prismatic and of uniform thickness — no
lintels, sills, or returns. Brick count is a modular estimate with no wastage
allowance, not a bond-aware count. Room finishes do not deduct openings (walls
do). No pricing: the bill carries quantities and codes; rates belong to the
estimator's system.

### AI-Native Phase 3 (M12): the AI service layer — propose, never commit

Third implementation phase ([ADR-0013](adr/0013-ai-service-layer.md)). The first
services that want to *write* to the drawing, and the mechanism that stops them
doing it unsupervised.

- **`aadt-assist` — the gated suggestion framework.** A service returns
  `Suggestion` records carrying a confidence, a written rationale, and an
  `Action` that is **data, not a closure**. Services never receive a mutable
  document; one function (`apply`) commits, inside `Document::transact` (so an
  accepted suggestion is one undo step) and writes an append-only audit row.
  `reject` changes nothing but is *also* audited — a reviewer can tell a
  declined suggestion from one never raised.
- **Dimension prediction.** Proposes a linear dimension for every detected wall
  carrying none, with confidence derived from detection quality. Re-running
  skips walls that are already dimensioned.
- **Code-check rule packs.** Three deterministic geometric rules (minimum room
  area, minimum room width, minimum door width), graded violation/warning.
  Confidence is always `1.0` — a code check that guesses is worse than none.
  Findings are **advisory and never auto-fixed**: resolving a violation is a
  design decision. With a spec corpus indexed (Phase 2), each finding cites the
  governing passage.
- **Natural-language command routing.** `route` maps a phrase to a *command
  string* — never to geometry. The proposed command runs through the existing
  deterministic interpreter or not at all, so NL inherits its validation, undo,
  and audit. Unmatched phrasings are refused with candidates rather than guessed.
- **BM25 stemming.** Corpus tokens now strip a plural `s`, so "dimension"
  reaches a clause headed "Room Dimensions" — singular/plural pairs are
  everywhere in spec prose, and exact matching missed them.
- **Surface.** `SUGGEST DIMS|LIST|WHY id|ACCEPT id|ACCEPTALL|REJECT id`
  (alias `PROPOSE`), `CHECK [rule|RULES]` (aliases `CODECHECK`/`AUDIT-RULES`),
  and `ASK <phrase>` (alias `NL`), which shows the routed command without
  running it. Later extended with `COMPARE <before>` (aliases `DIFF`/
  `REVCOMPARE`) and `RECOMMEND DETAIL [query]` (aliases `DETAIL`/`DETAILS`) —
  see the M12.6 / M12.7 entries above.

**Honest limitations.** The NL router recognises a small explicit vocabulary and
refuses far more phrasings than it accepts — that is the intended failure mode,
not a gap. Dimension prediction proposes one dimension per wall, not dimension
strings or sheet layout. Three rules ship: the pack is a frame for adding more,
and passing `CHECK` is not a claim that a drawing is code-compliant.

### AI-Native Phase 2 (M11): semantic search + local retrieval

Second implementation phase of the AI-native platform (ADR-0012). Everything
stays local, offline, and deterministic — retrieval surfaces existing content
and cites it; nothing is generated or invented.

- **Text + aggregate embeddings (`aadt-embed`).** `text_embedding` embeds a
  string as a 64-dim character n-gram (2/3/4-gram) hash vector — no training,
  no dependencies, so `WALL` / `A-WALL` / `WALL-TYPE` cluster together.
  `drawing_embedding` / `object_embedding` mean-pool element vectors into a
  single drawing- or object-level vector.
- **Semantic search (`aadt-vecdb::search`).** `find_by_text` ranks elements by
  their label (layer name + annotation text); `find_similar_to` searches the
  geometric embedding space; `find_duplicates` reports congruent geometry by
  all-pairs cosine above a threshold.
- **Local corpus + RAG retrieval (`aadt-vecdb::corpus`).** Sidecar spec/standard
  text is chunked per paragraph and ranked with **BM25** over word tokens, so a
  passage wins on its distinctive terms rather than the boilerplate every clause
  shares; when no query term occurs at all, scoring falls back to n-gram cosine
  so morphological variants still match. Passages that match nothing are dropped
  rather than padding the answer.
- **Persistence.** `.aadt` schema **v8 → v9** (additive tables: `corpus_source`,
  `corpus_chunk`; carried across plain `SAVE`). v8 files migrate in place with
  geometry, objects, and audit intact.
- **Surface.** `FIND <text>|SIMILAR id|DUPES [threshold]` (aliases `SEARCH` /
  `SEMSEARCH`), which selects its hits; and `ASKDOCS <question>|INDEX path|
  SOURCES` (aliases `DOCS` / `RAG`), which answers only with cited passages.

**Honest limitation.** `ASKDOCS` is retrieval, not generation: it returns the
passages most relevant to a question, each with its source and paragraph number,
and never composes an answer. BM25 scores are unbounded and comparable only
within one result set, so they are shown as a rank plus a raw score, not a
percentage.

### AI-Native Phase 1 (M10): intelligent objects + Graph DB

First implementation phase of the AI-native platform (ADR-0012). Geometry stays
the source of truth; objects reference document entities, never replace them.

- **`aadt-object` — the Object DB.** Typed intelligent engineering objects
  (Wall / Room / Door / Window / Column / Beam / Furniture) with stable ids,
  member entities, engineering properties (length, thickness, area, height…),
  confidence, and revision stamp — plus an **append-only audit log** (user,
  timestamp, action, prev/new, AI-suggested, approval, reason; nothing deleted).
- **`aadt-graph` — the Graph DB.** Persisted typed relationships
  (`BelongsTo` / `Contains` / `Bounds` / `ConnectedTo` / `Supports`) with
  forward/reverse queries and BFS traversal, and a deterministic **promotion**
  engine that assembles objects + relations from `aadt-detect` output
  (Wall —Bounds→ Room, Room —Contains→ Column/Furniture, Door —BelongsTo→ Wall,
  Wall —ConnectedTo→ Wall).
- **Persistence.** `.aadt` schema **v7 → v8** (additive tables: `objects`,
  `object_entities`, `object_props`, `graph_edges`, `object_audit`; carried
  across plain `SAVE`) and ShilpiDB `.vdb` **v2** sections (v1 files still load).
- **Surface.** `OBJECTS [LIST|INFO id|AUDIT]` (aliases `OBJ`/`PROMOTE`) and
  `RELATIONS id` commands; LISP `(aadt-objects)` / `(aadt-relations id)`.

### AI-native platform direction adopted (docs)

- Added the canonical [AI-Native CAD Platform guidelines](vision/ai-native-platform.md):
  multi-layer data architecture (geometry / object / metadata / vector / graph /
  spatial stores), intelligent objects, semantic engine, AI services,
  construction intelligence, enterprise APIs — with "geometry is the source of
  truth" and "AI never owns data" as governing principles.
- [ADR-0012](adr/0012-ai-native-platform-pivot.md) records the pivot and which
  existing product locks each future phase supersedes (never silently).
- [Phased roadmap](plans/2026-07-ai-native-phases.md) maps the six stores onto
  the existing crates (ShilpiDB → Geometry DB + Spatial Index; `aadt-embed`/
  `aadt-vecdb` → Vector DB; `aadt-detect`/`aadt-standards` → Object DB seeds)
  and sequences Phases 1–6, starting with intelligent objects + a Graph DB.
- Docs only — no code changes; current locks govern until a phase ships.

### Rebrand → AAD (Accelerated Architectural Drafting)

- Renamed the product from **AADT** to **AAD — Accelerated Architectural
  Drafting** across the user-facing surface: desktop window titles, the app
  logo/wordmark and brand tooltip, file-dialog filter labels, the default
  title-block firm name, the CLI banner, README/NOTICE, and the branding /
  architecture / UX / roadmap docs.
- Internal identifiers are unchanged for compatibility: the `aadt-*` crates, the
  `aadt` binary, the `.aadt` file extension, and the `AADT-*` entity/command
  keys keep their names. Logo art files and historical ADRs were left as-is.

### ShilpiDB — custom vector database store (`.vdb`)

The drawing can now be **stored as a custom vector database, ShilpiDB** — a
purpose-built, self-contained on-disk format (`aadt-vdb`, extension `.vdb`) that
holds every entity as a native binary vector record with a **built-in spatial
index**, rather than as text payloads in SQLite. See
[ADR-0011](adr/0011-vector-database-store.md).

- **Native vector records** for all geometry (line/polyline/circle/arc/text/
  insert) plus layers, blocks, hatches, dimensions, floors/spaces, and properties
  — full-fidelity round-trip. Hand-rolled little-endian binary; no SQLite, no
  `serde`, no external crate, `unsafe`-free. Atomic temp-file-then-rename writes.
- **Built-in spatial index** (uniform grid over entity extents), persisted in the
  file: window/point queries are `O(cells + hits)` instead of a linear scan.
- **Used as storage + queried:** `OPEN`/`SAVE` recognize `.vdb` (CLI and app file
  dialogs). New `WITHIN x1 y1 x2 y2` command (aliases `VSELECT`/`VWINDOW`) selects
  entities in a window via the index; LISP `(aadt-within x1 y1 x2 y2)`. `.aadt`
  (SQLite) remains the default working format.

### Embeddings vector DB → predefined-metadata mapping (`MAPMETA`)

The drawing is now **stored as a vector database**: each element gets a geometric
embedding from a small **local ML model** and is mapped to its predefined AIA
standard layer **and** model-space category (Walls/Structural/Lintel/BeamLayout/
Furniture) from geometry alone — replacing the old layer-name-only heuristic. See
[ADR-0010](adr/0010-embeddings-metadata-mapping.md).

- **New crates.** `aadt-embed` (offline, deterministic MLP classifier + featurizer,
  trained at first use on seeded synthetic exemplars; no new runtime deps,
  `unsafe`-free) and `aadt-vecdb` (custom cosine-kNN vector store + mapping engine).
- **Confidence-gated.** Confident predictions are **auto-assigned** to layer +
  category (one undoable transaction); low-confidence elements are queued
  **pending** for reconciliation (`Accept` / `Set` to a class or layer). Manual
  decisions are preserved and re-applied across reopen.
- **`MAPMETA`** command (aliases `CLASSIFY` / `VECTORDB` / `VDB`):
  `[ALL] | RECONCILE | ACCEPT id | SET id <class|layer> | INFO id`.
- **LISP:** `(aadt-classify id)` returns an element's predicted layer;
  `(aadt-map-metadata)` auto-classifies the drawing.
- **Persistence:** `.aadt` schema **v6 → v7** (additive) — new `vec_meta` /
  `vec_embedding` tables hold each element's vector, predicted layer/category,
  confidence, and reconciliation status; carried forward across `SAVE` and
  migrated in place on open.

### Layer manager → AutoCAD Layer Properties grid

- Rebuilt the Layer Manager as an AutoCAD-style **aligned, striped column grid**
  (`egui::Grid`) with AutoCAD's column order — Status · Name · On · Freeze ·
  Lock · Color · Lineweight — replacing the wrapped card list.
- Current layer is marked with an accent ▸ and row highlight; **double-click a
  name sets it current**, single-click selects.
- Per-layer actions moved to a **right-click context menu** (Set current, Map to
  space ▸, Delete layer), decluttering the grid to match AutoCAD.
- Search box filters rows live.
- **Color picker:** clicking a layer's color swatch opens a *Select Color (ACI)*
  popup of the full 1–255 ACI palette (current index highlighted); choosing one
  restyles the layer via `LAYER <name> COLOR <aci>`. Widened the dock so the
  Color / Lineweight columns are visible without horizontal scrolling.

### AutoCAD command-workflow parity (in progress)

Bringing every command's interaction into line with AutoCAD workstyles
(keyword options, default flows, entry/repeat/cancel, live preview):

- **TRIM** now cuts **circles** (previously rejected — only lines, arcs, and
  polylines trimmed). The cutting edges split the circle; the span under the
  pick is removed and the complement is kept as an arc, matching AutoCAD. The
  hover ghost highlights the removed span too (`trim_circle` /
  `circle_removed_span`, unit-tested).
- **ARC** now defaults to **three points on the arc** (start → second → end),
  AutoCAD's default, with the sweep chosen to pass through the second point.
  `[C]enter` switches to the old centre → start → end flow. Live preview and
  prompts updated for both modes.
- **CIRCLE** prompt brackets fixed (`[2P/3P/D]iameter`); Center/2P/3P already
  supported (TTR still to come).
- **FILLET** now stays active after each corner (AutoCAD *Multiple*), retaining
  the radius, so you can fillet several corners without re-invoking (Esc ends) —
  matching the existing INSERT/OFFSET/COPY loop convention.

- **ROTATE / SCALE** gain the AutoCAD **`[C]opy`** option — transform a clone and
  keep the source. Command-level `ROTATE/SCALE … COPY` (headlessly tested); in
  the GUI, `C` arms/disarms copy for the next transform (prompt shows the state).

- **RECTANGLE** gains AutoCAD **`[C]hamfer`** and **`[F]illet`** corner options.
  Arm the mode (asks for the chamfer distance / fillet radius), then draw as
  usual — the rectangle is emitted as a closed polyline with cut or densified
  rounded corners (`rect_corner_polyline`, unit-tested). Distance is clamped to
  half the short side. (Per-corner *Width* still needs polyline width support.)

- **FILLET** gains the AutoCAD **`[T]rim`** toggle — No-trim keeps both source
  lines full-length and adds only the arc. Command-level `FILLET … NOTRIM`
  (headlessly tested); `T` toggles it in the GUI (persists across corners).

- **CIRCLE** gains **TTR** (tangent-tangent-radius): pick two tangent lines, type
  a radius, and the circle is placed in the quadrant the picks indicate.
  `tangent_circle_center` (unit-tested) solves the four offset-line candidates and
  keeps the one nearest the pick midpoint.

- **FILLET P** rounds **every corner of a polyline** at once (`FILLET P id
  radius`; in the GUI, a polyline first-pick applies it). `fillet_polyline_corners`
  eases each corner (collinear/too-tight corners degrade gracefully) and densifies
  the arcs; unit-tested on a closed square.

Audit note: COPY (multiple), OFFSET (multiple), ROTATE/SCALE (Reference + Copy),
MIRROR (keep/erase), INSERT (multiple), RECTANGLE (Dimensions + Chamfer + Fillet),
FILLET (Multiple + Trim + Polyline), CIRCLE (Center/2P/3P/TTR) all conform. The
one remaining item, per-entity polyline **Width**, needs a width attribute on the
geometry model (a foundational change, not a workflow tweak) — deferred.

### Per-command QA harness (M9.8)

- `qa_checklist_every_command` asserts the BricsCAD §34 checklist across the
  **whole command catalog**: each command starts (every alias resolves; the
  engine activates with a prompt), cancels (Esc → idle), repeats iff
  `repeatable`, is keyboard-line addressable, keeps capability/step consistency
  (snap ⇒ a spatial pick step; mixed ⇒ dynamic input), and has correct undo
  semantics (mutating vs inquiry). Builds on the M9.7 capability contract.

### Per-command capability contract (M9.7)

- New `Capabilities` type + `CommandDef::capabilities()` formalize the BricsCAD
  §33 developer contract — `can_undo`, `can_repeat`, `supports_preview`,
  `supports_snap`, `supports_dynamic_input`, `supports_command_line`,
  `supports_selection_preview`. Derived from each command's step interface (plus
  a read-only query set for `can_undo`) so it can't drift from behavior.
- Surfaced on hover in the command palette; a catalog-wide test asserts the
  contract (LINE draws/snaps/undoes, MOVE previews a selection, LIST is
  read-only, every command is command-line addressable).

### Live hatch preview + internal-point pick (M9.5)

- HATCH now works AutoCAD-style: **pick a point inside** a closed region (not
  just on the boundary line). `hatch_boundary_at` finds the smallest enclosing
  closed polyline/circle under the cursor (innermost wins).
- While the HATCH tool is active, the region under the cursor previews live —
  translucent accent fill + accent boundary outline — so you see what a click
  will fill. Clicking a boundary entity directly still works as a fallback.

### Multi-mode grip editing (M9.3)

- A hot grip now cycles edit modes with **Space** (AutoCAD): **Stretch → Move →
  Rotate → Scale → Mirror**. Stretch moves the picked grip; the others transform
  the whole selection about the grip as base point, dispatched through the
  existing (unit-tested) MOVE / ROTATE / SCALE / MIRROR commands so each stays a
  single undo. Scale takes its factor from the base→cursor grab captured when
  Scale mode is entered (falling back to the entity's grip spread). The rubber
  preview shows the base→cursor axis, plus a moved-grip ghost in Stretch.

### Fluent 2 conformance pass

Tightened the chrome against the [Fluent 2](https://fluent2.microsoft.design)
foundations (shapes already matched: None 0 / Small 2 / Medium 4 / Large 8 /
X-Large 12):

- **Typography** — Fluent "Strong" is Semibold 600, not Bold. Load Segoe UI
  Semibold (`seguisb.ttf`) as a dedicated `Semibold` family and render panel
  titles and section labels with it via `theme::semibold()` (egui `.strong()`
  only recolors).
- **Iconography** — glyphs moved to Fluent System Icons' UI step (16px in a 28px
  target; dense rows 14px) instead of the off-ramp 14/12px. Selected dock-rail
  tabs now render the **filled** icon variant (Fluent's selected-state cue),
  vendored for the nav set (layer/cube/grid/text-font/slide-text/document-text/
  ruler/history/options).
- **Elevation** — Windows Fluent outlines with strokes and keeps key shadows
  subtle: dialogs ≈ shadow28 (y8/blur20), menus & tooltips ≈ shadow8 (y4/blur8).
- **Motion** — hover/rest transitions at ~100ms (`durationFaster`).
- **Color** — status INFO uses the Fluent 2 brand blue `#0F6CBD` (was the
  Fluent 1 `#0078D4`); AAD orange stays the product brand.

### TRIM — AutoCAD-style drag-to-trim + hover ghost

- Freehand quick-trim: drag a stroke across objects and every entity the path
  crosses is trimmed at the crossing (Shift-drag extends). Plain click still
  trims a single pick.
- Hover ghost previews the exact segment a click would remove (translucent red);
  a red trail follows a drag stroke. Non-mutating
  `aadt_command::trim_removed_preview` backs the preview.

### Fix: uncovered gap beside the left dock (Blocks tab)

- The Blocks manager's tile grids used `horizontal_wrapped` inside an
  `auto_shrink([false, false])` scroll area, which under-wraps: tiles overflow
  the panel edge and force the `SidePanel` to reserve a much wider strip than it
  paints, leaving a large uncovered band (clear-color) between the dock and the
  viewport. Tile grids (Place / In drawing / Library) now chunk into a fixed
  column count derived from the real panel width — the same pattern the Hatches
  dock already used — so content wraps within the dock.
- Manager docks are now fixed-width (`exact_width`, non-resizable) so the
  reservation can never diverge from the painted frame; `lock_panel_width` caps
  content via `set_max_width` instead of forcing width with `set_width`.

### Hatch selection / edit / delete

- Hatches are now **selectable** in the viewport — click inside a fill to pick
  it (topmost wins; boundary edges still select the underlying entity). Selected
  hatches highlight in selection blue.
- **Delete** removes selected hatches (Del or `E`/ERASE), undoable as one step;
  new `Document::remove_hatch` with an index-based `Op::RemoveHatch` that
  round-trips a middle-of-table hatch (unlike the LIFO `AddHatch`).
- **Edit**: with a hatch selected, "Set … to <pattern>" in the Hatches dock
  retargets its pattern via the undoable `Document::set_hatch_pattern`
  (`Op::SetHatch`). Previously hatches could be created but never selected,
  edited, or removed (they live in their own table, apart from entities).

### Scope reset — drafting-first (2026-07)

- **Removed all AI features:** the `aadt-ai` crate, the `infer` command, the
  RECOGNIZE/REVIEW/ACCEPT/REJECT/FREEZE/ASK command family, and the Foundry
  Local control panel.
- **Dropped the takeoff module:** the `aadt-takeoff` crate, `TAKEOFF`/`SCHEDULE`
  commands, the `aadt takeoff` CLI, the GUI Takeoff/BOQ dock, `.xlsx`/CSV
  export, and the takeoff LISP queries. `aadt-store` keeps its (now unused) run
  tables.
- **Dropped cloud collaboration:** the `aadt-collab` crate and `aadt collab` CLI.
- **Rebranded** from "AI-Aided Architecture Design and Takeoff" to a 2D
  architectural drafting tool (README, NOTICE, branding, architecture overview;
  affected ADRs marked superseded).

### Object snaps (M9.1) ✅

- Added Quadrant, Nearest, and Intersection running snaps (AutoCAD-style
  priority), with markers and labels.
- **Perpendicular / Tangent** snaps and one-shot **snap overrides**
  (`END`/`MID`/`INT`/`CEN`/`QUA`/`NEA`/`PER`/`TAN`): type the keyword at a point
  prompt or pick it from the **SHIFT+right-click** menu. An override forces a
  single kind for the next pick — even with running osnap (F3) off — and derives
  PER/TAN from the live drag anchor. Headless geometry + `find_override_in_spaces`
  / `parse_snap_keyword` resolver in `osnap.rs`; PER/TAN markers in the viewport.

### Hatch pattern library

- Internalised 28 architectural/floor `.pat` patterns; `aadt-render` now parses
  and renders `.pat` line families faithfully (multi-family, native spacing).

### ROADMAP Next complete

- **DWG write:** `export_dwg_path` via `acadrust`; `SAVE` / Save-As accept `.dwg`
  (best-effort 2D subset; hatches/dims remain DXF-fidelity).
- **Multi-path hatch:** `Hatch.boundaries` (outer + islands); DXF `91`/`93`
  round-trip; `.aadt` `HATCHPATHS` codec.
- **Local collab presence:** `J`/`L`/`P` lines on the TCP relay (ephemeral).
- Roadmap Next marked done; Post-1.0 lists hosted session + display leftovers.

### ARRAYRECT / ARRAYPOLAR / ARRAYPATH

- Interactive rectangular, polar, and path arrays (`AR` / `ARRAYPOLAR` / `APATH`)
  with `SELECTION` support; legacy `ARRAY` aliases `ARRAYRECT`.

### Docs & asset hygiene

- Curated command band lists the array family; gap-closure marked complete.
- Removed unused logo/wordmark previews and dead chrome helpers.

### DXF nested-insert warning

- Nested `INSERT` inside a `BLOCK` is skipped with a fidelity warning
  (`ImportReport` + `[DXF: …]` on drawing comments); `OPEN` surfaces it.

### Branding (orange identity)

- Brand system: [docs/branding](branding/README.md) + [logo guidelines](branding/logo-guidelines.md).
- Primary accent `#E85D04` (replaces Windows blue) across Fluent chrome.
- Bold layered mark, app icons, favicon; window/taskbar icon wired via eframe.

### Gap closure (docs → plan → build)

- **Product locks:** paper space / XREF out of scope (nameplate PDF plotting);
  session `UNDO` ≠ file `HISTORY` (documented, not merged).
- **Docs:** Bluebeam case study, industry BOQ standards, gap-closure plan,
  ADR-0002 updated for open-source DWG (`acadrust`).
- **DWG:** `OPEN` / `import_path` / `import_dwg_path` via `acadrust` (Line, Arc,
  Circle, LwPolyline, Text, Insert, XLine subset).
- **Place tools:** seed `DOOR-SINGLE` / `WINDOW` / `COLUMN` / `BEAM` / `SLAB` blocks.
- **GUI AI:** Recognize / Review / Accept / Freeze on Takeoff panel.
- **Takeoff:** `csi_division` CSV column + `csi_division_rollups()`.
- **Store:** migrate-safe save test; History panel clarifies UNDO vs versions.

### Industry audit & docs hygiene (2026-07)

- Added [industry-standards audit](audit/2026-07-industry-standards-audit.md)
  (overall grade **B+** after gap-closure; intentional G3/G7 locks).
- Removed orphaned `docs/ROADMAP-PARALLEL.md`.
- **BPARAM / BVIS** block-definition edits are undoable (`Op::SetBlock`).

### HCW developer-guideline audit

Aligned CAD command mutations with HCW Developer Guidelines Track A
(transactions / undo, shared defaults, no half-modified drawings):

- **AUTODIM** — isolate + dimensions are one undoable transaction; `RESTORE` skips isolate.
- **DIMCONTINUE** — multi-point chains are one undoable transaction.
- **UNITS / DIMSTYLE / BPLTSTART** — property changes recorded via `Op::SetProperties` inside `transact`.
- **HCWLEGEND / BPLTLAYERS** — layer setup + legend text in one transaction.
- **Save** — drawing properties written inside the same SQLite transaction as geometry.
- **Default dim layer** — canonical `AN-DIMS` (HCW); `A-DIMS` / `A-ANNO-DIMS` still accepted as aliases.
- **Schema migrate** — load/save upgrade via scratch copy + atomic replace.
- **Shared helpers** — `parse_linetype`; named sheet/legend/room constants.
