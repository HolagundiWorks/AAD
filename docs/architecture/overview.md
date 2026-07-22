# Architecture Overview

AADT is layered so that the **AADT Engine sits above the CAD core as its own
layer**, consuming the document model rather than being one command among many.
The CAD core's job is to produce clean, structured geometry; the AADT Engine's
job is to turn that geometry into identified elements, quantities, and finished
documentation.

```
┌───────────────────────────────────────────────────────────────┐
│                      AADT Application (aadt-app)                │
├───────────────────────────────────────────────────────────────┤
│                     AADT ENGINE (flagship layer)               │
│   aadt-detect     │     aadt-takeoff      │     aadt-docs       │
│   (elements)      │     (quantities)      │  (sheets/notes)     │
│                   AI Orchestration (aadt-ai)                    │
│         on-demand, local — Foundry Local (default)              │
├───────────────────────────────────────────────────────────────┤
│                     CAD CORE (substrate)                       │
│  aadt-command  │  aadt-geometry  │  aadt-lisp  │  aadt-pdf      │
│              aadt-model (document model / undo)                 │
├───────────────────────────────────────────────────────────────┤
│        File I/O: aadt-dxf (DXF now, DWG later) | native        │
├───────────────────────────────────────────────────────────────┤
│                Rendering: aadt-render (wgpu 2D)                 │
└───────────────────────────────────────────────────────────────┘
```

## Crate map

| Spec layer (§3) | Crate(s) |
|---|---|
| Rendering | `aadt-render` |
| File I/O (DWG/DXF, native) | `aadt-dxf` (native format: see ADR-0004) |
| Document model / undo | `aadt-model` |
| Geometry kernel | `aadt-geometry` |
| Command interpreter | `aadt-command` |
| LISP engine | `aadt-lisp` |
| PDF markup & comparison | `aadt-pdf` |
| Element detection & classification | `aadt-detect` |
| Takeoff & quantity engine | `aadt-takeoff` |
| Documentation & sheet production | `aadt-docs` |
| AI orchestration | `aadt-ai` |
| Application shell | `aadt-app`, `aadt-cli` |

## Data flow (the M0 pipeline, extended by later milestones)

```
DXF/DWG ──▶ aadt-dxf ──▶ aadt-model (Document)
                              │
                              ▼
                        aadt-detect  ── walls, wall graph, rooms
                              │
                              ▼
                        aadt-takeoff ── quantities ──▶ CSV / Excel / tables
                              │
                              ▼  (optional, explicit, on-demand)
                         aadt-ai ── wall-type inference, formatting, anomalies
```

Key properties:
- **Deterministic spine.** `aadt-dxf → aadt-model → aadt-detect → aadt-takeoff`
  is pure and order-independent: the same input always yields the same output
  (spec §8). The `Document` iterates entities in ascending id order so nothing
  downstream depends on insertion order.
- **AI is a branch, never a gate.** `aadt-ai` reads **structured data** produced
  by the deterministic spine (never raw geometry or images) and applies results
  as ordinary undoable operations. If AI is unavailable, the spine still
  produces complete, trustworthy quantities.
- **Plot paperwork** is also deterministic: nameplate presets + editable standard
  notes persist on `DrawingProperties` and are applied at PDF export (see title
  block / notes modules).

## GUI interaction (Spacebar-primary)

Mouse = **position**, keyboard = **intent**, Space = **execute**
([`command_engine`](../../crates/aadt-app/src/command_engine.rs)):

| Key | Action |
|-----|--------|
| Space | Accept typed value, or next/finish |
| Double-Space | Finish + repeat previous command |
| Hold Space / Ctrl+Space | Command palette (recent + fuzzy) |
| Enter / right-click | Same as Space |
| Esc | Cancel to idle |
| Tab | Cycle X ↔ Y while drafting a point |
| Shift+Space | Clear current typed input |

Aliases (examples): `L` line, `PL` polyline, `C` circle, `CO` copy, `O` offset,
`W` wall, `D` door, `WA` fuzzy→wall. Typed digits + Space still commit distance /
thickness / coordinates.

## AI lifecycle discipline (spec §6.1)

1. User selects entities and invokes an `AADT` command (or `aadt infer`).
2. The app extracts structured data (lengths, areas, counts, attributes).
3. A JSON-only prompt is sent to the **inbuilt** local backend —
   **Foundry Local** OpenAI-compatible Chat Completions (default endpoint
   `http://127.0.0.1:5273`, model `phi-3.5-mini`). Optional: `--backend ollama`.
4. A structured-JSON response is expected and parsed.
5. The response is **proposed** as classification attributes (`RECOGNIZE` /
   Propose → Review → Freeze; see [ADR-0007](../adr/0007-propose-review-freeze.md)).
   Geometry is never rewritten. Freeze is an explicit lock; re-infer skips frozen.
6. No AADT background AI thread; Foundry Local’s own service owns process
   lifetime. Optional AADT-owned hosts still use poll-driven warm/unload in
   `aadt_ai::lifecycle`.
7. If the local model is unavailable or slow, the action fails gracefully with a
   command-line message; drafting and deterministic takeoff are never blocked.
