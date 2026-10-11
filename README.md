# AAD - Accelerated Architecture Drafting

AAD is a 2D computer-aided drafting application written in Rust, aimed at architectural and civil
drafting. It reads and writes DXF and DWG, runs on Windows, macOS, Linux, FreeBSD and in the
browser (WebAssembly), and can be driven from scripts and agents. This repository is a fork of
[storytold/cadcraft](https://github.com/storytold/cadcraft) with the 3D, parametric-constraint and
mechanical features removed.

What is built and what is planned is listed in [docs/status.md](docs/status.md). Nothing in the
documentation describes a feature as available unless it exists and has tests.

## Build and run

Requires Rust 1.90 or later for the core crates; the desktop and web UI builds need Rust 1.95 or later.

```sh
cargo run --release -p cadcraft -- --sample    # desktop app with a sample drawing
cargo run --release -p cadcraft -- file.dxf    # open a file
```

Type commands at the command line, for example:

```text
line 0,0 @10,0 @0,5 c
circle 5,2 1
offset 0.25
zoom e
```

The web build: `cd apps/cadcraft-web && trunk serve` (needs [trunk](https://trunkrs.dev)).

## Tests

```sh
cargo xtask ci        # fmt, clippy -D warnings, tests, asset list, layering, wasm check
```

`cargo xtask --help` lists the other tasks (`layers`, `assets`, `wasm`, `parity`, `version`, `stats`).

## Command-line tool, MCP and control channel

```sh
cadcraft-cli info drawing.dxf
cadcraft-cli convert drawing.dxf drawing.svg
cadcraft-cli run --sample --script 'CIRCLE 22,3 1\n' --save out.dxf --export out.png
cadcraft-cli mcp                               # MCP server on stdio, headless
cadcraft-cli mcp --connect 127.0.0.1:7979      # ...or bridged to a running app (start it with --control 7979)
```

Image exports (PNG 2400x1600, SVG, PDF) fit the model-space extents; `--window x0,y0,x1,y1`
frames an exact rectangle and `--view extents|current` forces either framing. The JSON control
channel is described in [docs/control-protocol.md](docs/control-protocol.md) and the MCP tools in
[docs/mcp.md](docs/mcp.md).

## Opening files

Every DXF or DWG that is opened is flattened to 2D on a working copy, and the original stays
recoverable. `PURGE`, `AUDIT` and `RECOVER` keep a drawing healthy. Details, limits and safety
measures: [docs/maintenance.md](docs/maintenance.md).

## Built-in architecture tools

Walls, doors and windows, grid, columns, lintels, stairs, lifts, levels and section markers,
quantity take-off with rates and a bill, door and window schedules, electrical layout, and the
project data they share are built-in commands under the Architecture menu ([docs/hcw-kit-port.md](docs/hcw-kit-port.md)).
A calculator for feet, inches, lengths, areas and volumes and a Pomodoro timer are floating
widgets under Tools ([docs/widgets.md](docs/widgets.md)).

## Local AI assistant

An offline assistant can call a fixed set of validated tools (flatten, audit, purge, recover). The
tool layer is built and tested; the Qwen3-1.7B model runtime is not integrated yet. See
[docs/ai-agent.md](docs/ai-agent.md).

## Configuration

- Fonts: AAD ships none and uses the TrueType/OpenType fonts installed on the system. SHX fonts
  are not read; text in an SHX style is drawn with the built-in stroke font. `FONTALT` and
  `FONTFALLBACK` (or `CADCRAFT_FONTALT`, `CADCRAFT_FONTFALLBACK`) name a replacement font and the
  fonts tried first for characters a font lacks. `.` clears either.
- Logs: the desktop app writes to standard error and to `logs/cadcraft.log` in the settings
  directory (`~/.config/cadcraft/` on Linux and the BSDs, `~/Library/Application Support/AAD/`
  on macOS, `%APPDATA%\AAD\` on Windows). `CADCRAFT_CONFIG_DIR` overrides the directory and
  `RUST_LOG` sets the levels (default `info` for AAD's crates, `warn` for the rest). Each launch
  rotates the previous log; the file stops growing at 16 MiB.
- Theme: one light/dark toggle (View menu, status bar). Dark has a black drawing area, light a
  white one. There is no other colour customisation.
- Interface language: English or Ukrainian, see [docs/localization.md](docs/localization.md).

## Documentation

| File | Contents |
|---|---|
| [docs/architecture.md](docs/architecture.md) | Crates, layering, data model, commands, rendering, files |
| [docs/units-annotation.md](docs/units-annotation.md) | Drawing and display units, plot scale, annotation sizing and layout |
| [docs/workspaces.md](docs/workspaces.md) | Pro and Classic workspaces, panels, property toolbar, text editing, hatch selection |
| [docs/maintenance.md](docs/maintenance.md) | Flatten on import, `PURGE`, `AUDIT`, `RECOVER`, backups |
| [docs/ai-agent.md](docs/ai-agent.md) | Local AI agent: tools, validation, confirmation, loop limits |
| [docs/status.md](docs/status.md) | Implemented and planned functionality |
| [docs/hcw-kit-port.md](docs/hcw-kit-port.md) | Architectural tools ported from hcwCAD-KIT |
| [docs/control-protocol.md](docs/control-protocol.md) | JSON control channel |
| [docs/mcp.md](docs/mcp.md) | MCP server |
| [docs/plot-styles.md](docs/plot-styles.md) | Plot styles and plotting |
| [docs/localization.md](docs/localization.md) | Interface languages |
| [docs/contributors.md](docs/contributors.md) | Contributor credits in the About window |
| [docs/releasing.md](docs/releasing.md) | Versioning and release builds |
| [docs/parity-checklist.md](docs/parity-checklist.md) | Generated menu-name comparison (`cargo xtask parity`) |
| [CLAUDE.md](CLAUDE.md) | Rules for developers and coding agents |

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. Third-party notices are in
[NOTICE](NOTICE) and asset sources in [ATTRIBUTION.md](ATTRIBUTION.md).

Autodesk, AutoCAD and DWG are trademarks of Autodesk, Inc. This is an independent project, not
affiliated with or endorsed by Autodesk; the names are used only to describe file formats and
workflows it is compatible with.
