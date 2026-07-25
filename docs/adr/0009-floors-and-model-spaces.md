# ADR-0009 — Floors and category model spaces

## Status

Accepted — 2026-07-25

## Context

Architectural drafting needs multiple storeys that share a fixed XY reference
(like ArchiCAD stories), plus per-storey drafting contexts for Walls, Structural,
Lintel, Beam layout, and Furniture. AutoCAD paper space / layouts / XREF remain
out of product scope (see ROADMAP product locks and ADR-0002).

Users asked that these contexts be **real model spaces**, not layer-set filters:
the active model space *is* the element category.

## Decision

1. **One `Document`** continues to own all geometry (undo, `.aadt`, takeoff).
2. **`Floor`** = ordered storey with name + elevation (metres; XY shared).
3. **`ModelSpace`** = `(floor × SpaceCategory)` with five built-in categories.
4. **`reference_origin`** is a shared building origin drawn on the canvas; all
   floors align to it (no per-floor UCS in v1).
5. **`TRACE BELOW` / `TRACE ABOVE`** ghost adjacent floors (all categories) under
   the active floor; same-floor non-active categories are a soft underlay.
   Picking/editing is limited to the **active model space**. Object snap may
   acquire TRACE/underlay geometry without allowing edit (snap-without-edit).
6. **Layers** remain a DXF/DWG **compat shim** only (`SpaceCategory::default_layer`).
   Drafting UX is Floor + Category (`FLOOR`, `CATEGORY`, Levels strip). The Layer
   Manager does not own a separate “current drafting layer.”
7. **Schema v6** persists floors, model spaces, and `space_id` on entities /
   hatches / dimensions. Import assigns Ground spaces via layer-name heuristic.
   **`LAYERMAP APPLY` / `reassign_by_layer_map`** yanks entities from **all floors**
   onto the active floor’s category spaces.

## Consequences

- Layer manager stays for import/compat visibility; new draws follow the active
  category’s shim layer.
- Plot / takeoff can later filter by floor; v1 keep full-document behaviour with
  active-space drafting.
- No paper-space layouts, no XREF, no live MVIEW.
