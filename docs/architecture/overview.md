# Architecture Overview

AAD is layered so the **CAD core is the substrate** — a document model, a
geometry kernel, and a command interpreter — with drafting capabilities
(detection, standards, libraries, documentation, PDF, LISP) as layers on top.
The core's job is to produce clean, structured geometry; the capability layers
consume that model.

```
┌───────────────────────────────────────────────────────────────┐
│                      AAD Application (aadt-app / aadt-cli)     │
├───────────────────────────────────────────────────────────────┤
│              AI-NATIVE INTELLIGENCE (ADR-0012, M10–M13)        │
│  aadt-object │ aadt-graph │ aadt-embed │ aadt-vecdb            │
│  aadt-assist (→ aadt-pdf, aadt-detect, aadt-vecdb) │ aadt-takeoff │
├───────────────────────────────────────────────────────────────┤
│                     CAPABILITIES (on the core)                 │
│   aadt-detect  │  aadt-docs  │  aadt-standards  │  aadt-pdf     │
├───────────────────────────────────────────────────────────────┤
│                     CAD CORE (substrate)                       │
│  aadt-command  │  aadt-geometry  │  aadt-lisp  │  aadt-library  │
│                 aadt-model (document / undo)                    │
├───────────────────────────────────────────────────────────────┤
│  File I/O: aadt-dxf (DXF + DWG) · aadt-store (.aadt) · aadt-vdb (.vdb) │
├───────────────────────────────────────────────────────────────┤
│  Rendering: aadt-render · FFI: aadt-ffi · Local API: aadt-api    │
└───────────────────────────────────────────────────────────────┘
```

## Crate map

| Spec layer (§3) | Crate(s) |
|---|---|
| Rendering | `aadt-render` |
| File I/O (DXF, DWG import, native `.aadt`, ShilpiDB `.vdb`) | `aadt-dxf`, `aadt-store` (ADR-0002), `aadt-vdb` (ADR-0011) |
| Document model / undo | `aadt-model` |
| Geometry kernel | `aadt-geometry` |
| Command interpreter | `aadt-command` |
| Standards / libraries | `aadt-standards`, `aadt-library` |
| LISP engine | `aadt-lisp` |
| PDF markup & comparison | `aadt-pdf` |
| Element detection | `aadt-detect` |
| Documentation & sheet production | `aadt-docs` |
| Object DB / Graph DB (M10) | `aadt-object`, `aadt-graph` |
| Embeddings / vector search / corpus (M11) | `aadt-embed`, `aadt-vecdb` |
| AI service layer (M12, ADR-0013) | `aadt-assist` |
| Quantity takeoff / BOQ (M13) | `aadt-takeoff` |
| Application shell | `aadt-app`, `aadt-cli`, `aadt-ffi` |
| Local API (M14, ADR-0015) | `aadt-api` — HTTP/JSON over `Session`; reads project it, `POST /v1/command` is the one write path |

## Data flow

```
DXF / DWG / .aadt / .vdb ──▶ aadt-dxf | aadt-store | aadt-vdb ──▶ aadt-model (Document)
                                                    │
                        ┌───────────────────────────┼───────────────────────────┐
                        ▼                           ▼                           ▼
                  aadt-detect               aadt-embed/-vecdb            aadt-render
            walls, wall graph, rooms      FIND · ASKDOCS corpus       drawlist ──▶ egui / GPU / PDF
                        │                           │
                        ▼                           │
             aadt-object / aadt-graph               │
          objects, relations, audit (M10)           │
                        │                           │
            ┌───────────┴───────────┐               │
            ▼                       ▼               ▼
      aadt-takeoff            aadt-assist ◀─────────┘ (citations)
   TAKEOFF · BOQ (M13)   SUGGEST · CHECK · ASK · COMPARE · RECOMMEND
                              └── uses aadt-pdf::compare for the
                                  fingerprint diff behind COMPARE
```

Key properties:
- **Deterministic spine.** `aadt-dxf → aadt-model → aadt-detect` is pure and
  order-independent: the same input always yields the same output (spec §8). The
  `Document` iterates entities in ascending id order so nothing downstream
  depends on insertion order.
- **Assist proposes, never commits.** `aadt-assist` services return
  `Suggestion` records; only `apply` mutates the `Document`, inside one undo
  transaction with an audit row (ADR-0013). Advisory services — code checks,
  revision narration, detail recommendation — carry `Action::None` and cannot
  be applied at all.
- **Plot paperwork** is also deterministic: nameplate presets + editable standard
  notes persist on `DrawingProperties` and are applied at PDF export (see title
  block / notes modules).

## GUI interaction (Spacebar-primary, AutoCAD-like)

Mouse = **position**, keyboard = **intent**, Space = **execute**
([`command_engine`](../../crates/aadt-app/src/command_engine.rs)).

AAD follows classic AutoCAD command-loop rules (Enter≈Space, keyword options,
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
  Restore / Discard. Prefs (`ortho` / `osnap` / panel width / panel mode /
  floating panel rect / …) live in `%APPDATA%/AAD/prefs.json` (or XDG config).

## GUI chrome (Windows 11 / Fluent)

The egui shell follows Microsoft’s
[design principles](https://learn.microsoft.com/en-us/windows/apps/design/design-principles)
(Effortless, Calm, Familiar) and
[navigation basics](https://learn.microsoft.com/en-us/windows/apps/design/basics/navigation-basics):
calm layered surfaces, accent only for selection/status, flat peer destinations
in the right manager (tab + collapsed nav rail), and familiar File/Edit commanding
in the top bar. Tokens live in `crates/aadt-app/src/gui_theme.rs`.

**Panel docking (M9.6).** Each side manager carries a `PanelMode`
(`crates/aadt-app/src/prefs.rs`), persisted per side in `prefs.json`:

| Mode | Behaviour |
|---|---|
| `Docked` | Edge `SidePanel`; collapses to an icon rail |
| `Floating` | Free `egui::Window`; position and size remembered in `FloatRect` |
| `AutoHide` | Edge rail that expands a foreground overlay on hover, collapsing after a short grace period |

Panel headers expose Float / Auto-hide (unpin) / Pin (re-dock) / Collapse.
Commands and chrome that open a manager call `reveal_left_mgr` /
`reveal_right_mgr` so they work identically in all three modes.

## Related

- [AADT CAD + ShilpiDB developer architecture](./aadt-shilpidb-architecture.md) — target entity/database architecture with implementation-status map
- [Roadmap](../ROADMAP.md)
- [Industry-standards audit (July 2026)](../audit/2026-07-industry-standards-audit.md)
- [CHANGELOG](../CHANGELOG.md)
