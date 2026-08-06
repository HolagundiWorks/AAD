# Market Gap Case Study — AADT / AAD vs Architectural CAD Stack

**Date:** 2026-08-06  
**Audience:** Product, architecture, investors, review board  
**Status:** Draft for review  
**Scope:** Desktop **2D architectural CAD** + AI-native platform (AADT). Not a BIM suite.

---

## 1. Executive summary

The architectural CAD market is crowded at the **geometry editor** layer (AutoCAD LT, BricsCAD Lite, DraftSight, LibreCAD) and dominated at the **3D / BIM** layer (Revit, Archicad, Allplan). Between those poles sits a persistent gap:

> Firms need **2D floor-plan drafting that understands walls, openings, fills, and takeoff**, with **AI assist and enterprise data**, without paying Revit-class cost or accepting AutoCAD’s “dumb line” model.

**AADT’s bet:** command-first **2D CAD** (AutoCAD-like muscle memory) + semantic Elements/Fills + wall openings + ShilpiDB (objects / graph / embeddings) + local API — a platform that *understands* drawings and stays permanently plan-based.

This case study maps **market gaps**, **competitor positions**, **AADT fit**, and a **12–18 month product priority** for review.

---

## 2. Market map (who owns which job)

| Segment | Typical tools | Job-to-be-done | Price posture | Gap vs AADT thesis |
|---------|---------------|----------------|---------------|--------------------|
| Pro drafting (2D) | AutoCAD / LT, BricsCAD | Exact geometry, DWG exchange, plot | High subscription | Lines ≠ walls; AI/enterprise bolted on |
| Mid drafting | DraftSight, progeCAD, ZWCAD | DWG-compatible plans | Mid | Weak architecture semantics |
| Free / OSS | LibreCAD, QCAD | Learn / light drafting | Free | Thin UX; no AI; weak hatch/block fidelity |
| 3D / BIM suites | Revit, Archicad, Allplan | Model + docs + coordination | Very high | Different product; AADT does not compete here |
| Viewer / markup | Bluebeam, Autodesk Viewer | PDF review | Mid | Not authoring |
| AI CAD experiments | Various startups | Generate / chat | Uncertain | Often geometry-unowned or cloud-only |

**White space:** *Architectural plan production* (walls, doors, windows, floors, presentation boards) with **deterministic geometry**, **DWG/DXF survival**, and **AI that never owns the database**.

---

## 3. Gap analysis (detailed)

### Gap A — Geometry without meaning

**Problem.** AutoCAD-class tools store walls as lines/polylines. Takeoff, opening cuts, and hatch containment are manual.

**Evidence in AADT.** Door/window place snaps to walls, cuts **both faces** of 4½″/9″ (or metric 100–230 mm) walls, and closes jambs into hatchable polylines. Elements carry `AADT-ELEM`; Fills carry `FillKind` (Technical / Floor / Presentation).

**Market implication.** Competing on “more AutoCAD commands” loses. Competing on **architectural verbs** (WALL, DOOR, FILL, ROOM) wins for floor-plan shops.

### Gap B — Presentation vs technical drawings

**Problem.** Same model must plot as construction plan *and* client board. Competitors use layer freezes + separate files; presentation furniture is often PDF collage.

**Evidence in AADT.** View presets (Technical Plan / Presentation), typed Fills, PNG furniture (`Geometry::Image`), Elements catalog.

**Remaining gap.** Sheet-set viewports bound to presets; material libraries with opacity standards. IMAGEDEF on DWG/DXF export is shipped.

### Gap C — Dynamic / library blocks in DWG land

**Problem.** Autodesk Dynamic Blocks are proprietary. Open DWG stacks (including AADT via `acadrust`) cannot fully edit dyn actions. Firms still need doors that flip/stretch and round-trip.

**Evidence in AADT.** Nested INSERTs kept; `BFLIP` / WIDTH stretch / visibility; bake on export; AutoCAD dyn import → static + warning.

**Market implication.** Position honestly: **AADT Dynamic Elements** for authoring; **baked DWG** for exchange — not “100% AutoCAD dyn parity.”

### Gap D — AI that does not own geometry

**Problem.** Generative CAD demos often produce uneditable blobs or cloud lock-in. Enterprises need audit, permissions, and undoable commits.

**Evidence in AADT.** Vision docs + Object/Graph/Vec stores; local HTTP API; `transact` undo; AI suggests, document commits.

**Remaining gap.** Ship end-to-end “detect wall → promote EngObject → ASK” demos that sales can show in 5 minutes.

### Gap E — Interaction parity (trust)

**Problem.** Architects abandon tools that break hatch erase, rectangle dims, or trim/extend habits.

**Evidence.** M9 BricsCAD conformance largely closed. **2026-08 fixes:** hatch delete (fill + boundary), rectangle D-mode click stealing.

**Market implication.** Parity is table stakes; gaps here kill adoption faster than missing 3D features.

### Gap F — Interop fidelity

**Problem.** DWG write of hatches/dims/images remains best-effort in open stacks.

**Positioning.** `.aadt` / `.vdb` as source of truth; DXF/DWG as exchange with published fidelity matrix (nested blocks ✅, IMAGE/IMAGEDEF ✅, dyn ⚠️ baked).

---

## 4. Competitor scorecard (directional)

Scores are **relative fit to AADT’s target buyer** (2D architectural plan production + AI/enterprise), not absolute CAD quality. Scale 1–5 (5 = strong).

| Capability | AutoCAD LT | BricsCAD | DraftSight | LibreCAD | Revit | **AADT (today)** |
|------------|------------|----------|------------|----------|-------|------------------|
| DWG daily exchange | 5 | 5 | 4 | 2 | 3* | **4** |
| Command-line drafting | 5 | 5 | 4 | 3 | 2 | 4 |
| Wall/opening semantics | 2 | 2 | 1 | 1 | 5 | **4** |
| Typed fills / presentation | 3 | 3 | 2 | 1 | 4 | **4** |
| PNG furniture place | 3 | 3 | 2 | 1 | 3 | **4** |
| AI + object graph | 2 | 2 | 1 | 1 | 3 | **4** (foundation) |
| Cost for small firm | 2 | 3 | 3 | 5 | 1 | **5** (potential) |
| Learning curve from ACAD | 5 | 4 | 4 | 2 | 1 | **4** |

\*Revit exchanges via DWG export with loss of model semantics.

---

## 5. Buyer personas & jobs

1. **Small architecture studio (2–15)** — floor plans, municipal submissions, client boards; AutoCAD LT fatigue; needs DWG out.
2. **Interior / fit-out** — presentation fills + PNG furniture; 2D plan CAD is enough.
3. **Engineering office (MEP/struct overlay)** — layer standards, dims, takeoff hooks; AI for clash *hints* later.
4. **Enterprise / EPC** — audit, ACL, connectors (M14); local API into ERP/PLM.

**Primary beachhead:** persona 1–2 in markets where **metric + imperial** wall thicknesses (115/230 mm, 4½″/9″) are daily vocabulary.

---

## 6. Product priorities (review recommendations)

| Priority | Theme | Status (2026-08) |
|----------|-------|------------------|
| P0 | Interaction trust (erase fill, rectangle D-mode, RECTANGLE Tab) | **Done** |
| P0 | DWG fidelity matrix + export warnings | **Done** — [dwg-fidelity-matrix.md](../interop/dwg-fidelity-matrix.md) |
| P1 | Elements catalog + PNG tiles | **Done** — tile catalog + Import PNG |
| P1 | Sheet/plot Technical vs Presentation | **Done** — plot dialog View + layer filter in compose |
| P2 | Dynamic Element grips (flip/width) | **Done** — INSERT grips 1/2 |
| P2 | 5-minute AI demo | **Done** — `DEMO` / `AIDEMO` |
| P2 | Door/window: pick both wall lines + swing side | **Done** — explicit face pair + Tab/F swing |
| P3 | IMAGEDEF / richer DWG write | **Done** — IMAGE + IMAGEDEF via acadrust · ROADMAP **M15** |

**Out of product:** 3D modeling and BIM authoring — permanent (see ROADMAP product locks).

---

## 7. Risks & mitigations

| Risk | Mitigation |
|------|------------|
| “Yet another AutoCAD clone” perception | Lead with Elements / wall cut / Fills / AI graph — not command count |
| Dyn block parity expectations | Document bake-on-export; offer AADT Dynamic Elements |
| DWG hatch/IMAGE loss | Fidelity matrix; keep `.aadt` canonical |
| Scope creep into Revit | ROADMAP lock: **2D CAD only** — curated plan semantics, never BIM authoring |

---

## 8. Review asks

1. Confirm **beachhead** (small architecture / interiors) vs enterprise-first.
2. Approve **honest DWG fidelity** messaging over overclaiming dyn/IMAGE.
3. Rank P0–P2 for the next milestone after M14.
4. Decide whether market narrative is **“AI-native plan CAD”** or **“affordable AutoCAD alternative with walls.”**

---

## 9. Related internal docs

- [BricsCAD conformance audit](../ux/bricscad-conformance-audit.md)  
- [AutoCAD research notes](../ux/autocad-research.md)  
- [AI-native platform vision](../vision/ai-native-platform.md)  
- [ROADMAP](../ROADMAP.md) · [CHANGELOG](../CHANGELOG.md)  
- Interactive review canvas: Cursor canvases → `market-gap-case-study.canvas.tsx`
- Interop: [DWG/DXF fidelity matrix](../interop/dwg-fidelity-matrix.md)
- Commands: `DEMO` / `AIDEMO` for the AI narrative path

---

*Prepared for internal review. Not a customer-facing brochure.*
