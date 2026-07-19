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
│              on-demand, local, Ollama subprocess               │
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

## AI lifecycle discipline (spec §6.1)

1. User selects entities and invokes an `AADT` command.
2. The app extracts structured data (lengths, areas, counts, attributes).
3. An Ollama subprocess spawns if not warm.
4. A structured-JSON prompt goes to `localhost:11434`; a structured-JSON
   response is expected.
5. The response is applied as a normal undoable CAD operation.
6. Ollama stays warm for a short idle window (~2–3 min), then unloads.
7. If Ollama is unavailable or slow, the action fails gracefully with a
   command-line message; drafting and deterministic takeoff are never blocked.
