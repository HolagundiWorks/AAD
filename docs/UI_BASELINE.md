# UI baseline

What was measured and run on 2026-10-12, before any change to the interface. Nothing in the
application was modified for these results; only tests were added.

## Environment

Windows 11 Home 10.0.26300, 16 threads, 15.8 GB, rustc 1.99.0, **release** builds, warm cache,
nothing else heavy running (the benchmark run for the engine was stopped for these measurements).
One machine, one run each unless stated. These numbers describe this machine only.

## Test results

`cargo test -p cadcraft-ui-egui`: all pass (no failures, none skipped; two `#[ignore]`d tests are
the hardware-dependent latency run and the DXF writer used to prepare it). New in this milestone:
`drawing_workflow` (10 tests), `menu_integrity` (2 tests). `cargo clippy -p cadcraft-ui-egui
--all-targets -- -D warnings` is clean.

Three of the first draft's assertions failed before they were corrected, and **none was an
application defect**: a gesture started on the canvas's first pixel column (the press goes to the
docked panel's resize edge), a drag that moved in one jump instead of steps, and a wrong
expectation that an empty click deselects (in this product it opens a selection window and Escape
deselects). See [UI_TESTING_STRATEGY.md](UI_TESTING_STRATEGY.md).

## Frame and input latency in the test harness (CPU-painted)

`ui_latency`, 10 000 and 100 000 synthetic entities, one `run_steps(1)` per sample. The harness has
no GPU target, so the canvas is painted on the CPU: **these are not the shipped application's
frame times**. They are useful for relative cost of the non-paint work and for catching
regressions, and they show what the CPU fallback costs.

| Action | 10 000 median / p95 ms | 100 000 median / p95 ms |
|---|---|---|
| idle frame | 1.70 / 2.01 | 15.9 / 16.2 |
| pointer move over canvas | 1.84 / 2.37 | 16.3 / 17.1 |
| activate LINE tool | 2.30 / 2.91 | 17.5 / 19.9 |
| click (press + release frames) | 3.84 / 4.36 | 32.5 / 33.7 |
| recolour 1 000 objects + next frame | 16.1 / 17.0 | 146.7 / 150.2 |
| wheel zoom step | 3.63 / 4.54 | 32.2 / 35.4 |
| open drawing and first six frames | 48 ms | 368 ms |

At 100 000 entities the CPU-painted canvas spent about 9.8 ms per frame painting (`draw_ms`).

## The real application, GPU canvas (control channel)

`cadcraft.exe` with a 100 000-entity DXF (23.6 MB, 233 778 primitives), renderer `gpu`:

| Measure | Value |
|---|---|
| idle frame (`frameMs`, 5 samples) | 0.26 to 0.45 ms |
| pointer moves over the canvas, run 1 (30 samples) | median 0.31 ms, p95 1.60 ms, **max 49.5 ms** |
| the same run repeated | median 0.30 ms, p95 0.43 ms, max 0.44 ms |
| display list build (`buildMs`, at open) | 124.7 ms |
| mesh conversion (`meshMs`, at open) | 10.4 ms |

`frameMs` is the application's own measure of the CPU time of a frame; it does not include the
GPU's work or presentation, so it is not input-to-photon latency. The single 49.5 ms frame in run 1
was the first pick after opening and did not recur; fixed (UI-001), see the backlog for the after figures.

## From the command-line performance harness (`cadcraft-cli perf`, from the quality programme)

| Entities | display list | first pick | pick (per call) | DXF read | DXF write |
|---|---|---|---|---|---|
| 100 000 | 125 ms | not recorded | 0.88 us | 479 ms | 291 ms |
| 1 000 000 | 1 304 ms | 581 ms | not recorded | 5 188 ms | 3 378 ms |

## Not measured

Input-to-visible-response latency (needs a camera or a present-time probe), dialog open and close
time, text-editing responsiveness, stalls during import and export in the UI (the numbers above
suggest 0.5 s at 100 000 entities and 5 s at 1 000 000 for the read alone, on the UI thread: UI-003
is a hypothesis until timed in the app), memory, and any non-Windows platform.

## Targets

None are defined: the instructions say to set them after measurement and on stated supported
hardware, and no supported-hardware statement exists in the repository. Proposal for discussion: a
frame under 16 ms for editing and hover up to 100 000 entities on the GPU path, and no UI-thread
operation over 100 ms without a progress state. Both are met today except opening and the first
pick at large sizes.
