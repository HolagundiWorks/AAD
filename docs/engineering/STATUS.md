# Engineering handbook — plan vs existing

Cross-cutting status for all 15 volumes. Update this file when a gap closes or a
target changes. Full prose lives in each `vol-*.md` (Wave 1 + Wave 2 complete).

Legend: **Current** = shipped in tree · **Target** = ADR / ROADMAP / ShilpiDB arch ·
**Gap** = delta engineers must not invent around.

| Vol | Topic | Current | Target | Gap | Primary crates |
|-----|-------|---------|--------|-----|----------------|
| 1 | Architecture | Layered crates; `Document` in `Session` is live SoT | ShilpiDB as SoT; CAD as editor only | Dual narrative: RAM document vs DB-first design | `aadt-app`, `aadt-model`, `aadt-command` |
| 2 | Command engine | `Registry` + `Interpreter`; GUI `CommandEngine` + `Tool` | One interactive + headless catalog | Two catalogs (`CATALOG` ⊄ full `Registry`) | `aadt-command`, `command_engine.rs` |
| 3 | Input & events | Space-primary; dyn input; F-keys; GUI transparent ZOOM/PAN | Scriptable apostrophe suspend | Transparent cmds GUI-only | `gui.rs`, `command_engine.rs` |
| 4 | Geometry | 2D primitives + eps in `aadt-geometry` | Splines/meshes/solids (Phase 6) | No 3D; limited curve set | `aadt-geometry` |
| 5 | Entity DB | `.aadt` SQLite + `.vdb` ShilpiDB I/O; undo in RAM | Entity GUID, audit fields, DB transactions as SoT | GUI edits don’t round-trip every pick through SpatialGrid | `aadt-model`, `aadt-store`, `aadt-vdb` |
| 6 | Selection | Click, box W/C, WP/CP/FENCE, free-drag lasso | SpatialGrid-assisted pick | Lasso restored (`drag_is_lasso` + `finish_lasso`); GUI pick still linear | `selection.rs`, `gui.rs` |
| 7 | Osnap | END/MID/CEN/QUA/INT/NEA + PER/TAN override | Full OTRACK alignment paths | F11 toggles prefs; no true tracking rays | `osnap.rs` |
| 8 | Modify | Full daily modify band via `Command::run` + `Tool` phases | — (band locked) | Firm suites line-only; no Tool for many cmds | `aadt-command`, `gui.rs` |
| 9 | Rendering | egui `Painter` over `DrawList`; optional wgpu | Viewport text glyphs; wgpu if needed | Display-bound polish | `aadt-render`, `gui.rs` |
| 10 | Undo | `Document::transact` / undo / redo | — | Nested txn forbidden; OPEN clears history; HISTORY ≠ undo | `aadt-model`, `aadt-store` |
| 11 | AI / semantic | Assist propose/apply; FIND/ASK/MAPMETA/… | M14.2+ permissions, ERP, site ingest | Assist never auto-commits (by design) | `aadt-assist`, `aadt-object`, `aadt-vecdb` |
| 12 | Plugin / API | `Registry::register`; `POST /v1/command`; LISP | No entity POST; later plugin ABI | M14.1 done; later milestones open | `aadt-api`, `aadt-cli`, `aadt-lisp` |
| 13 | Perf / spatial | Drawlist cache; `SpatialGrid` in `.vdb` | Wire grid into GUI pick/osnap; R-tree option | Pick/osnap still linear over active space | `aadt-vdb`, `aadt-app` |
| 14 | Testing | Headless command + model tests; goldens | Broader GUI contracts | egui shell not headless-verified | crate `tests/`, `aadt-command` |
| 15 | Coding standards | feed-only mutation; curated band; AAD Chrome | Documented in this handbook | Enforce via review, not CI lints yet | this handbook + ADRs |

## Highlighted contradictions (do not ignore)

1. **ShilpiDB architecture doc** says “no monolithic drawing document.” **Code**
   uses `aadt_model::Document` as the interactive SoT. Persist to `.aadt` / `.vdb`
   on OPEN/SAVE.
2. **Transparent `'ZOOM`/`'PAN`** — GUI done; headless scripts still one-shot
   (`nav_hint` only). UX research updated.
3. **Two command catalogs** — interactive `command_engine::CATALOG` vs full
   `aadt_command::Registry`. Firm / Assist / file I/O are mostly line-only.
4. **Select free-drag lasso** — **Restored in code** (`drag_is_lasso` / `finish_lasso`). Living docs match.

## Related roadmap locks

See [ROADMAP.md](../ROADMAP.md): curated command band (not AutoCAD command-count
chase); Assist never auto-commits; LISP for depth; transparent view commands only.

## Wave follow-ups (optional next)

| Item | Kind |
|------|------|
| Wire `SpatialGrid` into GUI pick/osnap | Code (Vol 13) |
| True OTRACK (reclaim or redefine F11) | Code (Vol 7) |
| Free-drag Select lasso | Done (optional polish only) |
| Entity GUID migration | ADR + code (Vol 5) |
| PR template from Vol 15 checklist | Process |
