# Volume 14 — Testing & Validation

Internal engineering handbook. Status: [STATUS.md](./STATUS.md).

## Architectural goals

1. Prove the deterministic spine **headlessly** via `Session::feed` and pure modules.
2. Treat the egui shell as display-bound — logic under test must not require a window.
3. Use golden scripts for regression of command transcripts / document outcomes.
4. Manual workflows checklists cover Space/dyn/transparent interaction.

## Current implementation

### Patterns

| Pattern | Where |
|---------|-------|
| Inline `#[cfg(test)] mod tests` | Most crates’ `lib.rs` / modules |
| Integration goldens | `crates/aadt-cli/tests/golden.rs` |
| GUI logic tests without window | `gui.rs` modules: `geom_tests`, `zoom_tests`, `box_select_tests`, `perf_tests` |
| Engine FSM tests | `command_engine.rs` |
| Osnap / selection units | `osnap.rs`, `selection.rs` |

Almost no per-crate `tests/` directories beyond CLI goldens — unit tests live beside code.

### What is intentionally hard to automate

- Full egui event loop / pixel-perfect chrome.
- Transparent mid-command feel, palette hold timing.
- Use [command-workflows.md](../ux/command-workflows.md) as the QA script.

### Workspace commands (README)

```
cargo fmt
cargo clippy -- -D warnings
cargo test --workspace
```

## Target / planned

- Broader goldens for Assist/takeoff.
- Optional drawlist snapshot hashes.
- Perf gates as separate CI jobs (Vol 13) when stable.
- Keep GUI smoke manual.

## Gap analysis

| Topic | Gap |
|-------|-----|
| Full GUI CI | Not headless-verified by contract |
| Perf gate CI | Open |
| Exhaustive command goldens | Partial |

## Module layout (test ownership)

```
aadt-geometry / aadt-model / aadt-command / aadt-store / aadt-vdb / …
    └── src/** #[cfg(test)]
aadt-app/src/{lib,selection,osnap,command_engine,gui,prefs}.rs tests
aadt-cli/tests/golden.rs
aadt-api/src tests
```

## Error handling in tests

Assert on `Result` / `Feedback` / document state — not panic strings.
Prefer stable message prefixes if matching text.

## Performance

Don’t put flaky timing asserts in default `cargo test`. Use ignored benches or
dedicated jobs.

## Tests (meta-checklist for new features)

1. Headless `Command::run` or `Session::feed` case.
2. If interactive: `CommandDef` finish-rule unit test + workflows doc row.
3. If geometry predicate: `aadt-geometry` / `selection` / `osnap` unit test.
4. Update STATUS.md if closing a documented gap.

## Extension points

1. Define a “command golden” format: script lines → expected transcript + doc hash.
2. Policy for regenerating goldens when DXF importer improves.
3. Inventory binary names in CI matrix as the suite grows.

## Related

[Vol 2](./vol-02-command-engine.md) · [Vol 15](./vol-15-coding-standards.md) ·
[ux/command-workflows.md](../ux/command-workflows.md)
