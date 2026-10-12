# AAD - Accelerated Architecture Drafting

AAD is a 2D computer-aided drafting application written in Rust, aimed at architectural and civil
drafting. It reads and writes DXF and DWG and can be driven from scripts and agents. **Windows is
the only supported platform for now**; macOS, Linux and the web build come in later phases (the
code stays portable, but only Windows is built, tested and packaged). This repository is a fork of
[storytold/cadcraft](https://github.com/storytold/cadcraft) with the 3D, parametric-constraint and
mechanical features removed.

What is built and what is planned is listed in [docs/status.md](docs/status.md). Nothing in the
documentation describes a feature as available unless it exists and has tests.

## Status

Version 0.6.0. Windows x64 is the only platform that is built, tested and packaged. Working today:
2D drafting and modifying, DXF and DWG, layouts and plotting, the architecture tools, a command line
and automation interfaces. Not available: 3D, parametric constraints, UCS, SHX fonts, and the local
language model in the apps (the runtime crate exists, see [docs/ai-agent.md](docs/ai-agent.md)).
The full list of what exists and what is planned is in [docs/status.md](docs/status.md); known
problems are in [docs/KNOWN_ISSUES.md](docs/KNOWN_ISSUES.md).

## Prerequisites

Rust stable with the MSVC target (1.95 or later for the desktop and UI crates; the other crates
declare 1.90) and the Visual Studio Build Tools. A GPU with a wgpu-compatible driver runs the
application. Nothing else is needed to build and test. Installing the toolchain, packaging and
troubleshooting: [docs/BUILD_AND_DEVELOPMENT.md](docs/BUILD_AND_DEVELOPMENT.md).

## Build and run

```sh
cargo run --release -p aad -- --sample    # desktop app with a sample drawing
cargo run --release -p aad -- file.dxf    # open a file
```

Type commands at the command line, for example:

```text
line 0,0 @10,0 @0,5 c
circle 5,2 1
offset 0.25
zoom e
```

The web build (`apps/aad-web`) is not part of the supported product yet; it is only checked to compile.

## Tests

```sh
cargo xtask ci        # fmt, clippy -D warnings, tests, asset list, layering, wasm check
```

`cargo xtask --help` lists the other tasks (`layers`, `assets`, `wasm`, `parity`, `version`, `logo`, `stats`).

## Command-line tool, MCP and control channel

```sh
aad-cli info drawing.dxf
aad-cli convert drawing.dxf drawing.svg
aad-cli run --sample --script 'CIRCLE 22,3 1\n' --save out.dxf --export out.png
aad-cli mcp                               # MCP server on stdio, headless
aad-cli mcp --connect 127.0.0.1:7979      # ...or bridged to a running app (start it with --control 7979)
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

An offline assistant can call a fixed set of validated tools (summary, flatten, audit, purge, recover, units, plot configuration). It runs a local Qwen3-1.7B model through llama.cpp and is **not part of the default build**: build with `--features assistant` (`cargo build --release -p aad --features assistant`; needs CMake, a C++ toolchain and LLVM) and put the model file in `%APPDATA%AADmodels`. Type a request in the command bar (a leading `/`, a `/reference` such as `/layers`, or a plain sentence; see [docs/COMMAND_BAR.md](docs/COMMAND_BAR.md)), or use `aad-cli ask "request"`. A tool that changes the drawing asks first. The model file is not bundled with the installer yet. See
[docs/ai-agent.md](docs/ai-agent.md), including what was observed with the real model.

## Configuration

- Fonts: AAD ships none and uses the TrueType/OpenType fonts installed on the system. SHX fonts
  are not read; text in an SHX style is drawn with the built-in stroke font. `FONTALT` and
  `FONTFALLBACK` (or `AAD_FONTALT`, `AAD_FONTFALLBACK`) name a replacement font and the
  fonts tried first for characters a font lacks. `.` clears either.
- Logs: the desktop app writes to standard error and to `logs/aad.log` in the settings
  directory (`~/.config/aad/` on Linux and the BSDs, `~/Library/Application Support/AAD/`
  on macOS, `%APPDATA%\AAD\` on Windows). `AAD_CONFIG_DIR` overrides the directory and
  `RUST_LOG` sets the levels (default `info` for AAD's crates, `warn` for the rest). Each launch
  rotates the previous log; the file stops growing at 16 MiB.
- Theme: one light/dark toggle (View menu, status bar). Dark has a black drawing area, light a
  white one. There is no other colour customisation.
- Interface language: English only.

## Documentation

| File | Contents |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crates, layering, data model, commands, undo, threading, rendering, files |
| [docs/PROJECT_STRUCTURE.md](docs/PROJECT_STRUCTURE.md) | Directory and module map, entry points, generated folders |
| [docs/BUILD_AND_DEVELOPMENT.md](docs/BUILD_AND_DEVELOPMENT.md) | Toolchain, build, run, test, packaging, troubleshooting |
| [docs/CAD_ENGINE.md](docs/CAD_ENGINE.md) | Entities, geometry, units, snapping, selection, hatch, file interoperability, limits |
| [docs/UI_UX.md](docs/UI_UX.md) | Screens, menus, mouse and keyboard, dialogs, window sizes, UI test coverage |
| [docs/TESTING.md](docs/TESTING.md) | Test layers, property and mutation tests, regression procedure, CI |
| [docs/PERFORMANCE.md](docs/PERFORMANCE.md) | Benchmarks, measurements, bottlenecks, optimisation history |
| [docs/RELEASE_AND_PACKAGING.md](docs/RELEASE_AND_PACKAGING.md) | Release build, installer, validation, artifact retention |
| [docs/KNOWN_ISSUES.md](docs/KNOWN_ISSUES.md) | Confirmed issues, hypotheses, limits |
| [docs/ACTIVE_DRAWING_SPACE.md](docs/ACTIVE_DRAWING_SPACE.md) | Drawing spaces, layer protection, context bar, Classic and Pro workspaces: design, commands, acceptance status |
| [docs/units-annotation.md](docs/units-annotation.md) | Drawing and display units, plot scale, annotation sizing and layout |
| [docs/workspaces.md](docs/workspaces.md) | Pro and Classic workspaces, panels, text editing, hatch selection |
| [docs/maintenance.md](docs/maintenance.md) | Flatten on import, `PURGE`, `AUDIT`, `RECOVER`, backups, DWG worker |
| [docs/ai-agent.md](docs/ai-agent.md) | Local AI agent: tools, validation, confirmation, loop limits |
| [docs/status.md](docs/status.md) | Implemented and planned functionality |
| [docs/hcw-kit-port.md](docs/hcw-kit-port.md) | Architectural tools ported from hcwCAD-KIT |
| [docs/projects.md](docs/projects.md), [docs/sheets.md](docs/sheets.md), [docs/widgets.md](docs/widgets.md), [docs/input-forms.md](docs/input-forms.md) | Floor projects and Xrefs, layouts and plotting, calculator and Pomodoro, input forms |
| [docs/control-protocol.md](docs/control-protocol.md) | JSON control channel |
| [docs/mcp.md](docs/mcp.md) | MCP server |
| [docs/plot-styles.md](docs/plot-styles.md) | Plot styles and plotting |
| [docs/contributors.md](docs/contributors.md) | Contributor credits in the About window |
| [docs/releasing.md](docs/releasing.md) | Versioning, release workflow, signing |
| [docs/QUALITY_BASELINE.md](docs/QUALITY_BASELINE.md), [docs/BUG_BACKLOG.md](docs/BUG_BACKLOG.md), [docs/UI_BASELINE.md](docs/UI_BASELINE.md), [docs/UI_BUG_BACKLOG.md](docs/UI_BUG_BACKLOG.md) | Quality programme records |
| [docs/UI_TESTING_STRATEGY.md](docs/UI_TESTING_STRATEGY.md), [docs/UI_INVENTORY.md](docs/UI_INVENTORY.md), [docs/UI_TEST_MATRIX.md](docs/UI_TEST_MATRIX.md) | UI test tooling, control inventory, coverage |
| [docs/parity-checklist.md](docs/parity-checklist.md) | Generated menu-name comparison (`cargo xtask parity`) |
| [CLAUDE.md](CLAUDE.md) | Rules for developers and coding agents |

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. Third-party notices are in
[NOTICE](NOTICE) and asset sources in [ATTRIBUTION.md](ATTRIBUTION.md).

Autodesk, AutoCAD and DWG are trademarks of Autodesk, Inc. This is an independent project, not
affiliated with or endorsed by Autodesk; the names are used only to describe file formats and
workflows it is compatible with.
