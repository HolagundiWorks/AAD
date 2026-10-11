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
| Architectural tools | `hcwlayers` (HCW and building-permission layer sets), `room` (room labels with area), `rarea` ([hcw-kit-port.md](hcw-kit-port.md)) |
| Interface | Command line with prompts, object snaps, polar and ortho tracking, grips, dynamic input, quick properties, tool sets, light/dark theme toggle (black or white drawing area), English and Ukrainian interface |
| Automation | JSON control channel, MCP server, command-line tool, scripts ([control-protocol.md](control-protocol.md), [mcp.md](mcp.md)) |
| AI agent tool layer | Allow-listed, validated tools for flatten, audit, purge, recover, summary ([ai-agent.md](ai-agent.md)) |

## Planned (not implemented)

| Item | Notes |
|---|---|
| Qwen3-1.7B runtime, chat UI, retrieval | [ai-agent.md](ai-agent.md) |
| Schedule of openings, quantity take-off | Not in the engine; the AI tools for them do not exist. Plan in [hcw-kit-port.md](hcw-kit-port.md) |
| Remaining hcwCAD-KIT tools | Notes and fields, take-off, stairs, lifts, electrical layout, sections, project data ([hcw-kit-port.md](hcw-kit-port.md)) |
| External references (xref) | No attach, detach, reload or bind commands exist |
| Conversion of 3D solids, surfaces and meshes | They are kept as raw data and reported on import, not converted |
| User coordinate systems, SHX fonts, DWG on the web | Not available |
