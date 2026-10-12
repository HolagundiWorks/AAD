# Architecture

How the code on `main` is built. Planned work is in [status.md](status.md). The rules that keep this
shape (layering, never-crash, everything-is-a-command, clean-room) are in [CLAUDE.md](../CLAUDE.md).

## Workspace

13 library crates, 3 apps and `xtask`. `cargo xtask layers` enforces the layering: nothing below L5
depends on egui, eframe, winit, wgpu or rfd.

| Layer | Crate | What it does |
|---|---|---|
| L0 | `geom` | f64 vectors and matrices, bounds, lines, arcs and bulges, circles, ellipses, B-splines, intersections, tessellation, planar region tracing for HATCH and BOUNDARY |
| L0 | `dxf` | Standalone DXF tag reader and writer (ASCII and binary), tag-count limits for hostile input |
| L1 | `color` | Colour index table, true colour, ByLayer and ByBlock, display colour against the background |
| L1 | `doc` | The drawing database: header, symbol tables, blocks, model space, layouts; copy-on-write `EntityStore`; hatch pattern and linetype library (`library.rs`) |
| L2 | `fonts` | Single-stroke drafting font, TrueType/OpenType outlines through `skrifa`, text and MTEXT layout, font fallback |
| L2 | `render` | Display lists (linetypes, hatches, text, blocks, dimension geometry, paper-space clipping); CPU rasteriser for PNG, plot preview and tests |
| L3 | `io` | DXF to document mapping (`dxf_read.rs`, `dxf_write.rs`, AAD extension data in `dxf_ext.rs`), PDF plotting, SVG and PNG export |
| L3 | `dwg` | DWG through a DXF bridge: the `acadrust` crate (MPL-2.0, unmodified) converts DWG and DXF bytes; native builds only. The apps run the conversion in a worker process (`isolated.rs`) so a damaged file cannot abort the program |
| standalone | `llm` | Offline language-model runtime facade (Qwen3 prompt format, model file check). The llama.cpp backend is behind the off-by-default `llama` feature and is not wired into the apps yet ([ai-agent.md](ai-agent.md)) |
| L4 | `engine` | `Session`, the command registry (`cmd/*`), command-line parser and prompt machines, object snaps, selection, grips, associativity, system variables, units, undo, audit (`audit.rs`), flatten on import (`flatten.rs`) |
| L5 | `ui-egui` | The egui front end: GPU canvas, menus, command line, palettes, dialogs, icons drawn in code, light/dark theme, interface string catalog |
| L5 | `mcp` | MCP server (JSON-RPC 2.0 over stdio) on a headless session or bridged to a running app |
| L5 | `agent` | Tool layer for a local language model: registry, validation, confirmation, loop limits, audit log ([ai-agent.md](ai-agent.md)) |
| app | `cadcraft` | Desktop app (eframe/wgpu) |
| app | `cadcraft-cli` | `info`, `convert`, `run` (scripts), `commands`, `sample`, `mcp` |
| app | `cadcraft-web` | WASM build of the same UI (no DWG) |
| tool | `xtask` | `ci`, `layers`, `assets`, `wasm`, `parity`, `version`, `stats` |

## Data model

- A `Drawing` is header variables, symbol tables, block definitions, model space and paper-space
  layouts. Entities carry a handle, layer, colour, linetype, lineweight, transparency and an
  `EntityKind`: Line, Point, Circle, Arc, Ellipse, LwPolyline, Spline, Ray, XLine, Text, MText,
  AttDef, Insert (with attributes), Dimension, Leader, MLeader, Hatch (pattern, solid, gradient),
  Solid, Trace, Viewport, Image, Wipeout, Table and Unknown (raw DXF tags).
- 3D polylines and 3D faces are not entity kinds: the reader projects them to 2D. 3D solids,
  surfaces and meshes are kept as `Unknown` and reported on import ([maintenance.md](maintenance.md)).
- Points keep a z coordinate so that DXF round-trips; drafting is 2D in the world coordinate
  system and there is no UCS.
- Entity collections are copy-on-write, so undo is a whole-drawing snapshot that costs little.
- Table entities read from files are displayed, but there are no table commands.

## Commands

Every user-visible action is a `CommandSpec` (`crates/engine/src/cmd/mod.rs`): id (the command name
in lower case), label, menu path, shortcut, aliases, a JSON `run`, an `enabled` check and an
optional interactive prompt machine. There are 351 engine commands (135 with interactive prompts;
`cadcraft-cli commands` lists them) and 47 UI-only commands in `crates/ui-egui/src/menus.rs`. The
command line, menus, toolbar, tool sets, scripts, CLI, control channel, MCP and the agent tools all
reach the same commands. JSON calls never open dialogs.

## File import

`open`, drag and drop and `recover` read a file through the `IoHooks` installed by the host, then
call `cmd::file::import_drawing`, which flattens a working copy to 2D and keeps the original as the
saved state and one undo step. Saving keeps the first replaced file as `<name>.bak`. See
[maintenance.md](maintenance.md).

## Rendering

`render::build` turns a space into a display list once per change; the UI uploads it to wgpu in
batches (`gpu.rs`). Tessellation tolerance follows the view. The R-tree (`spatial.rs`) is patched per
changed store chunk, so picking stays fast on large drawings. PNG, PDF, SVG and plot preview reuse
the same display list through the CPU rasteriser or vector writers. The drawing-area background is
black in the dark theme and white in the light theme; colour 7 is drawn white or black to contrast.

## Files

DXF is the native path: `dxf` parses tags, `io::dxf_read` maps them to the document, and
`io::dxf_write` writes an R2000 (AC1015) DXF. DWG is converted to DXF in memory by `acadrust` and
read through the same path; saving DWG writes DXF and converts it back. AAD-only data
(associativity details, layer states, plot style tables, table flags, hatch gradients) travels in
extension data under the application name `CADCRAFT`. Object types AAD does not model are kept
as raw data when read and are not written back; saving reports them.

## Automation

- Control channel: `cadcraft --control PORT` accepts JSON lines (`cmdline.input`, `engine.execute`,
  `ui.screenshot`, `ui.render`, ...); see [control-protocol.md](control-protocol.md).
- MCP: `cadcraft-cli mcp` (headless) or `--connect HOST:PORT` (the running app); see [mcp.md](mcp.md).
- CLI: `cadcraft-cli run --script ... --export out.png` for batch work.
- Agent tools: `crates/agent`; see [ai-agent.md](ai-agent.md).

## Safety

Production crates deny `unwrap`, `expect` and `panic!` and forbid `unsafe`. `Session::execute` and
interactive input run under `catch_unwind`, restoring the drawing if a panic escapes. Input-sized
allocations are capped (DXF tag counts, DWG size limits, hatch segment and island limits,
`MAX_BLOCK_DEPTH`). The quality gate is `cargo xtask ci`.

## Undo and redo

`Session::execute` keeps the drawing as an `Arc<Drawing>` before each undoable command and pushes a
`Snapshot` (label, previous drawing, previous selection) when the command changed it. Because the
entity store is copy-on-write in chunks of up to 1 024 entities, a snapshot shares every chunk the
command did not touch. History is capped at 2 000 steps per document. Commands marked `noundo`
(for example save) do not add a step. After a command, edits that would leave NaN or infinite
coordinates are refused and the drawing is restored (`finite.rs`).

## Threading

The UI, the engine and rendering run on the thread that draws the window. Threads that exist:

- `spatial::warm` builds the picking index in the background when the display list is rebuilt
  (`aad-spatial-warm`, one at a time; not on wasm).
- The DWG worker process and its feeder and reader threads (`dwg/src/isolated.rs`).
- The control-channel server (one thread per connection, `apps/cadcraft/src/control_server.rs`),
  which hands requests to the UI thread over a channel.

File open, save and plot run on the UI thread; there is no progress state yet
([UI_BUG_BACKLOG.md](UI_BUG_BACKLOG.md), UI-003).

## External dependencies

Rendering and windowing: `egui`/`eframe` 0.36, `egui-wgpu`, `wgpu`. Geometry and spatial queries:
`rstar`. Fonts: `skrifa`. DWG: `acadrust`. Local model (optional): `llama-cpp-2`. Tests and
benchmarks: `egui_kittest`, `proptest`, `criterion`. The full list is in `Cargo.lock`.
