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
quantities, and exports an estimator-friendly CSV. Everything else in the
roadmap is scaffolded (documented crate skeletons) and built milestone by
milestone.

## Workspace layout

A Cargo workspace mirroring the layered architecture. Crates marked *(M0)* carry
real logic today; the rest are documented skeletons awaiting their milestone.

| Crate | Responsibility |
|---|---|
| `aadt-geometry` *(M0)* | 2D geometry kernel — points, vectors, segments, polylines, tolerance, bbox, intersection math |
| `aadt-model` *(M0)* | Document model — entity store, layers, attributes (undo/redo in M1) |
| `aadt-dxf` *(M0)* | DXF import (LINE, LWPOLYLINE); full entity set, export, and DWG later |
| `aadt-detect` *(M0)* | Element detection — deterministic wall pairing, wall graph, room extraction |
| `aadt-takeoff` *(M0)* | Takeoff & quantity engine — quantities + CSV export |
| `aadt-cli` *(M0)* | `aadt` binary — headless driver for the pipeline, batch, and tests |
| `aadt-command` | Command interpreter (registry, aliases, history) — M1 |
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
```

The end-to-end result is asserted against `fixtures/expected-takeoff.csv` by the
golden test in `crates/aadt-cli/tests/golden.rs`.

## Principles

1. The CAD core is a **substrate**, not the product.
2. **Command-first, always.**
3. Curated core commands, deep AADT capability.
4. **Local-first, on-demand AI** — nothing runs in the background.
5. **Deterministic core, AI as accelerant** — geometry and quantities are
   reproducible and auditable independent of AI availability.

## License

Apache-2.0.
