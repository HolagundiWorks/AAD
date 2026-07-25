# Case study — Bluebeam Revu quantity takeoff

**Purpose:** Learn industry-standard takeoff UX from Bluebeam Revu so AiADT’s
CAD-native takeoff stays competitive without copying a PDF-markup product.
**Date:** 2026-07-24.

## What Bluebeam is

Bluebeam Revu is a **PDF markup and measurement** platform used heavily by
estimators and trade partners. Takeoff happens on **calibrated PDF sheets**, not
on live CAD entities. Quantities live as **measurement markups** summarized in
the **Markups List**, then flow to Excel via **Quantity Link**.

## Core workflow (industry pattern)

1. **Calibrate scale** — set drawing scale (or separate X/Y); optional viewports
   with different scales on one PDF.
2. **Measure** — length, area, volume, count, polyline, radius; tools snap to
   PDF geometry; measurements can persist as markups.
3. **Organize** — tool sets, layers/subjects, **custom columns** on the Markups
   List (e.g. CSI division, unit cost, trade).
4. **Roll up** — Markups List totals by measurement type; Dynamic Properties
   totals for the current selection.
5. **Price & bid** — unit costs in custom columns; **Quantity Link** keeps Excel
   formulas tied to live markup quantities.
6. **Collaborate** — Studio sessions; markup summaries / PDF packages.

CSI MasterFormat is **not built into Revu**; firms encode it via custom columns
or third-party tool sets (often ~600 tools across 24 CSI divisions).

## Comparison to AiADT

| Bluebeam strength | AiADT equivalent today | Adopt? |
|---|---|---|
| PDF-first, any issued sheet | CAD-native 2D + PDF *plot* | Keep CAD-native; PDF is output |
| Scale calibration on PDF | Drawing units (`UNITS`) | Already; document clearly |
| Measurement markups as data | Detected walls/rooms/openings | Keep detection spine |
| Markups List + custom columns | Takeoff table + CSI codes | **Yes** — division + unit cost columns |
| Quantity Link → Excel | CSV / XLSX / JSON export | **Yes** — stable columns + rollups |
| VisualSearch / symbol count | Block inserts + COUNT | Later polish |
| Multi-scale viewports | Out of scope (nameplate PDF sheets) | No |

## Product implications for AiADT

1. **Do not chase PDF takeoff** as the primary path — AiADT’s differentiator is
   **structured CAD → deterministic quantities**.
2. **Match estimator expectations** on export shape: CSI MasterFormat code,
   CSI *division* rollup, optional unit cost / extended cost, category totals.
3. **Live panel** should feel like a Markups List: running totals by category,
   filter by classification status.
4. **History publish** of takeoff runs ≈ Bluebeam’s “snapshot before bid” habit;
   keep `TAKEOFF PUBLISH` + checkpoints.

## References

- Bluebeam: [Takeoffs and measurements](https://support.bluebeam.com/user-manual/menus/tools/takeoffs-measurements.html)
- Bluebeam: [Quantity takeoffs overview](https://blog.bluebeam.com/maini-qto-revu/)
- Bluebeam U: [Takeoff data and industry workflows](https://university.bluebeam.com/takeoff-data-and-industry-workflows)
