# AADT — AI-Aided Architecture Design and Takeoff

AADT is an **AI-aided architecture design and takeoff platform** built on a
native, command-line-first **2D CAD** core. The CAD engine is the substrate; the
product is what sits on top — deterministic quantity takeoff, documentation,
sheet production, and optional on-demand local AI — all running locally.

> **Positioning:** "Draft the way you always have. Let AADT handle the takeoff,
> the notes, and the paperwork you hate."

See [`docs/`](docs/) for engineering docs:
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — milestones, curated command band, product locks.
- [`docs/architecture/overview.md`](docs/architecture/overview.md) — layers and data flow.
- [`docs/ux/`](docs/ux/README.md) — AutoCAD-like command interaction research + workflows.
- [`docs/adr/`](docs/adr/) — Architecture Decision Records.

## Status

**M0–M8 cores are implemented** at the engine level: DXF → detect → takeoff
(CSV/JSON/XLSX/schedules), CAD commands with undo, AI infer/apply, docs/sheets,
LISP, PDF, and collab relay. See the roadmap for detail.

**Interactive shell (egui)** is the live product surface: command-first drafting,
live takeoff panel, pick/draw tools, Ortho, `OPEN`/`SAVE`/`TAKEOFF`, and in-app
`LISP`. True wgpu-at-100k, DWG, and native `.aadt` SQLite remain later work.

```sh
# Drive AADT with a command script
cargo run -p aadt-cli -- run fixtures/demo.aadt

# Desktop shell (needs --features app)
cargo run -p aadt-cli --features app -- app fixtures/sample-floorplan.dxf
```

## Workspace layout

| Crate | Responsibility |
|---|---|
| `aadt-geometry` | 2D geometry kernel |
| `aadt-model` | Document model, layers, undo/redo |
| `aadt-dxf` | DXF import/export |
| `aadt-detect` | Wall/room/opening detection, anomalies |
| `aadt-takeoff` | Quantities, cost codes, CSV/JSON/XLSX, schedules |
| `aadt-command` | Command interpreter (curated daily band) |
| `aadt-render` | Drawlist, camera; optional GPU/window backends |
| `aadt-ai` | On-demand local AI (Foundry Local default; Ollama optional) |
| `aadt-docs` | Sheets, title blocks, present |
| `aadt-lisp` | AutoLISP-flavoured engine + `aadt-*` bridge |
| `aadt-pdf` | PDF plot, markup, compare |
| `aadt-collab` | CRDT markup sync + TCP relay |
| `aadt-app` | `Session` + egui desktop shell |
| `aadt-store` | Native `.aadt` SQLite persistence |
| `aadt-ffi` | Thin C ABI over `Session` (future chrome) |
| `aadt-cli` | `aadt` binary |

## Build, test, run

```sh
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cargo run -p aadt-cli -- takeoff fixtures/sample-floorplan.dxf --csv out.csv
cargo run -p aadt-cli -- takeoff fixtures/sample-floorplan.dxf --json out.json
cargo run -p aadt-cli -- --version
```

Golden takeoff: `fixtures/expected-takeoff.csv` via `crates/aadt-cli/tests/golden.rs`.

### Local AI (Foundry Local)

Inbuilt AI defaults to **[Microsoft Foundry Local](https://learn.microsoft.com/azure/ai-foundry/foundry-local/)**
(`phi-3.5-mini` over the OpenAI-compatible API). Start Foundry Local, load the
model, then:

```sh
cargo run -p aadt-cli -- infer fixtures/sample-floorplan.dxf --save typed.dxf
# optional: AADT_FOUNDRY_ENDPOINT=http://127.0.0.1:55588
# optional: --backend ollama --model llama3
```

## Principles

1. The CAD core is a **substrate**, not the product — **2D only**.
2. **Command-first, always** — lean daily commands; **LISP for depth**.
3. Curated core commands, deep AADT capability (takeoff, docs) — no bloat.
4. **Local-first, on-demand AI** — nothing runs in the background.
5. **Deterministic core, AI as accelerant** — quantities auditable without AI.

## License

Apache-2.0.
