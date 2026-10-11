# hcwCAD-KIT port

hcwCAD-KIT (MIT, Holgundi Consulting Works) is an AutoCAD plugin in C#. Its tools are rebuilt
here in Rust as built-in commands (`crates/engine/src/cmd/hcw.rs` and siblings), not as an add-on.
Each tool is a normal command: JSON form, prompt form, menu entry, tests. Behaviour reference:
the kit's README (`/HolagundiWorks/hcwCAD-KIT`).

## Done
| Kit command | CADCraft command | Notes |
|---|---|---|
| Layer sets (HCW standard, BPLT) | `hcwlayers` `{set: hcw\|bp\|both}` | Creates missing layers only; unknown linetypes fall back to Continuous |
| `ROOM`, `ROOMC` | `room` (aliases `hcwroom`, `roomc`) | Two corners, 29 room codes, rectangle + name / size / area text, units from `INSUNITS` |
| `RAREA` | `rarea` | Area label at the centre of a closed polyline |

## To do (in order)
1. Room extras: room log saved in the drawing, `RTAG`, `ROOMTEXTFIT`, room table / schedule / total, polygon area.
2. Notes and fields: notes library, sheet fields, `FIXTXT`.
3. Take-off: `MLIN`, `MBRK`, `MBML`, `MCOL`, `MREC`, `MPAINT`, `MCEIL`, `MFLOOR`, `MAREA`, `MSLAB`, schedule, rates, CSV export.
4. Generators: stairs, lifts, rails, electrical layout, section and elevation markers, levels, wall / door / window tools.
5. Project data, floors, bridge file, permit-layer move.

## Not ported
Ribbon, installers, host (BricsCAD / ZWCAD) layers, Excel export (CSV only unless asked).
