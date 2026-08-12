# AAD Roadmap

Lean **2D architectural drafting**: keyboard-first CAD, DXF/DWG (incl. IMAGEDEF),
PDF sheets, LISP, Elements/Fills/wall openings, object takeoff, assistive AI, and
a local enterprise API. Each milestone is a shippable, verifiable increment.

Long-term direction (still **2D**): the
[AI-Native CAD Platform](vision/ai-native-platform.md) —
[ADR-0012](adr/0012-ai-native-platform-pivot.md),
[phased plan](plans/2026-07-ai-native-phases.md). Product locks below govern what
ships.

Milestone numbers jump (**M3 / M4 / M8** never shipped / retired). The 1.0 band
is **M1–M2, M5–M7, M9–M15**; **M16+** is post-1.0 shell work.

---

## How to follow this roadmap

| | Milestone | Exit |
|---|-----------|------|
| **Now** | — | M23 optional backlog complete |
| **Next** | — | Full Annotate/Review/PDF docks · IMAGE bitmap blit |
| **Done** | M1–M2, M5–M7, M9–**M23**; **L1–L7** | 1.0 + WinUI through optional gap close |

Pick from **Later** top-down. Do not revive BIM / 3D or tombstone milestones.

---

## Product locks

- **2D CAD only — permanent.** Plan/elevation/section in the plane. No solids,
  meshes, orbit modeling, BIM authoring, or IFC. Wall height / volume in takeoff
  are **attributes on 2D geometry**, not a 3D scene.
- **Lean commands.** Daily band only — not an AutoCAD command-count chase. See
  [Curated command band](#curated-command-band).
- **LISP for depth.** Firm automation via AutoLISP-flavoured scripts
  (`aadt-*`). No second scripting language. (Spec §10 deferred LISP to M6 — ✅.)
- **WinUI 3 shell (shipping path).** C++/WinRT + Fluent 2 + Direct2D under
  [`native/aad-winui/`](../native/aad-winui/); Rust engine via `aadt-ffi`
  ([ADR-0017](adr/0017-winui-shell-rust-engine.md)). egui (`aadt app`) is
  **maintenance fallback** only.
- **Deterministic core.** Geometry, numbering, and quantities are reproducible
  and auditable **independent of AI**. Local, on-demand AI only — never
  background. Offline drafting / detection / takeoff / PDF markup required.
- **No cloud collaboration product.** Offline-first; enterprise reach is the
  local API + connectors (M14), not multi-user sync.
- **DWG/DXF:** open-source `acadrust` 2D subset
  ([ADR-0002](adr/0002-dwg-dxf-strategy.md)). Paper space / XREF / VP out of
  scope. **UNDO ≠ HISTORY** — keep both.

---

## Curated command band

Depth goes into LISP, not more command names. Interaction is AutoCAD-like —
full UX notes in [`docs/ux/`](ux/README.md).

| Kind | Commands |
|---|---|
| Draw | `LINE` `POLYLINE` `CIRCLE` `ARC` `RECTANGLE` `XLINE` `TEXT` `INSERT` |
| Edit | `MOVE` `COPY` `ROTATE` `SCALE` `MIRROR` `OFFSET` `TRIM` `EXTEND` `FILLET` `STRETCH` `ARRAY*` `ERASE` |
| Org | `LAYER`/`LAYERS` `LAYERSET` `SELECT` `UNDO`/`REDO` `DIST` `LIST` `OPEN` `SAVE` `UNITS` |
| Annotate | `TEXT` `MLEADER` `DIM*` `DIMSTYLE` `REVCLOUD` |
| Arch | `WALL` `DOOR`/`WINDOW` `HATCH`/Fills `IMAGE` Elements |
| AAD / Intel / Assist / Power | `SECTION` `STANDARDS` `OBJECTS` `TAKEOFF`/`BOQ` `FIND` `ASK*` `LISP` … |

Assist commands **propose only** — never auto-commit geometry ([ADR-0013](adr/0013-ai-service-layer.md)).
Do not add: MTEXT sprawl, ribbon forests, 3D/BIM, paper-space XREF.

---

## Done (1.0 + M16)

### M1 — CAD core ✅

Headless: undo/redo, curated interpreter, selection, dims, hatch, layers, DXF
round-trip, drawlist. Native `.aadt` (ADR-0004). Floors + category model spaces
([ADR-0009](adr/0009-floors-and-model-spaces.md)): `FLOOR` / `CATEGORY` /
`ORIGIN` / `TRACE`. Transparent `'ZOOM`/`'PAN` in egui.

**Interactive:** egui fallback ✅ (full click-tools, dyn input, Ortho/osnap).
WinUI / Direct2D interim host + `aadt-ffi` ✅ for feed + M16 tools — full Fluent
shell is **M17**. Optional wgpu canvas criteria: [ADR-0020](adr/0020-wgpu-canvas-criteria.md).

### M2 — Detection ✅

Openings / structural / furniture classification; rooms; tags; anomalies.
On-canvas block-placement polish → Later.

### M5 — Sheets / plot ✅

`PLOTSETUP`, title block, notes, QA, `place_drawing`, `AADT-PRESENT`. Hatch
library: 28 `.pat` patterns under [`libraries/hatches/`](../libraries/hatches/)
(`aadt-render` multi-family parse).

### M6 — LISP ✅

AutoLISP-flavoured interpreter + `aadt-*` bridge; `LISP` / bare `(…)` on Session.

### M7 — PDF markup ✅ (core)

Vector plot, markup list, narrated compare. Interactive PDF viewer UI → Later.

### M9 — BricsCAD drafting parity ✅

| # | Item | Status |
|---|------|--------|
| M9.1 | Osnap QUA/NEA/INT + PER/TAN overrides + snap menu | ✅ |
| M9.2 | Fence / WPolygon / CPolygon (box select only for drag) | ✅ |
| M9.3 | Multi-mode grip edit | ✅ |
| M9.4 | Auto-type `DIM` + hover-to-dim | ✅ |
| M9.5 | Hatch internal-point preview | ✅ |
| M9.6 | Floating + auto-hide docks | ✅ |
| M9.7–9 | Capabilities · QA harness · F7 perf HUD | ✅ |

Cloud generative “AI suggestions” **not scheduled**. Auto layer via `MAPMETA`
only ([ADR-0010](adr/0010-embeddings-metadata-mapping.md)).

### M10 — Objects + graph ✅

`aadt-object` / `aadt-graph`; `.aadt` v8 + `.vdb` v2; `OBJECTS` / `RELATIONS`.
ShilpiDB: 2D vectors only ([ADR-0011](adr/0011-vector-database-store.md)).
Merkle version history in `.vdb` v4 ([ADR-0021](adr/0021-shilpidb-merkle-versioning.md)).

### M11 — Semantic search ✅

Embeddings, `FIND`, BM25 `ASKDOCS`, corpus in `.aadt` v9. No answer generation.

### M12 — Assist layer ✅

Suggest / check / ask / compare / detail recommend — propose + audited apply only.

### M13 — Object takeoff ✅

Quantities + CSI codes + `TAKEOFF` / `BOQ`. No pricing.

### M14 — Enterprise API ✅

Loopback `aadt serve`; perms; signed audit; connectors; site ingest
([ADR-0015](adr/0015-local-api-layer.md), [ADR-0016](adr/0016-m14-enterprise.md)).
No `POST /v1/entities`.

### M15 — Arch plan CAD ✅

Elements / typed Fills; dual-face openings; door swing; PNG furniture;
Technical/Presentation presets; IMAGEDEF export; interaction trust; `DEMO`.
DWG write for IMAGE+IMAGEDEF ✅; hatch/dim DWG write → Later.
([market-gap case study](audit/2026-08-market-gap-case-study.md),
[fidelity matrix](interop/dwg-fidelity-matrix.md)).

### M16 — WinUI interactive tools ✅

[ADR-0017](adr/0017-winui-shell-rust-engine.md): click-draw via FFI, not C++ geometry.

| # | Item | Status | Exit |
|---|------|--------|------|
| M16.1 | `aadt_app::command_engine` outside `gui` | ✅ | Shared catalog with egui |
| M16.2 | ABI v3 `aadt_tool_*` + preview | ✅ | `aadt_ffi_version() == 3` + tests |
| M16.3 | Host uses FFI tools (no C++ `PickTool`) | ✅ | L / C / REC + rubber-band |
| M16.4 | Docs match | ✅ | ROADMAP / CHANGELOG / ADR |

Not in M16: polyline chain, osnap, modify tools, MSIX / real WinUI 3 (→ M17).

---

## Active

### M17 — WinUI Chrome 2.0 ✅

Product Fluent shell on top of M16 FFI under
[`native/aad-winui/winui/`](../native/aad-winui/winui/).

| # | Item | Status | Exit |
|---|------|--------|------|
| M17.1 | VS Blank App (WinUI 3 C++) + Windows App SDK | ✅ | `native/aad-winui/winui/AadWinui.sln` builds (VS 2022 + WinAppSDK 1.6) |
| M17.2 | `SwapChainPanel` (or composition) + existing Direct2D / `FfiSession` | ✅ | Paper paints via ABI v3 (`SwapChainCanvas`) |
| M17.3 | Chrome 2.0: brand strip, CLI hero, status, auto-hide panes, Properties primary | ✅ | Rails + Properties (ABI v4 pick/props); QAT Undo/Redo |
| M17.4 | Optional MSIX packaging | ✅ | Release MSIX + `pack-msix.ps1`; Debug stays unpackaged |

egui stays maintenance fallback.

---

### M18 — WinUI usable CAD ✅

Daily drafting chrome on the M17 shell via **ABI v5** (`aadt-ffi`).

| # | Item | Status | Exit |
|---|------|--------|------|
| M18.1 | Layers / blocks list FFI + current layer | ✅ | `aadt_layers_list` / `aadt_blocks_list` / `aadt_layer_set_current` |
| M18.2 | Expand interactive tools (draw + modify) | ✅ | LINE/PL/REC/CIRCLE/ARC/XL + MOVE/COPY/ROTATE/SCALE/MIRROR/OFFSET/TRIM/EXTEND/FILLET/ERASE + INSERT |
| M18.3 | WinUI Layers/Blocks panes, tool strip, New/Open/Save | ✅ | Live lists; Comdlg file pickers; Space finishes |
| M18.4 | Properties layer edit + Tools catalog | ✅ | `CHANGE SELECTION LAYER`; Tools… palette |

Out of M18 (still egui): Fills/Annotate/Review/PDF managers, OTRACK overlays, grip edit.

---

### M19 — WinUI daily polish ✅

| # | Item | Status | Exit |
|---|------|--------|------|
| M19.1 | HATCH + TEXT interactive tools | ✅ | Tool strip + FFI |
| M19.2 | Osnap on tool picks (marker in preview) | ✅ | ABI v6 `aadt_osnap_*` |
| M19.3 | Window / crossing box select | ✅ | Drag L→R window, R→L crossing |

Still egui: Fills/Annotate/Review/PDF managers, OTRACK overlays, grip edit.

---

### M20 — WinUI commands + Elements/Fills ✅

Daily command-line CAD on the M19 shell via **ABI v7**.

| # | Item | Status | Exit |
|---|------|--------|------|
| M20.1 | Command line polish | ✅ | complete / history / is_interactive / empty Enter repeat |
| M20.2 | Elements + Fills panes | ✅ | catalog + hatch pattern ABI; Blk→Elm; Fil tab |
| M20.3 | Annotate / place tools | ✅ | DIM/MLEADER/DIST + DOOR/WINDOW place→INSERT |
| M20.4 | OTRACK rays + stretch grips | ✅ | overlay via preview; grip drag → GRIPEDIT |

Still egui (pre-M22): full Annotate/Review/PDF docks. STRETCH/ARRAY/swing → M22.

---

### M21 — WinUI honesty + daily CAD (audit) ✅

Closes doc/code gaps from [2026-08 WinUI audit](audit/2026-08-winui-doc-gap.md).
**ABI v8.**

| # | Item | Status | Exit |
|---|------|--------|------|
| M21.1 | Honest Tools catalog | ✅ | `aadt_catalog_tools` = interactive only |
| M21.2 | Selection highlight | ✅ | selection line export + D2D accent |
| M21.3 | LINE chain until Space | ✅ | segment commit, continue |
| M21.4 | DOOR/WINDOW wall cut | ✅ | `cut_wall_opening` on place |
| M21.5 | WALL interactive | ✅ | thickness + polyline → A-WALL |
| M21.6 | FILLET radius + Ortho/Polar | ✅ | typed radius; F8/F10 |

Still egui after M21: STRETCH/ARRAY (→ M22), Review/PDF, WP/CP/FENCE.

---

### M22 — WinUI remaining gap close ✅

Closes the post-M21 audit backlog on the shipping shell. **ABI v9.**

| # | Item | Status | Exit |
|---|------|--------|------|
| M22.1 | STRETCH interactive | ✅ | window → base → dest |
| M22.2 | ARRAYRECT / POLAR / PATH | ✅ | select + points / count |
| M22.3 | IMAGE place | ✅ | file picker + `aadt_tool_start_image` |
| M22.4 | REVCLOUD | ✅ | two-corner cloud |
| M22.5 | Multi-mode grips | ✅ | Space cycle Stretch→…→Mirror |
| M22.6 | Door swing + Shift pick | ✅ | Tab flip; Shift additive select |

Still optional / egui: full Review/PDF docks, hatch solid paint, WP/CP/FENCE tools,
apostrophe `'ZOOM` suspend (wheel zoom mid-tool covers daily need).

---

### M23 — WinUI optional gap close ✅

Closes remaining optional items from the WinUI audit. **ABI v10.**

| # | Item | Status | Exit |
|---|------|--------|------|
| M23.1 | WP / CPOLYGON / FENCE | ✅ | Space finishes region select |
| M23.2 | Hatch solid fill paint | ✅ | `aadt_drawlist_copy_fills` + D2D FillGeometry |
| M23.3 | Transparent `'ZOOM` / `'PAN` | ✅ | mid-tool feed; `aadt_nav_take` host |
| M23.4 | Thin Review / Plot QAT | ✅ | LIST / PLOTSETUP / ZoomE |

Still egui: full Annotate/Review/PDF docks; IMAGE bitmap blit; rich apostrophe
suspend (wheel + `'ZOOM E` cover daily need).

---

## Later (optional)

Ordered backlog — pick after M17 or in parallel if unblocked:

| # | Item | Notes |
|---|------|-------|
| L1 | SpatialGrid in GUI click pick | ✅ click pick (osnap still linear) |
| L2 | True OTRACK | ✅ F11 acquire + H/V (±polar) rays ([ADR-0018](adr/0018-otrack.md)) |
| L3 | M7 PDF viewer UI | **L3.1–L3.3 ✅** sheet preview, freehand, PDFium open/render + raster diff ([ADR-0019](adr/0019-pdfium-reader.md)) |
| L4 | DWG hatch + dimension write | ✅ polyline HATCH + common DIMENSION kinds via acadrust |
| L5 | Viewport text glyphs; wgpu if egui too slow | **✅ glyphs** + **L5.2 ✅** optional wgpu path / criteria ([ADR-0020](adr/0020-wgpu-canvas-criteria.md)); WinUI stays default |
| L6 | Sheet-bound view presets; material / opacity libraries | **✅** clone-safe plot/present presets; `libraries/materials/catalog.json` fill opacities |
| L7 | On-canvas block placement polish | **✅** INSERT rotation/scale + oriented ghost (type ° / Sscale) |

---

## Status summary

| Milestone | Status |
|-----------|--------|
| M1 CAD core (egui ✅ · WinUI host via M16) | ✅ |
| M2 Detection | ✅ |
| M5 Sheets / plot | ✅ |
| M6 LISP | ✅ |
| M7 PDF markup | ✅ core |
| M9 BricsCAD parity | ✅ |
| M10–M15 AI-native + arch plan | ✅ |
| M16 WinUI interactive tools | ✅ |
| M17 WinUI Chrome 2.0 | ✅ |
| M18 WinUI usable CAD | ✅ |
| M19 WinUI daily polish | ✅ |
| M20 WinUI commands + Elements/Fills | ✅ |
| M21 WinUI honesty + daily CAD | ✅ |
| M22 WinUI remaining gap close | ✅ |
| M23 WinUI optional gap close | ✅ |
| L5 Viewport text glyphs | ✅ |
| L5.2 Optional wgpu path | ✅ criteria ADR-0020 (WinUI remains default) |
| L6 Sheet presets + materials | ✅ |
| L7 INSERT placement polish | ✅ |
| L3.3 PDFium + raster diff | ✅ ADR-0019 |

**1.0 band:** M1–M2, M5–M7, M9–M15. **Post-1.0:** M16–M17 ✅; **Later L1–L7 ✅**.
**2D only** — no BIM / 3D milestone.
