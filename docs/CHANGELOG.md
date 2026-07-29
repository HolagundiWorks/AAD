# Changelog

## Unreleased

### AutoCAD command-workflow parity (in progress)

Bringing every command's interaction into line with AutoCAD workstyles
(keyword options, default flows, entry/repeat/cancel, live preview):

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

Audit note: COPY (multiple), OFFSET (multiple), ROTATE/SCALE (Reference + Copy),
MIRROR (keep/erase), INSERT (multiple), RECTANGLE (Dimensions + Chamfer + Fillet)
conform. Remaining gaps to schedule: CIRCLE TTR, FILLET Polyline/Trim, and
per-entity polyline Width.

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
  Fluent 1 `#0078D4`); AADT orange stays the product brand.

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
