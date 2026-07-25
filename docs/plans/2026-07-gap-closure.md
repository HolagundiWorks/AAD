# Implementation plan — close audit gaps (post G3/G7 clarification)

**Date:** 2026-07-24  
**Inputs:** [Industry audit](../audit/2026-07-industry-standards-audit.md),
[Bluebeam case study](../takeoff/bluebeam-case-study.md),
[BOQ standards](../takeoff/industry-boq-standards.md).

## Product decisions (locked)

| Topic | Decision |
|---|---|
| **G3 Paper space / XREF** | **Out of scope.** Sheets use **model-space geometry + inbuilt nameplate / PDF plot** (`aadt-docs` / `aadt-pdf`). Do not implement `VP`/`MVIEW`/XREF for parity. |
| **UNDO vs HISTORY** | **Two products.** Session `UNDO`/`REDO` = short RAM stack of drafting ops. File `HISTORY` = versioned checkpoints inside `.aadt`. Neither replaces the other. |
| **DWG** | **Open-source pure Rust** via [`acadrust`](https://crates.io/crates/acadrust) (MPL-2.0), not ODA. Feature-gated `dwg` on `aadt-dxf`. |
| **Takeoff** | Stay **CAD-native + deterministic** (not PDF markup-first like Bluebeam). Adopt Bluebeam *ideas*: CSI columns, division rollups, Excel-friendly export, live totals. |

## Build order

1. **Docs** (this pass) — audit reframe, Bluebeam + BOQ notes, ADR-0002, ROADMAP Next.
2. **DWG import** — `aadt-dxf` + `acadrust`; wire `OPEN`/`SAVE` for `.dwg` (export DXF or best-effort DWG write).
3. **Place tools** — seed door/window/column/beam/slab block defs so INSERT succeeds.
4. **GUI AI review** — History-style panel for `REVIEW` / Accept / Reject / Freeze.
5. **Takeoff** — CSI division field + rollup totals; optional unit-cost columns; docs.
6. **Store tests** — migrate + checkpoint round-trips.
7. **DXF fidelity** — multi-path hatch (when model supports) / nested insert skip → warn; dim kinds already in model.

## Done-when

- [x] Docs: G3/UNDO-HISTORY locks, Bluebeam + BOQ, plan, ADR-0002
- [x] `OPEN file.dwg` imports supported 2D entities into `Document`
- [x] DOOR/WINDOW/COLUMN/BEAM/SLAB seed real inserts
- [x] egui AI Classify actions on Takeoff panel
- [x] Takeoff CSV includes CSI division + rollup helper
- [x] Store tests cover save/checkpoint after migrate-safe path
- [x] DXF nested-insert warning (remaining polish)
- [x] Audit gap table matches the locks above
