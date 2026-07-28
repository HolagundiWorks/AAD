# Changelog

## Unreleased

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
