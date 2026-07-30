# AAD — Accelerated Architectural Drafting

<p align="left">
  <img src="crates/aadt-app/assets/branding/aadt_mark.svg" alt="" height="28" />
  <strong>AAD</strong>
</p>

AAD is a lean, **keyboard-first 2D CAD drafting tool** for architecture, built
on a native command-line-first core. Drafting speed comes first: the command
line is the primary interaction engine, the mouse assists, and the canvas stays
the focus. Everything runs locally.

> **Positioning:** "Draft the way you always have — fast, predictable, and out
> of your way."

See [`docs/`](docs/) for engineering docs:
- [`docs/vision/ai-native-platform.md`](docs/vision/ai-native-platform.md) — the AI-native platform direction (multi-store architecture, semantic objects, AI services).
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — milestones, curated command band, product locks.
- [`docs/architecture/overview.md`](docs/architecture/overview.md) — layers and data flow.
- [`docs/audit/2026-07-industry-standards-audit.md`](docs/audit/2026-07-industry-standards-audit.md) — industry grade + gap register.
- [`docs/ux/`](docs/ux/README.md) — AutoCAD/BricsCAD-like command interaction research + workflows.
- [`docs/adr/`](docs/adr/) — Architecture Decision Records.
- [`docs/CHANGELOG.md`](docs/CHANGELOG.md) — recent changes.

## Status

The CAD core is implemented: DXF/DWG import + native `.aadt` SQLite (with
history checkpoints), CAD commands with atomic undo/redo, wall/room/opening
**detection**, AIA/NCS standards layers, block/hatch libraries, PDF plotting +
markup, sheet/title-block production, and an AutoLISP-flavoured engine. See the
[roadmap](docs/ROADMAP.md) and [industry audit](docs/audit/2026-07-industry-standards-audit.md).

**Interactive shell (egui)** is the live product surface: command-first
drafting, History manager, pick/draw/modify tools, Ortho/Osnap/Polar, dynamic
input, docked Layers/Properties, and `OPEN`/`SAVE` for `.dxf` / `.dwg` /
`.aadt`. Open-source DWG import via `acadrust` works now (`OPEN file.dwg`); true
wgpu-at-100k and ODA-grade DWG fidelity remain later work.

```sh
# Drive AAD with a command script
cargo run -p aadt-cli -- run fixtures/demo.aadt

# Desktop shell (needs --features app)
cargo run -p aadt-cli --features app -- app fixtures/sample-floorplan.dxf
```

## Workspace layout

| Crate | Responsibility |
|---|---|
| `aadt-geometry` | 2D geometry kernel |
| `aadt-model` | Document model, layers, undo/redo |
| `aadt-dxf` | DXF/DWG import/export |
| `aadt-store` | Native `.aadt` SQLite (history checkpoints, embeddings vector db) |
| `aadt-vdb` | **VastuDB** — custom on-disk vector database `.vdb`; the drawing's geometry as native vector records + spatial index |
| `aadt-embed` | Local, offline ML classifier/embedder (element → AIA layer + category) |
| `aadt-vecdb` | Custom embeddings vector db + confidence-gated metadata mapping (`MAPMETA`) |
| `aadt-detect` | Wall/room/opening detection, anomalies |
| `aadt-command` | Command interpreter (daily band + firm suites) |
| `aadt-standards` | AIA/NCS layers, layer sets, WCAG helpers |
| `aadt-library` | Block/hatch library (DXF/SVG) |
| `aadt-render` | Drawlist, camera; optional GPU/window backends |
| `aadt-docs` | Sheets, title blocks, present |
| `aadt-lisp` | AutoLISP-flavoured engine + `aadt-*` bridge |
| `aadt-pdf` | PDF plot, markup, compare |
| `aadt-app` | `Session` + egui desktop shell |
| `aadt-ffi` | Thin C ABI over `Session` (future chrome) |
| `aadt-cli` | `aadt` binary |

## Build, test, run

```sh
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cargo run -p aadt-cli -- run fixtures/demo.aadt
cargo run -p aadt-cli -- --version
```

## Principles

1. **Keyboard-first, mouse-assisted** — the command line is the interaction engine.
2. **Command-first, always** — lean daily commands; **LISP for depth**.
3. **2D only** — plan/elevation/section drafting; no 3D, solids, or BIM.
4. Curated core commands, no bloat — depth goes into LISP, not command-count.
5. **Deterministic core** — geometry, numbering, and detection are reproducible.

## License

Apache-2.0.
