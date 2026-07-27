# Architecture Overview

AADT is layered so the **CAD core is the substrate** — a document model, a
geometry kernel, and a command interpreter — with drafting capabilities
(detection, standards, libraries, documentation, PDF, LISP) as layers on top.
The core's job is to produce clean, structured geometry; the capability layers
consume that model.

```
┌───────────────────────────────────────────────────────────────┐
│                      AADT Application (aadt-app / aadt-cli)     │
├───────────────────────────────────────────────────────────────┤
│                     CAPABILITIES (on the core)                 │
│   aadt-detect  │  aadt-docs  │  aadt-standards  │  aadt-pdf     │
├───────────────────────────────────────────────────────────────┤
│                     CAD CORE (substrate)                       │
│  aadt-command  │  aadt-geometry  │  aadt-lisp  │  aadt-library  │
│                 aadt-model (document / undo)                    │
├───────────────────────────────────────────────────────────────┤
│  File I/O: aadt-dxf (DXF + DWG import) · aadt-store (.aadt)      │
├───────────────────────────────────────────────────────────────┤
│  Rendering: aadt-render · FFI: aadt-ffi                         │
└───────────────────────────────────────────────────────────────┘
```

## Crate map

| Spec layer (§3) | Crate(s) |
|---|---|
| Rendering | `aadt-render` |
| File I/O (DXF, DWG import, native `.aadt`) | `aadt-dxf`, `aadt-store` (ADR-0002) |
| Document model / undo | `aadt-model` |
| Geometry kernel | `aadt-geometry` |
| Command interpreter | `aadt-command` |
| Standards / libraries | `aadt-standards`, `aadt-library` |
| LISP engine | `aadt-lisp` |
| PDF markup & comparison | `aadt-pdf` |
| Element detection | `aadt-detect` |
| Documentation & sheet production | `aadt-docs` |
| Application shell | `aadt-app`, `aadt-cli`, `aadt-ffi` |

## Data flow

```
DXF / DWG / .aadt ──▶ aadt-dxf | aadt-store ──▶ aadt-model (Document)
                                                    │
                                                    ▼
                                              aadt-detect  ── walls, wall graph, rooms
                                                    │
                                                    ▼
                                              aadt-render ── drawlist ──▶ egui / GPU / PDF
```

Key properties:
- **Deterministic spine.** `aadt-dxf → aadt-model → aadt-detect` is pure and
  order-independent: the same input always yields the same output (spec §8). The
  `Document` iterates entities in ascending id order so nothing downstream
  depends on insertion order.
- **Plot paperwork** is also deterministic: nameplate presets + editable standard
  notes persist on `DrawingProperties` and are applied at PDF export (see title
  block / notes modules).

## GUI interaction (Spacebar-primary, AutoCAD-like)

Mouse = **position**, keyboard = **intent**, Space = **execute**
([`command_engine`](../../crates/aadt-app/src/command_engine.rs)).

AADT follows classic AutoCAD command-loop rules (Enter≈Space, keyword options,
noun-verb **and** verb-noun selection, dyn prompts) without chasing AutoCAD’s
full command count. Deep dive + step tables:

- [UX index](../ux/README.md)
- [AutoCAD research notes](../ux/autocad-research.md)
- [Command workflows & parity matrix](../ux/command-workflows.md)

| Key | Action |
|-----|--------|
| Space | Accept typed value, or next/finish |
| Double-Space | Finish + repeat previous command |
| Hold Space / Ctrl+Space | Command palette (recent + fuzzy) |
| Enter / right-click | Same as Space |
| Esc | Cancel to idle |
| Tab | Cycle X ↔ Y while drafting a point |
| Shift+Space | Clear current typed input |
| F1 | Shortcuts sheet |
| F3 / F8 / F10 / F11 | Osnap / Ortho / Polar / Osnap tracking |

Aliases (examples): `L` line, `PL` polyline, `C` circle, `M` move, `CO` copy,
`SC` scale, `TR` trim, `DIM` aligned dimension, `O` offset, `W` wall.
Typed digits + Space still commit distance / thickness / coordinates / scale
factors / rotate angles.

**Selection:** click or window/crossing; selected entities (including expanded
block inserts) highlight in blue with grips.

**Units / dims:** `UNITS ft-in|m` and optional secondary labels; `DIM` is
three-point aligned with AutoCAD-like glyphs (see workflows doc).

## GUI performance & workflow polish

- **Drawlist cache.** `Session::drawlist` rebuilds only when `Document::revision`
  changes.
- **Repaint gating.** The viewport requests a repaint on meaningful pointer motion,
  active drafting, or Space-hold — not continuous idle hover.
- **Autosave / recovery.** Dirty drawings with a known path autosave to a
  `*.autosave.aadt` sibling on a timer; on open, a newer autosave prompts
  Restore / Discard. Prefs (`ortho` / `osnap` / panel width / …) live in
  `%APPDATA%/AADT/prefs.json` (or XDG config).

## GUI chrome (Windows 11 / Fluent)

The egui shell follows Microsoft’s
[design principles](https://learn.microsoft.com/en-us/windows/apps/design/design-principles)
(Effortless, Calm, Familiar) and
[navigation basics](https://learn.microsoft.com/en-us/windows/apps/design/basics/navigation-basics):
calm layered surfaces, accent only for selection/status, flat peer destinations
in the right manager (tab + collapsed nav rail), and familiar File/Edit commanding
in the top bar. Tokens live in `crates/aadt-app/src/gui_theme.rs`.

## Related

- [Roadmap](../ROADMAP.md)
- [Industry-standards audit (July 2026)](../audit/2026-07-industry-standards-audit.md)
- [CHANGELOG](../CHANGELOG.md)
