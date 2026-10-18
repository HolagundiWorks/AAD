# hcwCAD-KIT port

hcwCAD-KIT (MIT, Holgundi Consulting Works) is an AutoCAD plugin in C#. Its tools are rebuilt
here in Rust as built-in commands (`crates/engine/src/cmd/`), not as an add-on. Each tool is a
normal command: JSON form, menu entry under **Architecture**, tests. Behaviour reference: the
kit's README (`/HolagundiWorks/hcwCAD-KIT`). No C# source is included.

## Done

| Area | AAD commands | Notes |
|---|---|---|
| Layer sets | `hcwlayers` `{set: hcw\|bp\|both}` | Creates missing layers only; unknown linetypes fall back to Continuous |
| Room tools | `room.relabel`, `room.log`, `room.total`, `room.audit`, `room.check` | From `hcwtools.lsp` section 11 (MRELABEL, MSCHEDULE, MTOTAL, MAUDIT, MCHECK). The three lines of a room label are paired by position, not by drawing order, so a log and a total are right whatever order the labels were drawn in. MFLOOR is the active floor of `floors`, which `room` already uses |
| Building permission (BPLT) | `bpltstart`, `bpltlayers`, `bpltcopy`, `bpltcheck`, `bpltarea`, `bpltreport`, `bplttitleblock` | From `hcwtools.lsp` section 6 and 9, for the BBMP BPAS / AutoPlan workflow: metres at 1:1, 65 BP- drafting layers and 27 non-plotting AP- marking layers. `bpltstart` scales the model to metres from `INSUNITS` once (skipping locked layers) and sets `BPLT-DIM`. `bpltcheck` and `bpltarea` read closed lightweight polylines only |
| HCW layer tools | `hcwreset`, `hcwinfo`, `hcwlock`, `hcwunlock`, `hcwaudit2`, `hcwbyblock`, `hcwmove`, `hcwschedule`, `hcwlegend` | From `hcwtools.lsp` section 7 and 10. The standard has 36 layers (the toolkit says 35). HCWPURGE and HCWAUDIT are the existing `purge` and `audit`; HCWLAYERSTATE is `layerstate.*`. The legend is sized as for metres at 1:1 and scaled to the drawing units |
| Text tools | `fixtxt`, `fixtxth`, `txtalign`, `txtdup`, `txtaudit`, `txtexport`, `txtinc` | From the AutoLISP toolkit `hcwtools.lsp` (same authors). `txtalign` sets the insertion point on one axis; `txtinc` is JSON-only (no prompt sequence yet) |
| Rooms | `room` (aliases `hcwroom`, `roomc`), `rarea` | 29 room codes; label with name, size and area; units from the drawing units |
| Project data | `project`, `floors`, `beams`, `levels` | Stored in the drawing (`Drawing.project`, DXF XRECORD `AAD_PROJECT`). `floors` rejects a ceiling height above the floor-to-floor height |
| Take-off | `takeoff.start` and `takeoff.linear\|brick\|beams\|columns\|rects\|paint\|ceiling\|floor\|area\|slab`, `takeoff.list`, `takeoff.clear`, `takeoff.export` | Reads the `MEASURE-*` layers. Walls are numbered left to right, then bottom to top; equal lengths or areas share a group (FB-A, FB-B ...). Metric results round to 1 cm, imperial to 1/8 in. Tables are drawn in the drawing and results are saved with it. CSV export only |
| Deductions | `MEASURE-DEDUCT` lines, including lines inside blocks | Door and window blocks made by `wall.door` and `wall.window` carry their own deduction line |
| Rates and bill | `takeoff.rates`, `takeoff.bill` | Rate book per item, bill of quantities from the saved take-off results |
| Schedules | `schedule`, `openings.std`, `openings.sync`, `openings.table` | Standard door and window set, counts synchronised from the blocks in the drawing, table drawn on request |
| Walls and openings | `wall.draw`, `wall.door`, `wall.window`, `wall.clean` | Faces mitred at corners, end caps on open ends, openings cut both faces; `wall.clean` erases zero-length and duplicate lines and joins overlaps |
| Structure | `grid.axis`, `column.place`, `column.schedule`, `lintel.draw`, `stair.plan`, `lift.shaft`, `rail.draw` | The grid is remembered; columns go on its intersections; lintels get a 150 mm bearing each side; stairs report risers, goings and concrete volume |
| Levels and symbols | `level.mark`, `level.schedule`, `section.marker`, `elevation.marker`, `north.arrow`, `slope.arrow` | Level marks are stored so `level.schedule` lists them in ascending order |
| Electrical layout | `electrical.blocks`, `electrical.number`, `electrical.layers`, `electrical.connect`, `electrical.schedule`, `electrical.update` | 17 symbol codes (SB, LP, FP, EF, SW1, SW2, CB, P5, P15, AC, WP, GY, FR, OV, WF, TV, INV). A point that reaches two boards is reported, not counted |
| AQC bridge | `bridge.export`, `bridge.import` | File format `hcw-aqc-bridge` version 1, millimetres. Export writes project details, floors, beam depths, the opening schedule, door and window blocks, and walls, columns, lintels and slabs from the saved take-offs, beside the drawing as `name.aqc.json`. Import reads AQC's project details, floors and beam depths; it previews first, applies only on `apply` (the Apply button in the menu flow), leaves filled fields alone unless `overwrite`, asks for confirmation when the file came from another drawing, and never changes geometry. Not exported: rooms (no room log yet) and wall thickness |

Limits that are stated, not hidden: electrical layout does not size circuits, compute loads or
measure cable length; `stair.plan` draws a plan only; no section is drawn from the plan.

## To do (in order)

1. Drawing tools: `DBCOUNT`, `DGRID`, `AUTOLABEL`, `INCARRAY`, `RENUMBERLAYOUTS`, `ROOMTEXTFIT`, `RTAG`, polygon area.
2. `VHLAYERS`, `LAYERMAP`, `TXTSTYLE`.
3. Notes and fields, title block.
4. Wall edit, join, regenerate, hatch and adopt; escalators; column table; auto-dimension; sheet set and fit.

## Not ported

Ribbon, installers, host (BricsCAD / ZWCAD) layers, Excel export (CSV only unless asked).
