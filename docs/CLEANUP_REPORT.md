# Cleanup report

Documentation audit and repository cleanup of 2026-10-12. The inventory and findings are in
[REPOSITORY_AUDIT.md](REPOSITORY_AUDIT.md). Everything below records what actually happened.

## Repository summary

| | Before | After |
|---|---|---|
| Tracked files | 456 | 464 (8 new documents; two were renamed and one rewritten in place) |
| Source size (excluding `target/`, `.git/`) | about 5.9 MB tracked content | unchanged apart from documents |
| `.git` | 38.8 MB on disk (25 MiB packed) | unchanged; no history, tag or branch was touched |
| `target/` | 103 697 MB, 101 501 files (`debug` 98 284 MB, `release` 3 264 MB, `x86_64-pc-windows-msvc` 1 263 MB, `wasm32-unknown-unknown` 776 MB, `windows-package` 116 MB) | 25 210 MB, 22 316 files, after a **complete rebuild** (`cargo xtask ci` and `cargo build --workspace --release`); `debug` 23 277 MB, `release` 1 597 MB |
| `dist/` | 38.5 MB (MSI and portable zip) | 38.5 MB, untouched |

Inspected: every top-level directory, all 456 tracked files by category, every `Cargo.toml`
dependency, module declarations of all crates, the documentation links, `.gitignore`, `.cargo`,
workflows (by listing), packaging scripts (by reading `package.ps1` and grepping references).

## Files removed

| Path | Category | Reason and evidence |
|---|---|---|
| `target/debug` (98 284 MB) | generated | Cargo output, ignored, rebuilt by `cargo build`/`test` (it was rebuilt: 23 GB) |
| `target/wasm32-unknown-unknown` (776 MB) | generated | output of `cargo xtask wasm`; rebuilt by the gate |
| `target/x86_64-pc-windows-msvc` (1 263 MB), `target/windows-package` (116 MB) | generated | installer build and staging written by `packaging/windows/package.ps1` (read: lines 50 to 107); the deliverables in `dist/release` were kept |
| `target/release` (3 264 MB) | generated | rebuilt (`cargo build --workspace --release`, 3.7 minutes) |
| `target/quality`, `target/criterion` (0.4 MB) | generated | benchmark and baseline raw output: **lost unintentionally, see below** |

No tracked file was deleted. No file outside the repository was touched.

### An unintended over-deletion

I ran `cargo clean --profile dev` and then `cargo clean --target wasm32-unknown-unknown` to remove
only the debug and wasm output. The two commands together removed the **entire** `target/` folder
(96.2 GiB and 5.3 GiB reported), including the release build, the installer staging folder and
`target/quality` (the raw benchmark logs `bench-run1/2/3*.txt` and the baseline timing logs from
`tools/quality/baseline.ps1`), which I had meant to keep. I did not check which command did it before running them. Consequences:

- Everything reproducible was rebuilt and verified (below). The installer files in `dist/` were
  never inside `target/` and are intact.
- The raw benchmark text files are gone. The measured numbers are recorded in
  [PERFORMANCE.md](PERFORMANCE.md), [UI_BASELINE.md](UI_BASELINE.md) and
  [QUALITY_BASELINE.md](QUALITY_BASELINE.md); the benchmark commands can regenerate the raw files.
  The first quality baseline log cannot be regenerated for the old commit.
- [BUILD_AND_DEVELOPMENT.md](BUILD_AND_DEVELOPMENT.md) now warns about this.

## Code changes

| Change | Evidence |
|---|---|
| Removed unused dependency `cadcraft-render` from `apps/cadcraft-cli/Cargo.toml` | no `cadcraft_render` use in that crate; `cargo check --workspace --all-targets` passes without it |
| Removed unused dependency `serde` from `crates/agent/Cargo.toml` | no use in the crate (`serde_json` is separate); same check |
| Removed unused dependency `cadcraft-io` from `crates/mcp/Cargo.toml` | no use in the crate or its tests; same check |
| `Cargo.lock` updated accordingly (removal of those edges only) | `--locked` packaging remains valid for the new lockfile |
| `xtask` messages and a comment: `plan/architecture.md §3` replaced by `docs/ARCHITECTURE.md` | the old path is in a git-ignored folder that does not exist; `cargo xtask layers` runs |
| `crates/llm/Cargo.toml` comment: `--features assistant` replaced by `-p cadcraft-llm --features llama` | the `assistant` feature does not exist |
| `crates/io/tests/mutation_robustness.rs` comment points to `docs/TESTING.md` | renamed document |

No behaviour changed. No feature flags, modules or assets were removed: the module check found no
orphan files, the only feature (`llama`) is used, and every asset file has an `ATTRIBUTION.md` row
(`cargo xtask assets` passes). Dependencies were not upgraded.

## Documentation

Created: `PROJECT_STRUCTURE.md`, `BUILD_AND_DEVELOPMENT.md`, `CAD_ENGINE.md`, `UI_UX.md`,
`RELEASE_AND_PACKAGING.md`, `KNOWN_ISSUES.md`, `REPOSITORY_AUDIT.md`, this report.
Renamed: `architecture.md` to `ARCHITECTURE.md`, `TESTING_STRATEGY.md` to `TESTING.md`,
`PERFORMANCE_BASELINE.md` to `PERFORMANCE.md` (the names the cleanup specification asked for;
case-only renames collide on Windows, so the old file was moved, not copied).
Updated: `README.md` (status, prerequisites, documentation index, AI note), `ARCHITECTURE.md`
(crate count, `llm`, 339 commands, undo, threading, dependencies, DWG worker),
`UI_INVENTORY.md` (385 is 339 + 46), `BUG_BACKLOG.md` (a link to a document that never existed).
Links between all documents were checked with a script: none are broken.

Areas that remain undocumented or unaudited: the older feature documents were not re-verified
against the code beyond the facts quoted above; macOS, Linux and web packaging; the release
workflow's signing; dependency licences and advisories. `docs/parity-checklist.md` is generated and
was not edited.

## Verification

| Check | Result |
|---|---|
| `cargo xtask ci` (fmt, clippy -D warnings, tests, asset attribution, layering, wasm) | passed after the clean rebuild (13.5 minutes) and again on the final tree: 1 045 tests passed, 0 failed (the baseline was 1 004 passed, 0 failed; the difference is the property, workflow, menu, store and spatial tests added since) |
| `cargo fmt --all -- --check` | passed |
| `cargo build --workspace --release` | passed, 3.7 minutes from an empty `target/` |
| Application launch | `cadcraft.exe --control 47125 --sample` started on the GPU canvas; a line was added through the control channel, a screenshot was written and read (the sample drawing with panels rendered), and `app.quit` closed it |
| CLI | `cadcraft-cli info examples/apartment.dxf` and `run --sample --script ... --save --export` wrote a DXF and a PNG; the DXF was read back |
| UI tests | the 17 test files of `cadcraft-ui-egui` pass inside the gate |
| Packaging | **not re-run**; the existing `dist/release` files were deliberately kept and are not re-validated |
| CI workflows | not run on GitHub (unverified, as before) |

Compared with the baseline there are no new failures. No test was disabled or changed.

## Remaining work

- Decide whether to keep `packaging/macos/dmg/generate.py` (Python) given the "Rust only" rule.
- Decide on a smaller debug profile (for example less debug info) if the 25 GB rebuild size matters.
- Rebuild and re-validate the installer (`package.ps1`) before the next release, and install it
  on a clean machine; the audit did not do this.
- Run the benchmarks again if the raw logs are wanted.
- Re-verify the feature documents (`hcw-kit-port.md`, `projects.md`, `sheets.md`, ...) against the code.
- Optional: `git gc` (587 loose objects); not done because it rewrites repository internals.
