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
- [`docs/engineering/README.md`](docs/engineering/README.md) — **internal CAD handbook** (15 volumes: command engine, input, undo, plan vs existing).
- [`docs/vision/ai-native-platform.md`](docs/vision/ai-native-platform.md) — the AI-native platform direction (multi-store architecture, semantic objects, AI services).
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — milestones, curated command band, product locks.
- [`docs/architecture/overview.md`](docs/architecture/overview.md) — layers and data flow.
- [`docs/audit/2026-07-industry-standards-audit.md`](docs/audit/2026-07-industry-standards-audit.md) — industry grade + gap register.
- [`docs/ux/`](docs/ux/README.md) — AutoCAD/BricsCAD-like command interaction research + workflows.
- [`docs/adr/`](docs/adr/) — Architecture Decision Records.
- [`docs/CHANGELOG.md`](docs/CHANGELOG.md) — recent changes.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — contributor contract (points at the handbook).

## Status

The CAD core is implemented: DXF/DWG import + native `.aadt` SQLite (with
history checkpoints), CAD commands with atomic undo/redo, wall/room/opening
**detection**, AIA/NCS standards layers, block/hatch/detail libraries, PDF
plotting + markup, sheet/title-block production, and an AutoLISP-flavoured
engine. See the [roadmap](docs/ROADMAP.md) and
[industry audit](docs/audit/2026-07-industry-standards-audit.md).

**Interactive shell (shipping path):** C++ **WinUI 3** + Direct2D host under
[`native/aad-winui/`](native/aad-winui/) driving the Rust engine via
`aadt_ffi.dll` ([ADR-0017](docs/adr/0017-winui-shell-rust-engine.md)).

**egui fallback:** `cargo run -p aadt-cli --features app -- app` — maintenance
only. Command-first drafting, History, docks, Ortho/Osnap/Polar, and
`OPEN`/`SAVE` for `.dxf` / `.dwg` / `.aadt` / `.vdb` share the same `Session`.
Open-source DWG import via `acadrust` works now (`OPEN file.dwg`); true
wgpu-at-100k and ODA-grade DWG
fidelity remain later work.

**AI-native layer (local, offline, gated).** Typed objects and relationships
(`OBJECTS` / `RELATIONS`), semantic search and cited document retrieval
(`FIND` / `ASKDOCS`), assist services that propose but never commit
(`SUGGEST` / `CHECK` / `ASK`, plus `COMPARE` revision narration and
`RECOMMEND DETAIL` over the bundled detail sheets), and object-based quantity
takeoff (`TAKEOFF` / `BOQ`). See
[the AI-native platform vision](docs/vision/ai-native-platform.md).

**Local API (`aadt serve`).** A loopback-only, token-gated HTTP/JSON server over
a live session, off unless you start it. Reads project the drawing; the one
write path is `POST /v1/command`. Optional `--identity` / `--role` for M14
permissions; `PERMS`, `CONNECTOR`, and `SITE` ride the same command door. See
the [endpoint reference](docs/api/README.md) and [ADR-0016](docs/adr/0016-m14-enterprise.md).

```sh
# Drive AAD with a command script
cargo run -p aadt-cli -- run fixtures/demo.aadt

# Desktop shell (egui fallback — needs --features app)
cargo run -p aadt-cli --features app -- app fixtures/sample-floorplan.dxf

# WinUI / Direct2D host — see native/aad-winui/README.md
#   cargo build -p aadt-ffi --release
#   cmake -B native/aad-winui/build ... && cmake --build ...

# Local API on 127.0.0.1:7421 (prints a one-run token)
cargo run -p aadt-cli -- serve fixtures/sample-floorplan.dxf
```

## Workspace layout

| Crate | Responsibility |
|---|---|
| `aadt-geometry` | 2D geometry kernel |
| `aadt-model` | Document model, layers, undo/redo |
| `aadt-dxf` | DXF/DWG import/export |
| `aadt-store` | Native `.aadt` SQLite (history checkpoints, embeddings vector db) |
| `aadt-vdb` | **ShilpiDB** — custom on-disk vector database `.vdb`; the drawing's geometry as native vector records + spatial index |
| `aadt-object` | Object DB — typed intelligent engineering objects (Wall/Room/Door/Column…) + append-only audit log |
| `aadt-graph` | Graph DB — typed relationships (BelongsTo/Contains/Bounds/ConnectedTo) + detection→object promotion |
| `aadt-embed` | Local, offline ML classifier/embedder (element → AIA layer + category) + text/aggregate embeddings |
| `aadt-vecdb` | Custom embeddings vector db, confidence-gated metadata mapping (`MAPMETA`), semantic search (`FIND`) + BM25 corpus retrieval (`ASKDOCS`) |
| `aadt-assist` | AI service layer — gated suggestions: dimension prediction, code-check rule packs, revision-comparison narration (`COMPARE`), standard-detail recommendation (`RECOMMEND DETAIL`), NL command routing (`SUGGEST`/`CHECK`/`ASK`) |
| `aadt-takeoff` | Quantity takeoff + BOQ over the Object DB, CSI MasterFormat cost codes (`TAKEOFF`/`BOQ`) |
| `aadt-detect` | Wall/room/opening detection, anomalies |
| `aadt-command` | Command interpreter (daily band + firm suites) |
| `aadt-standards` | AIA/NCS layers, layer sets, WCAG helpers |
| `aadt-library` | Block/hatch library (DXF/SVG) |
| `aadt-render` | Drawlist, camera; optional GPU/window backends |
| `aadt-docs` | Sheets, title blocks, present |
| `aadt-lisp` | AutoLISP-flavoured engine + `aadt-*` bridge |
| `aadt-pdf` | PDF plot, markup, compare |
| `aadt-app` | `Session` + egui fallback shell |
| `aadt-ffi` | C ABI over `Session` for WinUI / Direct2D (`aadt_ffi.dll`) |
| `aadt-api` | Local HTTP/JSON API over `Session` — loopback-only, token-gated (`aadt serve`) |
| `aadt-cli` | `aadt` binary + `register` library for host commands |

## Bundled libraries

Shipped alongside the binary (see [`libraries/`](libraries/README.md)) and
resolved at runtime by `aadt-library`:

| Path | Contents | Used by |
|---|---|---|
| `libraries/blocks/` | Block definitions (DXF / SVG) | `BLOCKLIB`, `INSERT` |
| `libraries/hatches/` | `.pat` hatch pattern families | `HATCHLIB`, `HATCH` |
| `libraries/details/` | Standard-detail Markdown sheets | `RECOMMEND DETAIL` |
| `libraries/fonts/` | Drawing text fonts | Text rendering |

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
