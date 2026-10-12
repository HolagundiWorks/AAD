# Quality baseline

What the build, tests and checks did on a known commit, before the quality programme changed
anything in the application. Reproduce it with `powershell -File tools/quality/baseline.ps1`
(it writes `target/quality/baseline-<date>.md` and one log per command).

## Environment

| | |
|---|---|
| Commit | `6dd0f04` (the baseline script itself was the only uncommitted path) |
| Date | 2026-10-12 |
| OS | Windows 11 Home Single Language 10.0.26300 |
| CPU / memory | 16 hardware threads, 15.8 GB |
| Toolchain | rustc 1.99.0 (2026-09-28), cargo 1.99.0, MSVC target `x86_64-pc-windows-msvc` |
| Workspace | 13 library crates, 3 applications, `xtask`; about 99 000 lines of Rust |

## Commands and results

| Step | Command | Exit | Seconds |
|---|---|---|---|
| check | `cargo check --workspace --all-targets` | 0 | 12.3 |
| test | `cargo test --workspace --all-targets --no-fail-fast` | 0 | 12.0 |
| fmt | `cargo fmt --all -- --check` | 0 | 1.7 |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 0.8 |
| release | `cargo build --workspace --release` | 0 | 149.7 |

The test run: **1 004 tests passed, 0 failed, 0 ignored.**

**These times are with a warm build cache** (the same checkout had just been built), so they are
not cold-build times. The first full release build of the workspace on this machine took several
minutes; the first build of the CI job will take longer still. The repository's own gate,
`cargo xtask ci`, adds asset attribution, layering and wasm-compile checks and passed on the same
commit.

## Known pre-existing failures

None. Every command exited 0 and no test failed, was skipped or was ignored. Nothing was
disabled or weakened to get that result.

## What this baseline does not establish

- **Platform:** only Windows x64 was built and tested. macOS, Linux and the web build are later
  phases; the web build is only checked to compile (`cargo xtask wasm`).
- **UI behaviour on real displays:** the UI tests drive the egui interface headlessly with
  `egui_kittest`; rendering on a GPU through `wgpu` was checked by taking screenshots of a running
  build during development, not by an automated test.
- **The installer:** at the time of this baseline the MSI had been built (17.8 MB) but not installed; it was installed, run and uninstalled on 2026-10-12 (see [RELEASE_AND_PACKAGING.md](RELEASE_AND_PACKAGING.md)).
- **The language-model runtime:** at the time of this baseline the `llama` feature had never been compiled (it has since: see [ai-agent.md](ai-agent.md))
  here (it needs LLVM's `libclang.dll`) and no model has been loaded.
- **DWG:** reading goes through a third-party library; its behaviour on real-world DWG files from
  other programs is only as tested as the fixtures we have (see BUG-001 in
  [BUG_BACKLOG.md](BUG_BACKLOG.md)).
- **Performance** is in [PERFORMANCE.md](PERFORMANCE.md).

## What the baseline run did not contain (before this programme)

No property-based tests, no fuzzing or mutation testing, no benchmarks, and no GitHub workflow that
ran the tests on a pull request (only the release workflow and a packaging lint existed). The
programme added them; see [TESTING.md](TESTING.md).
