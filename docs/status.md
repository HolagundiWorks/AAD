# Implemented and planned functionality

This page separates what exists (and is tested) from what is only planned. If a feature is not in
the first table it is not implemented. Scope: 2D drafting for architecture and civil work.
Out of scope and removed: 3D modelling and viewing, parametric constraints, and
mechanical-engineering commands (mass properties, ordinate and arc-length dimensions, centre
marks, isometric drafting).

## Implemented

| Area | Functionality |
|---|---|
| Files | Open and save DXF; open and save DWG through a DXF bridge (native builds only); export PDF, SVG, PNG. Flatten on import, `.bak` before the first overwrite ([maintenance.md](maintenance.md)) |
| Drawing | Line, polyline, circle, arc, ellipse, rectangle, polygon, spline, point, construction line, ray, multiline, revision cloud, wipeout, hatch, boundary, region, text, multiline text, blocks and attributes |
| Modify | Move, copy, rotate, scale, mirror, stretch, offset, trim, extend, fillet, chamfer, break, join, explode, lengthen, align, array (rectangular, polar, path), overkill, polyline and spline editing, group, match properties, flatten |
| Dimensions | Linear, aligned, radius, diameter, angular, baseline, continue, quick dimension, multileader, dimension styles, text edit, associativity |
| Layers and properties | Layer manager and layer tools, layer states, linetypes, lineweights, colours, plot styles, text and dimension styles |
| Layouts and output | Paper-space layouts, viewports, page setup, PDF plotting |
| Maintenance | `PURGE`, `AUDIT`, `RECOVER`, `FLATTEN` ([maintenance.md](maintenance.md)) |
| Architecture: layers and rooms | `hcwlayers` (HCW and building-permission layer sets), `room` (room labels with area), `rarea` ([hcw-kit-port.md](hcw-kit-port.md)) |
| Architecture: walls and openings | `wall.draw` (mitred faces and end caps), `wall.door`, `wall.window` (cut the wall lines, place a block that carries a deduction line, link the schedule entry), `wall.clean` |
| Architecture: structure | `grid.axis`, `column.place`, `column.schedule`, `lintel.draw`, `stair.plan` (straight or dogleg, risers, goings, concrete volume), `lift.shaft`, `rail.draw` |
| Architecture: levels and symbols | `level.mark`, `level.schedule`, `section.marker`, `elevation.marker`, `north.arrow`, `slope.arrow` |
| Project data | `project`, `floors`, `beams`, `levels`: kept in the drawing (DXF extension data `CADCRAFT_PROJECT`) and restored on open |
| Quantity take-off | `takeoff.start`, `takeoff.linear`, `takeoff.brick`, `takeoff.beams`, `takeoff.columns`, `takeoff.rects`, `takeoff.paint`, `takeoff.ceiling`, `takeoff.floor`, `takeoff.area`, `takeoff.slab`, `takeoff.list`, `takeoff.clear`, `takeoff.export` (CSV), `takeoff.rates`, `takeoff.bill`; opening deductions from blocks |
| Schedules | `schedule`, `openings.std`, `openings.sync`, `openings.table` (door and window schedule drawn as a table) |
| Electrical layout | `electrical.blocks`, `electrical.number`, `electrical.layers`, `electrical.connect`, `electrical.schedule`, `electrical.update` (17 symbol codes, boards and circuits; no load or cable sizing) |
| AQC bridge | `bridge.export` and `bridge.import`: project, floors, beam depths, schedule and measured quantities to and from the AQC estimation program ([hcw-kit-port.md](hcw-kit-port.md)) |
| Xrefs and floor projects | `xref.attach`, `xref.reload`, `xref.detach`, `xref.bind`, `xref.list`; `project.setup`, `project.open`, `floor.list`, `floor.activate`, `floor.set`: a project of floor and layout drawings joined by relative Xrefs, with per-floor visibility, locking and print flags ([projects.md](projects.md)) |
| Sheets and printing | Live viewports; `sheet.titleblock`, `sheet.fields`, `sheet.template.*`, `viewport.list/add/remove`, `floor.viewport`; `plot.batch` and the Plot window ([sheets.md](sheets.md)) |
| Widgets | Architect calculator (`cal`, `ui.toggle.calculator`) and Pomodoro timer (`ui.toggle.pomodoro`) ([widgets.md](widgets.md)) |
| Input forms | Dialogs and command-line prompts for the architecture commands ([input-forms.md](input-forms.md)) |
| Interface | Pro and Classic workspaces ([workspaces.md](workspaces.md)), property toolbar, Blocks panel, pin and auto-collapse; command line with prompts, object snaps, polar and ortho tracking, grips, dynamic input, quick properties, light/dark theme toggle (black or white drawing area), English and Ukrainian interface |
| Automation | JSON control channel, MCP server, command-line tool, scripts ([control-protocol.md](control-protocol.md), [mcp.md](mcp.md)) |
| Units and annotation | Drawing and display units, plot scale and paper, annotation sizing, collision analysis and text layout ([units-annotation.md](units-annotation.md)) |
| AI agent tool layer | Allow-listed, validated tools for flatten, audit, purge, recover, summary, units, plot configuration and annotation layout ([ai-agent.md](ai-agent.md)) |

## Planned (not implemented)

| Item | Notes |
|---|---|
| macOS, Linux and web builds | Later phases. Only Windows is built, tested and packaged now |
| Qwen3-1.7B runtime, chat UI, retrieval | [ai-agent.md](ai-agent.md) |
| Remaining hcwCAD-KIT tools | Text and drawing tools (`FIXTXT`, `TXTALIGN`, `INCARRAY`, ...), layer and permit-layer tools, notes and fields, wall edit and join, escalators, auto-dimension ([hcw-kit-port.md](hcw-kit-port.md)) |
| Agent tools for take-off and schedules | The engine commands exist; the agent registry does not expose them yet ([ai-agent.md](ai-agent.md)) |
| Xref and floor-project interface | Floor selector in the toolbar, setup wizard dialog, Floor View Manager, opening-block generation and reconciliation ([projects.md](projects.md)) |
| Conversion of 3D solids, surfaces and meshes | They are kept as raw data and reported on import, not converted |
| User coordinate systems, SHX fonts, DWG on the web | Not available |
