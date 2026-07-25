# Industry-standards audit — AiADT (July 2026)

**Scope:** Compare AiADT against AEC/CAD and takeoff/BOQ expectations.  
**Method:** Code + docs inventory, HCW Track A, ADR conformance, Bluebeam QTO
case study.  
**Verdict grade:** **B+** (gap-closure complete; remaining polish below).

Interactive gradeboard: workspace canvas `aiadt-industry-audit.canvas.tsx`.  
Build plan: [`docs/plans/2026-07-gap-closure.md`](../plans/2026-07-gap-closure.md) (done).

---

## 1. Executive summary

AiADT is a **local-first 2D CAD + deterministic takeoff** product: strong undo,
`.aadt` v5 history, DXF (+ open-source DWG path), LISP, PDF nameplate plotting,
and on-demand AI classification. Sheets are **model space + nameplate/PDF**, not
AutoCAD paper space.

| Dimension | Grade | One-line |
|---|:---:|---|
| Architecture & layering | **A−** | Clean substrate / engine / app split |
| Undo / transactional CAD | **A−** | `Document::transact` rule |
| Drafting command parity | **B+** | Daily band + ARRAYRECT/POLAR/PATH; VP/XREF omitted by lock |
| Interop (DXF/DWG) | **B+** | DXF solid; DWG R/W via `acadrust` (best-effort subset) |
| Persistence & history | **A−** | Session undo ≠ file HISTORY (by design) |
| Takeoff / BOQ | **B+** | CSI spine + division rollups; optional unit-cost later |
| AI assist (local) | **B+** | Engine + Takeoff-panel Recognize / Review / Accept / Freeze |
| GUI / UX | **B+** | Place tools seeded; arrays interactive; Fluent orange chrome |
| Testing | **B** | Store migrate/checkpoint coverage expanded |
| Docs hygiene | **A−** | Audit + takeoff case study + completed gap plan |
| Security / local-first | **A−** | No cloud secrets |
| **Overall** | **B+** | Pilot-ready; gap-closure shipped; polish remains |

---

## 2. Intentional product locks (not gaps)

### G3 — Paper space / XREF / VP — **not a gap**

AiADT ships **inbuilt nameplate plotting** and PDF sheet production
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

Withdrawn from gap register: **G3**, **G7** (locks above).

---

## 4. Takeoff / BOQ vs industry

See [industry BOQ standards](../takeoff/industry-boq-standards.md) and
[Bluebeam case study](../takeoff/bluebeam-case-study.md).

**AiADT posture:** CAD-native detect → quantities (not PDF markup-first). Adopt
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

Gap-closure and ROADMAP Next are **shipped**. Still open (Post-1.0):

1. Hosted collab session service (local presence exists on the TCP relay).
2. Autodesk-class DWG / hatch edge-type fidelity.
3. M1/M7 display polish (wgpu, PDF viewer UI).
4. **Do not** implement paper space or merge UNDO with HISTORY.
