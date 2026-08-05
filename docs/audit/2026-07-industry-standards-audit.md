# Industry-standards audit — AAD (July 2026, refreshed 2026-08)

**Scope:** Compare AAD against AEC/CAD and takeoff/BOQ expectations.  
**Method:** Code + docs inventory, HCW Track A, ADR conformance, Bluebeam QTO
case study.  
**Verdict grade:** **A−** (gap-closure, M9 drafting parity, and the M10–M13
AI-native phases all shipped; remaining polish below).

Build plan: [`docs/plans/2026-07-gap-closure.md`](../plans/2026-07-gap-closure.md) (done).

> **Refresh note (2026-08).** The July audit was written before the product was
> renamed to AAD and before M9–M13. Grades, the gap register, and the polish
> list below are current; the product locks in §2 are unchanged.

---

## 1. Executive summary

AAD is a **local-first 2D CAD platform**: strong undo, `.aadt` history
checkpoints, DXF (+ open-source DWG path), ShilpiDB `.vdb`, LISP, PDF nameplate
plotting, typed engineering objects with a graph of relationships, and a gated
local assist layer. Sheets are **model space + nameplate/PDF**, not AutoCAD
paper space.

| Dimension | Grade | One-line |
|---|:---:|---|
| Architecture & layering | **A−** | Clean substrate / capability / intelligence split |
| Undo / transactional CAD | **A−** | `Document::transact` rule |
| Drafting command parity | **A−** | M9 complete: osnap overrides, fence/polygon/lasso, multi-mode grips, auto-type DIM, docking |
| Interop (DXF/DWG) | **B+** | DXF solid; DWG R/W via `acadrust` (best-effort subset) |
| Persistence & history | **A−** | Session undo ≠ file HISTORY (by design); objects/graph/audit in `.aadt` v8+ / `.vdb` v2 |
| Takeoff / BOQ | **A−** | M13 object-based takeoff; CSI spine + division rollups; graph-driven opening deduction |
| AI assist (local) | **A−** | `aadt-assist` propose-never-commit: dims, code checks, NL routing, `COMPARE`, `RECOMMEND DETAIL` |
| GUI / UX | **A−** | Docked / floating / auto-hide panels, place tools, Fluent orange chrome, F7 perf HUD |
| Testing | **B+** | Store migrate/checkpoint coverage plus per-command QA harness (M9.8) |
| Docs hygiene | **A−** | Audit + takeoff case study + ADRs + phase plans kept current |
| Security / local-first | **A−** | No cloud secrets; no network dependency for drafting or assist |
| **Overall** | **A−** | Pilot-ready; M9–M13 shipped; Post-1.0 polish remains |

---

## 2. Intentional product locks (not gaps)

### G3 — Paper space / XREF / VP — **not a gap**

AAD ships **inbuilt nameplate plotting** and PDF sheet production
(`aadt-docs` / `aadt-pdf`). Layouts are model-space drawings composed onto sheets
with title blocks — not AutoCAD paper-space viewports or XREF underlays.
Commands `VP` / `MVIEW` / `XREF` / `IMAGEATTACH` remain stubs and **must stay
that way** unless the product lock changes.

### UNDO vs HISTORY — **two separate systems**

| | Session **UNDO** / **REDO** | File **HISTORY** |
|---|---|---|
| Scope | Few drafting steps in RAM | Versioned checkpoints in `.aadt` |
| Lifetime | Lost when the document is closed | Survives save / reopen |
| API | `UNDO` / `REDO` | `HISTORY LIST\|SAVE\|RESTORE\|PURGE` |
| Analogy | AutoCAD UNDO | Sheet revisions / time machine |

Do **not** merge them. Docs and UI should label them clearly.

---

## 3. Gap register (post gap-closure)

| ID | Pri | Gap | Status |
|---|---|---|---|
| G1 | P0 | DWG via open-source parser (`acadrust`) | **Done** (read + best-effort write) |
| G2 | P0 | DXF fidelity (warn nested inserts; hatch paths) | **Done** — nested-insert warn + multi-path polyline hatch |
| G4 | P1 | GUI AI review panel | **Done** |
| G5 | P1 | Door/window/column/beam/slab place tools | **Done** |
| G6 | P1 | Store migrate / HISTORY tests | **Done** |
| T1 | P1 | Takeoff CSI division + unit/extended cost columns | **Done** (CSI + rollups; unit-cost columns optional later) |
| G8 | P1 | BricsCAD §28 docking: floating panels + auto-hide | **Done** (M9.6 — `PanelMode` per side, persisted in prefs) |
| G9 | P2 | Revision-comparison narration for transmittals / RFIs | **Done** (M12.6 — `COMPARE`, CLI `aadt compare`) |
| G10 | P2 | Standard-detail recommendation | **Done** (M12.7 — `RECOMMEND DETAIL` over `libraries/details/`) |

Withdrawn from gap register: **G3**, **G7** (locks above).

The rest of the BricsCAD workflow gaps (osnap overrides, fence/polygon/lasso
selection, multi-mode grips, auto-type `DIM`, live hatch preview, capability
contract, QA harness, perf telemetry) closed as M9.1–M9.9 — see the
[conformance audit](../ux/bricscad-conformance-audit.md).

---

## 4. Takeoff / BOQ vs industry

See [industry BOQ standards](../takeoff/industry-boq-standards.md) and
[Bluebeam case study](../takeoff/bluebeam-case-study.md).

**AAD posture:** CAD-native detect → quantities (not PDF markup-first). Adopt
Bluebeam *estimator* expectations: CSI codes, division rollups, Excel-stable
columns, optional unit costs, versioned takeoff runs.

---

## 5. HCW Track A

| Rule | Status |
|---|---|
| Mutations inside transactions | Pass |
| Shared logic | Mostly |
| Undo tested | Strong core |
| No secrets | Pass |
| Changelog / command docs | Pass |

---

## 6. Remaining polish

Gap-closure, ROADMAP Next, M9 (drafting parity), and M10–M13 (AI-native
phases 1–4) are **shipped**. Still open (Post-1.0):

1. Autodesk-class DWG / hatch edge-type fidelity.
2. M1 display polish: viewport text glyph quality, wgpu canvas at 100k+
   entities, transparent mid-command `'ZOOM` / grip-edit.
3. M7 PDF viewer UI: interactive viewing, freehand/stamps, on-PDF measure,
   raster diff.
4. Phase 5 (enterprise + APIs): local API service over `aadt-ffi`, connectors
   as plugins, object permissions, signed audit trail.
5. **Do not** implement paper space or merge UNDO with HISTORY.

Withdrawn: hosted collaboration — the `aadt-collab` crate and TCP relay were
removed with M8, so there is no local presence to build a hosted service on.
