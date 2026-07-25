# Industry BOQ / takeoff standards (AiADT mapping)

**Date:** 2026-07-24  
**Related:** [ADR-0005](../adr/0005-cost-code-standard.md),
[Bluebeam case study](./bluebeam-case-study.md).

## What estimators expect

Commercial takeoff / BOQ tools (PlanSwift, STACK, On-Screen Takeoff, Bluebeam
+ Excel, CostX, etc.) converge on:

| Expectation | Notes |
|---|---|
| **Calibrated units** | Drawing scale → real lengths/areas |
| **Traceable quantities** | Each row links back to a measurement or element |
| **Cost-code taxonomy** | CSI MasterFormat (NA), often UniFormat for systems |
| **Division rollups** | Sum by CSI division (e.g. 08 Doors & Windows) |
| **Unit + extended cost** | Optional unit price × quantity |
| **Excel/CSV export** | Stable columns for bidding software |
| **Versioned snapshots** | Freeze quantities at bid / IFC / IFC-equivalent issue |

AiADT already covers calibrated units (`UNITS`), element-linked rows
(detect → takeoff), CSI codes (ADR-0005), CSV/JSON/XLSX, and `.aadt` takeoff
runs / HISTORY checkpoints.

## Taxonomies

### CSI MasterFormat (built-in — ADR-0005)

Primary North American construction-spec numbering (`XX YY ZZ`). AiADT maps
element kinds to section codes (e.g. doors → `08 10 00`). **Division** =
first two digits (`08`).

### UniFormat (optional later)

Systems-oriented (A–G). Useful for design-stage estimates. Not required for
M3; document as future firm mapping layer, not a second built-in default.

### Regional / firm maps

UK NRM, Australian Standard Method of Measurement, etc. — firm override JSON /
project properties only (same mechanism as custom CSI overrides).

## AiADT export columns (target)

| Column | Source |
|---|---|
| `category` | wall / opening / room / structural |
| `ref` | entity or opening number |
| `kind` | subtype |
| quantity fields | length, area, volume, … |
| `cost_code` | CSI MasterFormat section |
| `csi_division` | two-digit division |
| `unit_cost` | optional firm price |
| `extended_cost` | quantity × unit_cost when set |
| `wall_type` / `class_status` | AI classification attrs |

## Non-goals

- Live ERP write-back (keep SQL script / file sinks).
- PDF markup takeoff as primary UI (see Bluebeam case study).
- Paper-space takeoff viewports (product lock: nameplate PDF plotting).
