# AADT Roadmap

This roadmap sequences AADT from a proven deterministic takeoff slice up to the
full product. It reorders the spec's suggested phases (§10) so the **element
detection and takeoff pipeline — the product's reason for existing — is proven
first**, on the minimum substrate it needs, rather than after a full CAD core.

Each milestone is a shippable, verifiable increment.

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
2. **DWG read/write is deferred behind DXF.** See
   [`adr/0002-dwg-dxf-strategy.md`](adr/0002-dwg-dxf-strategy.md).

---

## Curated command band

AADT is a **powerful 2D drafting tool free of bloat**. Depth goes into LISP, not
more command names.

**Daily (interactive + prominent autocomplete):**

| Kind | Commands |
|---|---|
| Draw | `LINE` `POLYLINE` `CIRCLE` `ARC` `RECTANGLE` `TEXT` `INSERT` |
| Edit | `MOVE` `COPY` `ROTATE` `SCALE` `MIRROR` `OFFSET` `TRIM` `EXTEND` `FILLET` `STRETCH` `ERASE` |
| Org | `LAYER`/`LAYERS` `LAYERSET` `SELECT` `UNDO`/`REDO` `DIST` `LIST` `OPEN` `SAVE` (`.dxf` / `.aadt`) |
| Annotate | `TEXT` `MLEADER` `DIM*` (de-emphasized) |
| AADT | `TAKEOFF` `SCHEDULE` `SECTION` `STANDARDS` `BLOCKLIB` |
| Power | `LISP` / bare `(…)` · `LISPLOAD` |

**Click-tools in the egui viewport:** `L` `C` `REC` `A` `O` `T` `F` `MLEADER` `STRETCH` `BRUSH` ·
`M` `CO` `RO` `SC` · `E` · pick/box select · Ortho (Shift / F8) · Osnap endpoint/midpoint (F3).

**Keep but de-emphasize** (scripts / DXF fidelity, not GUI chrome): `DIM*`
variants, `HATCH`, `CHAMFER`, `ARRAY`, `EXPLODE`, `PURGE`, `GROUP*`/`UNGROUP`,
`AREA`.

**Do not add:** MTEXT sprawl, spline/ellipse without a real need, ribbon/toolbar
forests, 3D commands. Raster brushes / PNG block libraries are out of scope
(vector-only: DXF + SVG blocks, vector freehand on `A-PRES-*`).

**Standards / presentation:** AIA/US NCS layers (`aadt-standards`, WCAG vs white
paper), layer sets (`CONSTRUCTION` / `SERVICES` / `ELECTRICAL` / `PRESENTATION`),
light print-preview GUI, `MLEADER`, `STRETCH`, `SECTION`, block/hatch libraries,
vector brush.

---

## M0 — Deterministic Takeoff Vertical Slice ✅

`DXF import → wall/room detection → deterministic takeoff → CSV export`.

Verify: `cargo test --workspace`,
`cargo run -p aadt-cli -- takeoff fixtures/sample-floorplan.dxf --csv out.csv`.

---

## M1 — CAD Core Substrate ✅ (headless) · interactive shell ✅ (egui)

**Headless foundations — ✅:** atomic undo/redo, command interpreter with the
curated band above, selection sets, dimensions (full DXF set), hatch, layer
state, DXF round-trip, drawlist/camera seam.

**Interactive surface — ✅ (egui):**
- Command-first shell with live takeoff panel, pick/box select, click-to-draw
  (`L`/`C`/`REC`/`A`), noun-verb `M`/`CO`/`RO`/`SC`/`E`, dynamic input, Ortho
  (F8), native file dialogs (`rfd`).
- `OPEN`/`SAVE`/`TAKEOFF`/`SCHEDULE`/`LISP`/`LISPLOAD` on the same `Session` the
  CLI shell uses; bare `(…)` evaluates AutoLISP against the live document.
- Thin `aadt-ffi` cdylib for future chrome (e.g. WinUI) without engine rewrite.

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

## M3 — Takeoff & Quantity Engine ✅

Wall area/volume, opening subtraction, CSI cost codes, `LiveTakeoff`, `.xlsx`
export, in-drawing `SCHEDULE` tables.

---

## M4 — AI Orchestration Layer ✅ (core)

Structured extraction, wall-type inference, graceful `Unavailable`, audit log,
**Foundry Local** as default inbuilt AI (`FoundryLocalClient`), optional
`OllamaClient`, process lifecycle, `apply_wall_types` as undoable ops, CLI
`infer --backend foundry-local|ollama`. **Remaining:** AI schedule formatting /
notes suggestion (optional accelerants — never gates).

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

## M8 — Cloud Collaboration ✅ (engine)

Convergent op log + TCP relay (`aadt collab --net`). **Remaining:** presence /
hosted session service.

---

## Next

1. More AADT metadata tables in `.aadt` (detection cache, cost-code maps).
2. DWG when exchange demand requires it (ADR-0002).
3. Optional WinUI chrome over `aadt-ffi` — engine unchanged.
4. wgpu viewport only if entity-count performance demands it.
