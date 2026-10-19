# Hand-off to O-CoreOp: parametric take-off modellers (walls)

Date: 2026-10-19  
From: O-Draft (AADT)  
To: O-CoreOp  
Subject: What O-Draft now emits for parametric walls, and what O-CoreOp must own

This is the O-Draft side of the wall vertical slice. Copy or link this file into the O-CoreOp
repository next to `docs/TAKEOFF-SHILPIDB.md` and `docs/HANDOFF-O-DRAFT-TAKEOFF.md`. O-Draft
implementation notes: [parametric.md](parametric.md), [takeoff-shilpidb.md](takeoff-shilpidb.md).

## 1. Ownership (unchanged)

| Concern | System |
|---|---|
| Drawing geometry, graph, undo, commit | O-Draft |
| Measured facts (length, height, thickness, level, layer) | O-Draft → ShilpiDB / bridge |
| Gross/net areas, finishes (plaster, paint), shuttering, rates, costs | **O-CoreOp only** |
| Component library as a hosted store | Not built; schemas are code in O-Draft (`osas.wall` v1) |

O-Draft never sends plaster, paint, shuttering, rates or costs. Gross face area and volume from the
graph's `measure.wall` node are **preview only** and are **not** written into the take-off records.

## 2. How O-CoreOp receives the data

Same path as today's take-off. No new wire protocol.

1. User (or script) commits parametric walls in O-Draft (`param.commit`).
2. `takeoff.record` (or `bridge.export`) runs `bridge.collect`.
3. Linked, current parametric walls are merged into the `walls` array with brick take-off walls.
4. ShilpiDB records: kind `3` (wall), id `(3 << 56) | n` where `n` is the low 56 bits of the **first
   face-line handle**; payload = the bridge wall row; bbox = union of face-line boxes in mm.

File default: `<drawing>.takeoff.vdb`. Server: `takeoff.record { server: "host:port" }`.

Contract version remains **hcw-takeoff-shilpi / 1** and bridge format **hcw-aqc-bridge / 1**.

## 3. Wall row shapes O-CoreOp will see

### 3.1 Parametric wall (new)

```json
{
  "ref": "PW-wall-1",
  "level": "Ground",
  "length_mm": 5000.0,
  "height_mm": 3000.0,
  "thickness_mm": 230.0,
  "layer": "A-WALL"
}
```

| Field | Notes |
|---|---|
| `ref` | Always `PW-<instanceId>`. Stable for that instance while it stays in the graph. |
| `level` | From the wall component when set; else the export `level` / first floor name. |
| `length_mm` | Centre-line length, millimetres, one decimal place. |
| `height_mm` | Wall height in mm. |
| `thickness_mm` | **Present.** Brick take-off walls omit this. |
| `layer` | Always `A-WALL` for parametric walls. |

Detect parametric rows with: `ref` starts with `PW-`, or `thickness_mm` is present on a wall row.

### 3.2 Brick take-off wall (unchanged)

```json
{
  "ref": "W1A2",
  "level": "Ground",
  "length_mm": 5000.0,
  "height_mm": 2850.0,
  "layer": "MEASURE-FULLBRICK"
}
```

No `thickness_mm`. Layer is `MEASURE-FULLBRICK` or `MEASURE-HALFBRICK`. Height comes from the floor
ceiling when the brick take-off was saved.

A drawing may contain **both** kinds in one `walls` array.

## 4. What is not in the ShilpiDB / bridge payload

O-Draft keeps the following for its own UI and for `param.takeoff` (engine JSON). They are **not**
copied into the bridge wall row or the ShilpiDB wall record today:

| Field | Where it lives in O-Draft |
|---|---|
| `componentType` (`osas.wall`) | Link facts / `param.takeoff` provenance |
| `schemaVersion` (1) | same |
| `material` (`brick.full`, `brick.half`, `block.concrete`, `rcc`) | same |
| `finish` (`plaster.internal`, `plaster.external`, `none`) | same |
| `measurementRule` (`hcw-aqc-bridge/1#walls`) | same |
| `graphRevision`, `inputHash`, `drawingRevision` | same |
| Face-line handles | same |

**Ask for O-CoreOp:** if finishes or material must drive BOQ rules, either:

- **A (preferred for v1):** O-CoreOp derives finishes from `layer` / project defaults / its own rules
  (current contract spirit), or
- **B:** Agree additive optional fields on the wall row (e.g. `material`, `finish`,
  `component_type`, `schema_version`) and O-Draft will start writing them in a later bridge revision.

Do not invent a `measure(component, rule, revision)` call on O-Draft unless O-CoreOp publishes that
API; O-Draft currently only pushes measured-fact rows.

## 5. Stale and detached walls

| State | Recorded? |
|---|---|
| `linked` (fingerprint and graph match) | Yes |
| `stale` (graph changed, source line moved, or faces hand-edited) | No — export warns |
| `detached` (user detached, or wall removed from graph) | No — lines stay as plain geometry |

O-CoreOp should treat a missing `PW-*` ref on a re-record as "no longer measured", same as a brick
wall whose handle disappeared (existing delete-on-re-record behaviour).

## 6. Measurement rule

Component schema names the rule id:

`hcw-aqc-bridge/1#walls`

Meaning for O-CoreOp (proposed mapping; confirm in CoreOp docs):

- Inputs: centre-line length, height, thickness (when present).
- O-Draft supplies gross measured length × height × thickness facts only.
- Net wall area, opening deductions (from opening rows kind 4), plaster/paint and costing stay in
  O-CoreOp.

## 7. Suggested O-CoreOp work

1. **Reader:** accept wall rows with optional `thickness_mm`; keep brick rows without it.
2. **Identity:** treat `ref` values starting with `PW-` as stable wall ids (do not parse as `W`+handle).
3. **BOQ:** when `thickness_mm` is present, use it; when absent, keep today's brick/half-brick
   thickness assumption from `layer`.
4. **Optional:** request material/finish on the row (section 4B) if CoreOp cannot infer finishes.
5. **Smoke test:** open a `.takeoff.vdb` produced by O-Draft after `param.commit` + `takeoff.record`
   and confirm header + kind-3 records round-trip. O-Draft has not yet run CoreOp's reader against
   a file it wrote (known gap in [takeoff-shilpidb.md](takeoff-shilpidb.md)).

### Minimal reproduction on O-Draft

```text
# metric drawing; then via control channel or aad-cli:
param.wall.new { "points": [[0,0],[5000,0]], "height": "3 m", "thickness": 230 }
param.commit {}
takeoff.record {}          # → <drawing>.takeoff.vdb
# or: bridge.export { "write": true }
```

Engine tests that lock the row shape: `cargo test -p aad-engine --test param`.

## 8. Explicitly not done on O-Draft

- Node-graph editor UI (WinUI / egui).
- Columns, beams, slabs as parametric components.
- ShilpiDB-hosted component library.
- PDF underlay take-off.
- `wall_ref` on openings.
- Provenance fields inside the ShilpiDB wall payload (section 4).

## 9. Questions for O-CoreOp

1. Confirm optional `thickness_mm` on wall kind 3 is accepted without a contract version bump.
2. Prefer section 4A (infer finishes) or 4B (add `material` / `finish` to the wall row)?
3. Should `PW-*` refs be reserved in the CoreOp contract text, or is "any string `ref`" enough?
4. After CoreOp reads an O-Draft `.takeoff.vdb` with a parametric wall, paste any schema mismatch
   back here so O-Draft can adjust the emitter.
