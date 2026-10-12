# Bug backlog

Defects found by the quality programme, with evidence. A defect is **confirmed** only when it was
reproduced; everything else is in the *hypotheses* list in [KNOWN_ISSUES.md](KNOWN_ISSUES.md).
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
  above and exited 0xc0000409. After: `apps/aad-cli/tests/dwg_isolation.rs` (4 tests) and
  `crates/dwg` unit tests (5) pass; `aad-cli info <fixture>` exits 1 with
  *DWG: the reader stopped on this file*; 400 corrupted DWG files were read through the worker
  (96 read, 304 refused, 0 crashes of the test process).
- **Regression tests:** `dwg_isolation.rs::the_file_that_aborted_the_reader_is_now_an_error_and_the_process_lives`,
  `::the_command_line_tool_reports_the_bad_file_and_exits_normally`, fixture
  `apps/aad-cli/tests/fixtures/dwg/alloc-bomb-ac18-section-size.dwg`.
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

None confirmed that is still unfixed. Hypotheses and limits: [KNOWN_ISSUES.md](KNOWN_ISSUES.md).

## PERF-001 (P3): editing existing objects cost about 40 ms at 100 000 entities - FIXED

- **Measured before** (`cargo bench -p aad-engine --bench drawing_operations`, 1 000 objects
  in a 100 000-entity synthetic drawing, `Session::execute`): move 41.8 ms, rotate 40.3 ms, erase
  39.2 ms, recolour 40.3 ms; copy 2.7 ms; undo of a move 0.62 ms.
- **Profile** (temporary timers around each stage of `Session::execute`, since removed; five move
  samples): command body 2.2 to 2.7 ms, group fix-up 0.0005 ms, association fix-up 0.5 to 0.6 ms,
  **`refuse_non_finite` 35.3 to 37.5 ms**. So 90% of the time was the check that refuses edits
  producing NaN or infinite coordinates, not the edit.
- **Root cause (confirmed):** `finite::changed_non_finite` skips chunks shared with the previous
  version, but 1 000 scattered objects dirty nearly every chunk, and for every entity in a dirty
  chunk it called `EntityStore::get(handle)`, which scans the chunk linearly (up to 1 024 entities)
  to find the old version: about 100 000 lookups of roughly 500 comparisons each.
- **Fix:** `EntityStore::changed_since(&before)` compares each changed chunk with the chunk of the
  same id in `before` by pointer, position by position (falling back to that chunk's pointer set if
  positions shifted), and returns only the changed entities; `changed_non_finite` checks just
  those. It can report an entity that merely moved between chunks, never miss a changed one, so
  the check is as strict as before.
- **Regression tests:** `store::tests::changed_since_matches_the_handle_lookup_definition` (30
  rounds of random modify, remove and add, compared with the old definition) and the existing
  `crates/engine/tests/non_finite_edits.rs` (huge results are still refused), both passing.
- **Measured after** (same benchmark, same machine, one run): move 5.30 ms, rotate 5.36 ms, erase
  5.64 ms, recolour 5.61 ms (about 7.5 times faster); copy 2.43 ms and undo 0.48 ms unchanged
  within noise. The non-finite stage alone fell from about 36 ms to 1.6 to 2.8 ms.
- **Left:** about 5 ms of an edit is now the command body (2.3 ms) plus association fix-up
  (0.5 to 0.8 ms) plus the check; none has been profiled further.
