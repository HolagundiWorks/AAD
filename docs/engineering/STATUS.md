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
| 4 | Geometry | 2D primitives + eps in `aadt-geometry` | Richer 2D curves if needed | 2D only; limited curve set | `aadt-geometry` |
| 5 | Entity DB | `.aadt` SQLite + `.vdb` ShilpiDB I/O; undo in RAM | Entity GUID, audit fields, DB transactions as SoT | GUI edits don’t round-trip every pick through SpatialGrid | `aadt-model`, `aadt-store`, `aadt-vdb` |
| 6 | Selection | Click, box W/C, WP/CP/FENCE; SpatialGrid click pick (L1) | SpatialGrid-assisted pick | Free-drag lasso not shipped; osnap still linear | `selection.rs`, `pick.rs`, `gui.rs` |
| 7 | Osnap | END/MID/CEN/QUA/INT/NEA + PER/TAN override; **OTRACK** H/V (±polar) | Full AutoCAD temporary tracking from tool anchor | Temporary ray from live base still missing | `osnap.rs`, `otrack.rs` |
| 8 | Modify | Full daily modify band via `Command::run` + `Tool` phases | — (band locked) | Firm suites line-only; no Tool for many cmds | `aadt-command`, `gui.rs` |
| 9 | Rendering | egui + FFI glyphs; optional wgpu (`gpu` / `wgpu-canvas`) | — | Criteria ADR-0020 (no default shell swap) | `aadt-render`, `gui.rs` |
| 10 | Undo | `Document::transact` / undo / redo | — | Nested txn forbidden; OPEN clears history; HISTORY ≠ undo | `aadt-model`, `aadt-store` |
| 11 | AI / semantic | Assist propose/apply; FIND/ASK/MAPMETA/…; M14 perms+connectors+site | — (2D plan CAD locked) | Assist never auto-commits (by design) | `aadt-assist`, `aadt-object`, `aadt-vecdb` |
| 12 | Plugin / API | `Registry::register`; `POST /v1/command`; LISP; M14.2–5 done | No entity POST | M14 complete ([ADR-0016](../adr/0016-m14-enterprise.md)) | `aadt-api`, `aadt-cli`, `aadt-lisp` |
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
4. **Select free-drag lasso** — **Not shipped.** Select drag is box-only; use
   `WP`/`CP`/`FENCE` for polygon/fence regions. Do not re-claim in ROADMAP.
5. **M14 enterprise** — Complete (ADR-0016); living docs match.

## Related roadmap locks

See [ROADMAP.md](../ROADMAP.md): curated command band (not AutoCAD command-count
chase); Assist never auto-commits; LISP for depth; transparent view commands only.

## Wave follow-ups (optional next)

| Item | Kind |
|------|------|
| **M22** WinUI gap close — ✅ (STRETCH/ARRAY/IMAGE/grips/swing) | Done — [ROADMAP](../ROADMAP.md) |
| **M21** WinUI honesty — ✅ (catalog/LINE/WALL/DOOR cut/ortho) | Done — [audit](../audit/2026-08-winui-doc-gap.md) |
| **M20** WinUI commands — ✅ (CLI/Elements/Fills/DIM/OTRACK/grips) | Done — [ROADMAP](../ROADMAP.md) |
| **M19** WinUI daily polish — ✅ (HATCH/TEXT/osnap/box) | Done — [ROADMAP](../ROADMAP.md) |
| **M18** WinUI usable CAD — ✅ (ABI v5 Layers/Blocks/tools) | Done — [ROADMAP](../ROADMAP.md) |
| **M17** WinUI Chrome 2.0 — ✅ (incl. M17.4 MSIX) | Done — [ROADMAP](../ROADMAP.md) |
| **L5** Viewport text glyphs — ✅ | Done — [ROADMAP](../ROADMAP.md) |
| **L5.2** Optional wgpu path — ✅ ADR-0020 | Done — [ROADMAP](../ROADMAP.md) |
| **L3.2** Freehand markups — ✅ | Done — [ROADMAP](../ROADMAP.md) |
| **L3.3** PDFium + raster diff — ✅ ADR-0019 | Done — [ROADMAP](../ROADMAP.md) |
| **L6** Sheet presets + material opacities — ✅ | Done — [ROADMAP](../ROADMAP.md) |
| **L7** INSERT rotation/scale polish — ✅ | Done — [ROADMAP](../ROADMAP.md) |
| SpatialGrid in GUI click pick | **Done (L1)** — osnap still linear |
| True OTRACK | **Done (L2)** — [ADR-0018](../adr/0018-otrack.md) |
| Entity GUID migration | ADR + code (Vol 5) |
| PR template from Vol 15 checklist | Process |

Done recently: M14–**M22**; **Later L1–L7** complete.
**Now:** optional polish (hatch paint, WP/CP/FENCE, Review). See [ROADMAP](../ROADMAP.md)
and [WinUI audit](../audit/2026-08-winui-doc-gap.md).
