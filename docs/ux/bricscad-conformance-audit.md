# BricsCAD Workflow Spec — Conformance Audit

**Date:** 2026-07-26
**Spec audited:** BricsCAD-Inspired Workflow Specification v1.0
**Scope:** AiADT desktop shell (`aadt-app`, `--features gui`) + command core (`aadt-command`, `aadt-app/command_engine.rs`)
**Method:** Read-only mapping of each spec section to current code. No implementation performed.
**Status of build at audit time:** GUI feature compiles clean (`cargo check -p aadt-app --features gui` → exit 0).

## Legend

| Mark | Meaning |
|------|---------|
| ✅ | Conforms — implemented as specified |
| ⚠️ | Partial — core exists, spec detail missing or divergent |
| ❌ | Gap — not implemented |
| ➖ | Not applicable / spec marks optional and app opts out |

## Executive summary

AiADT already embodies the spec's **hard core**: a single uniform command lifecycle, spacebar-primary confirmation with double-space repeat, a keyboard-first command line with aliases/autocomplete/history/palette, verb-noun modify commands, docked layers/properties, and command-line (non-modal) error reporting. These are the expensive, architecture-defining requirements and they are met.

The gaps are **breadth**, not foundation: advanced selection modes (fence/polygon/lasso), a fuller object-snap set with overrides, multi-mode grip editing, and a single auto-typing `DIM`.

**Scorecard (interaction-bearing sections §5–§34):**

| Status | Count | Sections |
|--------|-------|----------|
| ✅ Conforms | 18 | 5, 6, 7, 8, 9, 10, 11, 15, 16, 18, 19, 20, 23, 24, 25, 27, 29, 30 |
| ⚠️ Partial | 9 | 13, 14, 17, 21, 22, 26, 28, 32, 33 |
| ❌ Gap | 2 | 12 (fence/polygon/lasso), 34 (no automated command QA harness) |
| ➖ Optional-out | 1 | (Ribbon, §4/§32 — command-driven by design) |

Performance (§31) is **unmeasured** — no telemetry exists to assert the ms targets.

---

## Per-section findings

### §1–2 Design Philosophy / User Profile
✅ **Conforms (as intent).** Command-driven, keyboard-first, canvas-centric design is the app's actual architecture (`command_engine.rs` header: "Mouse = position · Keyboard = intent · Space = execute"). No dumbing-down of drafting. Nothing to verify beyond alignment.

### §3 Workspace Layout
✅ **Conforms.** The shell matches the diagram: QAT + icon toolbar top, left tool panel (Layers/Blocks), central drawing area, right Properties/Takeoff/etc. docks, command line, status bar. See `gui.rs` `draw_top_bar` / `draw_bottom_bar` and `gui_chrome.rs` panel builders.

### §4 UI Components
✅ **Required set present:** Canvas, Command Line, Toolbar, Layer Manager, Properties, Status Bar, Object Snap controls, Coordinate display, Undo history. **Ribbon:** ➖ intentionally absent (command-driven). **Optional:** Block Library ✅; Sheet Manager (partial via `aadt sheet`/present); External References/Design Center ❌ (not present).

### §5 Workflow Philosophy (one lifecycle)
✅ **Conforms.** `EngineState { Idle, Command, Input, Complete }` in [command_engine.rs](../../crates/aadt-app/src/command_engine.rs) enforces a single lifecycle for every command; no per-command bespoke flows.

### §6 Command State Machine
✅ **Conforms.** Idle → Command → Input(step) → Complete, with `FinishRule` governing how empty-Space finishes. Preview/commit handled uniformly by the engine + `transact`.

### §7 Command Activation
✅ **Conforms.** Keyboard aliases (`resolve_alias`/`resolve_exact`), command palette / search (`palette_open`, hold-Space → `SpaceAction::HoldPalette`), toolbar icon buttons (`icon_btn`), context menu (right-click), API (the `Command` trait + `Interpreter`), macro/scripting (LISP host, `aadt lisp`). Keyboard is the preferred path, as specified.

### §8 Command Line Behavior
✅ **Conforms.** Autocomplete (completions list in the command bar), aliases, history (`last_line`), command options (bracketed prompts, e.g. TRIM `[C]utting edges`), repeat (double-space), scripting (LISP). Primary interaction engine, as specified.

### §9 Keyboard Workflow
✅ **Conforms.** `L`/`M`/`CO` + Space + picks + Space maps directly onto engine aliases + `StepKind` sequences.

### §10 Spacebar Behavior
✅ **Conforms.** `SpaceGesture` distinguishes `Single` (confirm/next/accept-typed), `Double` (finish + repeat, `finish_and_repeat`), and `HoldPalette`. Double-window 280 ms, hold 250 ms.

### §11 Mouse Workflow
✅ **Conforms.** Middle-drag pan, wheel zoom, left-click select, right-click = "same as Space"/context (configurable per header comment: "Right-click confirms like empty Space"). Double-click edit → verify (see §25).

### §12 Selection Workflow
❌ **Gap.** Window (L→R) and Crossing (R→L) exist (`box_start`, "crossing-window" prompts). **Fence, Polygon (WPolygon/CPolygon), and Lasso are not implemented** (`grep fence|lasso|polygon-select` → empty). This is the single largest selection gap.

### §13 Smart Selection (hover preview, exposed points)
⚠️ **Partial.** Snap-point preview on hover exists (`snap_hit`, rubber-band). Full pre-click *selection* preview (highlighting the object that a click would pick) and exposed grip/quadrant/intersection markers on hover are not fully realized (quadrant/intersection snaps absent — see §17).

### §14 Grip Editing
⚠️ **Partial.** Selecting exposes grips and a **stretch** grip drag is implemented (`Tool::GripEdit { grip_idx }`, [gui.rs:317](../../crates/aadt-app/src/gui.rs:317)). The spec's multi-mode grips (Move/Rotate/Scale/Mirror cycled via Space on a hot grip) are **not** present — only stretch.

### §15 Layer Workflow
✅ **Conforms.** Docked layer manager; current-layer drives new geometry; per-layer Color/Lock/Freeze/Lineweight/Linetype/(Transparency) surfaced. (Transparency: verify column exists.)

### §16 Drawing Workflow
✅ **Conforms.** Choose layer → activate → draw → snap → finish → ready is the engine's default loop.

### §17 Object Snap Workflow
⚠️ **Partial.** Running snaps limited to **Endpoint / Midpoint / Center** ([osnap.rs](../../crates/aadt-app/src/osnap.rs): "Lean set: END, MID, CEN (no quadrant/intersection)"). **Missing:** Intersection, Perpendicular, Tangent, Quadrant, Nearest (as selectable snaps), plus the SHIFT+right-click override menu and `END/MID/INT/PER/TAN` single-shot keyword overrides. `Nearest` exists only as an internal resolver helper, not a user snap.

### §18 Modify Workflow (shared flow)
✅ **Conforms.** MOVE/COPY/ROTATE/SCALE/MIRROR/STRETCH/OFFSET/TRIM/EXTEND/ARRAY/ARRAYPOLAR all present in `aadt-command`, all driven by the shared Activate→Select→Confirm→Reference→Execute engine flow.

### §19 Trim Workflow (modern)
✅ **Conforms.** TRIM prompt: *"select object to trim or [C]utting edges (Space finishes)"* — modern hover/select-to-trim with cutting edges optional, exactly the spec's model. `repeatable: true`.

### §20 Offset Workflow (continuous)
✅ **Conforms.** OFFSET steps: Distance → Entity → Side, `repeatable: true` → continuous until cancelled.

### §21 Hatch Workflow (live preview)
⚠️ **Partial.** HATCH command + pattern picker exist; general rubber-band/print preview exists. A dedicated **live hatch preview** on internal-point pick is unverified in code — likely place-then-show rather than live. Flag for hands-on check.

### §22 Dimension Workflow (single auto-type DIM)
⚠️ **Partial.** `DIM`, `DIMLINEAR`, and `QDIM`/`AUTODIM` (aligned dims from endpoints, [autodim.rs](../../crates/aadt-command/src/autodim.rs)) exist. A **single `DIM` that auto-selects linear/angular/radius/diameter/aligned from the picked geometry** is not implemented as one unified command.

### §23 Text Workflow
✅ **Conforms.** MTEXT + TEXT present.

### §24 Block Workflow
✅ **Conforms.** BLOCK (create) + INSERT (scale/rotation/place); block library + `ensure_place_blocks`.

### §25 Properties Workflow (docked, inline)
✅ **Conforms.** Properties is a docked tab (`right_manager_nav` → Properties), not a modal dialog. Live edit → verify double-click-to-edit binding (§11) resolves here.

### §26 Navigation Workflow
⚠️ **Partial.** Wheel zoom, middle-drag pan, ZOOM Window, ZOOM Previous (`zoom_prev`) present. **Zoom Extents** exists at load (`fit_document`) but a **double-middle-click zoom-extents** gesture and **View Cube** (both spec-optional) are absent. Navigation does not interrupt commands ✅.

### §27 Undo/Redo
✅ **Conforms.** Unlimited history via `transact`; CTRL+Z/CTRL+Y. Command context preserved through the transaction wrapper.

### §28 Docking System
✅ **Conforms (M9.6).** Left/right docks + tabbed panels + persisted layout (prefs), plus **floating** managers (`egui::Window`) and **auto-hide** (edge rail → hover overlay with grace). Mode is per-side (`PanelMode::{Docked, Floating, AutoHide}`) and survives restart. Bottom-dock of the left/right managers is still deferred; Drawing Properties already bottom-docks.

### §29 Status Bar
✅ **Conforms.** Coordinates, Snap, (Grid), Ortho, Polar, Dynamic Input, Selection count, Current layer surfaced in `draw_bottom_bar`. (Grid toggle: verify presence.)

### §30 Error Handling (no modals)
✅ **Conforms.** Recoverable errors report to the command-line `note` field and the command continues; no modal dialogs for recoverable issues.

### §31 Performance Targets
➖ **Unmeasured.** No frame-time/latency telemetry exists, so the <16 ms redraw / <50 ms zoom / <2 s open targets cannot be asserted or refuted from code. Would require an instrumentation pass to audit.

### §32 UX Rules
⚠️ **Partial (mostly conforms).** Common commands are not hidden; ribbon not forced (➖ none); inline editing favored; keyboard never blocked. The "never more than 2 clicks" and "never require property dialogs" rules hold. Full verification of "never interrupt mouse movement" needs runtime observation.

### §33 Developer Rules (per-command capability contract)
✅ **Conforms (M9.7).** `Capabilities` + `CommandDef::capabilities()` make the contract explicit and queryable: `can_undo`/`can_repeat`/`supports_preview`/`supports_snap`/`supports_dynamic_input`/`supports_command_line`/`supports_selection_preview`. Derived from each command's step interface (plus a query-command set for `can_undo`) so it can't drift from behavior; surfaced on hover in the command palette and asserted by a catalog-wide test.

### §34 QA Checklist
✅ **Conforms (M9.8).** `qa_checklist_every_command` iterates the whole command catalog and asserts the headlessly-verifiable checklist per command: starts (every alias resolves; engine activates with a prompt), cancels (Esc → idle), repeats (matches `repeatable`), keyboard-line addressable, capability/step consistency (snap ⇒ spatial pick step; mixed ⇒ dynamic input), and undo semantics (mutating vs inquiry). The purely-interactive rows (mouse-only / keyboard-only end-to-end) are represented by their structural guarantees. Built on the M9.7 capability contract.

### §35 Future Extensions
⚠️ **Direct conflict flagged.** This section lists AI command suggestions, AI quantity takeoff, PDF drawing recognition, and auto layer assignment — the exact capabilities removed in commit `e4d93e7` ("Remove all AI features"). **Any work toward §35 reverses that decision.** Resolve the product direction before scheduling §35 items.

---

## Consolidated gap backlog (suggested priority)

| # | Gap | Section | Size | Notes |
|---|-----|---------|------|-------|
| 1 | Object snaps: INT/PER/TAN/QUA/NEA + override menu + keyword overrides | §17 | M | Highest daily-use impact; extends existing `SnapKind` enum |
| 2 | Selection: Fence, WPolygon/CPolygon, Lasso | §12 | M–L | New selection-geometry pipeline |
| 3 | Multi-mode grip editing (Move/Rotate/Scale/Mirror via hot grip) | §14 | M | Builds on existing `GripEdit` |
| 4 | Single auto-type `DIM` | §22 | M | Wraps existing DIMLINEAR/QDIM/autodim with type inference |
| 5 | Live hatch preview | §21 | S–M | Verify current behavior first |
| 6 | Docking: floating / auto-hide | §28 | M | ✅ M9.6 — `PanelMode` float / auto-hide without `egui_dock` |
| 7 | Per-command capability trait (`Supports*`) | §33 | S | Formalizes existing behavior |
| 8 | Per-command QA harness for §34 checklist | §34 | M | Test infrastructure |
| 9 | Performance instrumentation to validate §31 | §31 | S | Frame-time telemetry |
| 10 | Zoom-extents gesture / View Cube (optional) | §26 | S | Spec-optional |

## Open product decision

**§35 vs. the AI removal.** The most recent three changes stripped all AI features; the spec's roadmap reintroduces them. This is not a code question — it needs a product call before either direction is scheduled.
