# AADT — AI-Aided Architecture Design and Takeoff

AADT is an **AI-aided architecture design and takeoff platform** built on a
native, command-line-first 2D CAD core. The CAD engine is the substrate; the
product is what sits on top of it — AI-assisted element identification,
deterministic quantity takeoff, documentation generation, and sheet production,
all running locally and on demand.

> **Positioning:** "Draft the way you always have. Let AADT handle the takeoff,
> the notes, and the paperwork you hate."

See [`docs/`](docs/) for the full engineering documentation:
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — milestone plan (M0–M8).
- [`docs/architecture/overview.md`](docs/architecture/overview.md) — layers,
  crate map, and data flow.
- [`docs/adr/`](docs/adr/) — Architecture Decision Records for the early forks.

## Status

**M0 — Deterministic Takeoff Vertical Slice** is implemented: a headless
pipeline that imports a DXF, detects walls and rooms, computes reproducible
quantities, and exports an estimator-friendly CSV.

**M1 — CAD Core Substrate (headless foundations)** is implemented: atomic
undo/redo transactions, a command interpreter with registry/aliases/history and
script execution (`aadt run <script>`), and DXF export + round-trip. The `wgpu`
render canvas and interactive `aadt-app` shell remain.

**M2 — Element Detection & M3 — Takeoff (deterministic core)** are implemented:
door/window/furniture classification from block inserts, circle/rectangle
structural detection, sequential numbering and tag points; wall area/volume from
height, opening subtraction (net wall area), and CSI MasterFormat cost-code
mapping in the CSV. Room-from-wall-graph derivation, live-linked tables, and
`.xlsx` export remain. Everything further out (AI orchestration, docs/sheets,
LISP, PDF) is scaffolded and built milestone by milestone.

```sh
# Drive AADT with a command script (OPEN, TAKEOFF, LINE, UNDO, LIST, SAVE)
cargo run -p aadt-cli -- run fixtures/demo.aadt
```

## Workspace layout

A Cargo workspace mirroring the layered architecture. Crates marked *(M0)* carry
real logic today; the rest are documented skeletons awaiting their milestone.

| Crate | Responsibility |
|---|---|
| `aadt-geometry` *(M0)* | 2D geometry kernel — points, vectors, segments, polylines, tolerance, bbox, intersection math |
| `aadt-model` *(M0/M1)* | Document model — entity store, layers, attributes, undo/redo transactions |
| `aadt-dxf` *(M0/M1)* | DXF import (LINE, LWPOLYLINE, legacy POLYLINE) + export/round-trip; more entities & DWG later |
| `aadt-detect` *(M0)* | Element detection — deterministic wall pairing, wall graph, room extraction |
| `aadt-takeoff` *(M0)* | Takeoff & quantity engine — quantities + CSV export |
| `aadt-cli` *(M0/M1)* | `aadt` binary — `takeoff` pipeline and `run` script interpreter |
| `aadt-command` *(M1)* | Command interpreter — registry, aliases, history, script execution |
| `aadt-render` | GPU 2D canvas (wgpu) — M1 |
| `aadt-ai` | AI orchestration (on-demand Ollama) — M4 |
| `aadt-docs` | Documentation & sheet production — M5 |
| `aadt-lisp` | Embedded LISP engine — M6 |
| `aadt-pdf` | PDF markup & comparison — M7 |
| `aadt-app` | Desktop application shell (egui/Slint) — from M1 |

## Build, test, run

```sh
# Build everything
cargo build --workspace

# Lint + format check + tests (what CI runs)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Run the M0 takeoff on the sample floor plan
cargo run -p aadt-cli -- takeoff fixtures/sample-floorplan.dxf --csv out.csv

# Machine-readable output for downstream engines (versioned schema)
cargo run -p aadt-cli -- takeoff fixtures/sample-floorplan.dxf --json out.json
cargo run -p aadt-cli -- --version        # engine + takeoff-schema version
```

The end-to-end result is asserted against `fixtures/expected-takeoff.csv` by the
golden test in `crates/aadt-cli/tests/golden.rs`. `--json` emits the same
quantities as a versioned JSON object (`schema_version`, `engine_version`,
`units`, `totals`, and typed `walls`/`openings`/`rooms`/`structural`) for
app-to-app integration — see the AiADT integration spec in the esti repo.

## Principles

1. The CAD core is a **substrate**, not the product.
2. **Command-first, always.**
3. Curated core commands, deep AADT capability.
4. **Local-first, on-demand AI** — nothing runs in the background.
5. **Deterministic core, AI as accelerant** — geometry and quantities are
   reproducible and auditable independent of AI availability.

## License

Apache-2.0.
