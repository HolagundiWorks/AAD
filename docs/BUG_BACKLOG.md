# Bug backlog

Defects found by the quality programme, with evidence. A defect is **confirmed** only when it was
reproduced; everything else is in the *suspected* table of [QUALITY_AUDIT.md](QUALITY_AUDIT.md).
Severity: P0 data corruption, severe crash or unsafe behaviour; P1 core drafting broken; P2
significant lag or important functionality unreliable; P3 minor; P4 optimisation or debt.

## Confirmed

### BUG-001 (P0, fixed): a damaged DWG aborts the whole program

- **What happened:** reading a corrupted DWG ended the process with *memory allocation of
  3377699721332651 bytes failed* (exit status `0xc0000409`). Every open drawing was lost. An
  allocation failure aborts the process and cannot be caught, not even by `catch_unwind`, so the
  existing panic guards did not help.
- **Found by:** `crates/io/tests/mutation_robustness.rs` (seeded corruption of valid files), on its
  first run of the DWG case. Reproduced deterministically with the saved file.
- **Root cause (confirmed with a backtrace):** `acadrust` 0.6.3,
  `src/io/dwg/dwg_reader.rs:2610`, `get_section_buffer_ac18`: `vec![0u8; total_size]`, where
  `total_size` is the AC18 section descriptor's size field read straight from the file, with no
  plausibility check. The same reader has more allocations sized from file data
  (`dwg_reader.rs` lines 145-154, 1833, 1913, 2014, 2039 and others), so patching one site would
  not close the class.
- **Fix (commit 1520658):** DWG files are read by a short-lived
  copy of the program (`--aad-dwg-worker`, `crates/dwg/src/isolated.rs`). A crash, hang
  (180 s timeout), stack overflow or runaway output in the worker is an ordinary error message in
  the parent. Both applications turn it on at start-up. The dependency stays unmodified.
- **Evidence:** before: `cargo run` of a throw-away binary on the fixture printed the backtrace
  above and exited 0xc0000409. After: `apps/cadcraft-cli/tests/dwg_isolation.rs` (4 tests) and
  `crates/dwg` unit tests (5) pass; `cadcraft-cli info <fixture>` exits 1 with
  *DWG: the reader stopped on this file*; 400 corrupted DWG files were read through the worker
  (96 read, 304 refused, 0 crashes of the test process).
- **Regression tests:** `dwg_isolation.rs::the_file_that_aborted_the_reader_is_now_an_error_and_the_process_lives`,
  `::the_command_line_tool_reports_the_bad_file_and_exits_normally`, fixture
  `apps/cadcraft-cli/tests/fixtures/dwg/alloc-bomb-ac18-section-size.dwg`.
- **Remaining risks:** the worker's memory is not capped (needs OS job objects and `unsafe`), so a
  file that makes it allocate a large but possible amount uses that memory until it ends. Library
  users that do not call `isolated::enable()` still read DWG in-process and are exposed. Not
  reported upstream yet.

### BUG-002 (P2, fixed): panning or zooming inside an MSPACE viewport left it stale

- **What happened:** the viewport kept showing its old content until another edit.
- **Root cause (confirmed):** `DocState::store_view` changed the viewport entity quietly, without
  bumping the revision the canvas cache is keyed on.
- **Fix:** commit `19e4a15`; **regression test:** `crates/engine/tests/viewport_live.rs`.

## Open

None confirmed. See the suspected defects and the backlog in [QUALITY_AUDIT.md](QUALITY_AUDIT.md).
