# Contributing to AADT

Thanks for contributing. This repo is a **command-line-first 2D CAD** core with
an egui shell, Assist services that propose but never auto-commit, and a local
HTTP API whose only write path is a command line.

## Before you write code

1. Read the [internal engineering handbook](docs/engineering/README.md) — start
   with [STATUS.md](docs/engineering/STATUS.md) (Current vs Target vs Gap).
2. For interactive drafting changes, read **Vol 2 (Command Engine)** and
   **Vol 3 (Input & Events)**.
3. Product locks live in [ROADMAP.md](docs/ROADMAP.md) and [docs/adr/](docs/adr/).

## Hard rules

| Rule | Why |
|------|-----|
| Mutations go through `Session::feed` → `Command::run` → `Document::transact` | One undo/audit/API path |
| No `POST /v1/entities` (or any write that bypasses `/v1/command`) | [ADR-0015](docs/adr/0015-local-api-layer.md) |
| Assist proposes; apply/ACCEPT commits | [ADR-0013](docs/adr/0013-ai-service-layer.md) |
| Curated command band — prefer LISP / registered packs for depth | ROADMAP |
| New interactive tool = `CommandDef` + `Tool` + headless `Command` + workflows row | Two catalogs must stay in sync |
| GUI chrome uses `gui_theme` primitives | AAD Chrome mono system |

Full checklist: [Vol 15 — Coding Standards](docs/engineering/vol-15-coding-standards.md).

## Build / test

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Desktop shell (needs a window):

```sh
cargo run -p aadt-cli --features app -- app fixtures/sample-floorplan.dxf
```

Prefer **headless** `Session::feed` tests for behavior. The egui shell is not
fully headless-verifiable — see [Vol 14](docs/engineering/vol-14-testing.md).

## Docs hygiene

- Closing a handbook gap? Update [STATUS.md](docs/engineering/STATUS.md) in the same PR.
- Durable locks? Add or amend an ADR.
- Do not claim features the code does not have (free-drag Select lasso is not
  shipped — Vol 6 / ROADMAP M9.2; use WP/CP/FENCE for polygon regions).

## License

Contributions are under Apache-2.0 (see repository license).
