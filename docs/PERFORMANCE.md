# Performance baseline

Measured numbers for the engine, renderer and file I/O on 2026-10-12. The user-interface numbers
are in [UI_BASELINE.md](UI_BASELINE.md). No targets are set here: targets come after the supported
hardware is stated, and no such statement exists yet.

## How it was measured

- Windows 11, 16 threads, 15.8 GB, rustc 1.99.0, `cargo bench` (release profile, criterion 0.5).
- Each benchmark: 1 s warm-up, 3 s measurement, 100 samples (10 to 20 for the slow ones). Values are
  criterion's `[low estimate high]`; the table shows the estimate.
- One machine, one run, nothing else running. Treat differences under about 10% as noise: the
  first geometry run (a few minutes earlier, while other work was running) and the second differ by
  up to 2x on the intersection benches, and the DXF benches report changes of 7 to 18% between two
  runs. Use these numbers for orders of magnitude and for before/after on the same machine, not as
  absolutes.
- Data: `tools/quality/synthetic.rs`, a deterministic drawing (lines, circles, arcs, bulging
  polylines and text, equal shares, constant density). It is not a real architectural drawing.
- Commands: `cargo bench -p aad-geom --bench geometry`, `-p aad-engine --bench selection`,
  `--bench drawing_operations`, `-p aad-render --bench render`, `-p aad-io --bench dxf`,
  each followed by `-- --warm-up-time 1 --measurement-time 3`. Raw output:
  `target/quality/bench-run2.txt` and `bench-run2-ops.txt`; these raw files were deleted by the 2026-10-12 cleanup (the tables here are the record; rerun the commands to regenerate).

## Geometry (`aad-geom`)

| Operation | Time |
|---|---|
| line-line x 1 000 pairs | 9.7 us |
| line-circle x 1 000 | 12.6 us |
| circle-circle x 1 000 pairs | 16.1 us |
| `Line::closest` x 1 000 | 1.89 us |
| `Mat3::apply` x 1 000 points | 0.70 us |
| point-in-polygon, 100 / 1 000 / 10 000 vertices | 74 ns / 705 ns / 6.9 us |
| shoelace area, 100 / 1 000 / 10 000 vertices | 66 ns / 677 ns / 6.8 us |
| polyline area with bulges, 100 / 1 000 / 10 000 | 1.14 us / 11.6 us / 115 us |
| polyline tessellate, 100 / 1 000 / 10 000 | 9.7 us / 79.7 us / 739 us |
| arc tessellate, r = 1000, tolerance 0.01 | 7.2 us |

All scale linearly with vertex count.

## Selection and snapping (`aad-engine`)

| Operation | 10 000 entities | 100 000 entities |
|---|---|---|
| first pick (builds the index) | 1.74 ms | 5.89 ms |
| pick x 200, index warm | 77 us | 120 us |
| window select, 10% side | 7.9 us | 110 us |
| crossing select, 10% side | 13.8 us | 127 us |
| fence select | 1.70 ms | 21.6 ms |
| object snap x 200 | 93 us | 140 us |

The first pick here is 5.9 ms at 100 000 entities, against the 49.5 ms frame seen in the application
(UI-001). I have not explained the difference; the application frame also contained other work
that was not isolated. Both are one-time costs, which is why the index is now built in
the background. Picking is sub-linear: 12 times the entities cost 1.6 times as much per pick.
Fence selection is the one operation that scales close to linearly (12.7 times for 10 times the
entities).

## Editing 1 000 objects in a 100 000-entity drawing (`Session::execute`)

| Command | Time |
|---|---|
| add one line | 386 us |
| copy ~1 000 | 2.67 ms |
| undo of a move of ~1 000 | 0.62 ms |
| **move ~1 000** | **41.8 ms** |
| **rotate ~1 000** | **40.3 ms** |
| **erase ~1 000** | **39.2 ms** |
| **recolour ~1 000** | **40.3 ms** |

**Finding (hypothesis, not yet profiled):** commands that modify existing objects cost about 40 ms
each at 100 000 entities, 15 times a copy of the same objects and 65 times their undo. The time is
suspiciously flat across four different operations, which points at something they share (for
example copying or touching every chunk of the entity store, or an index or extents recomputation
after the edit) rather than at the geometry. It is below the 100 ms feel threshold but is the
largest single cost of a routine edit. Recorded as PERF-001 in [BUG_BACKLOG.md](BUG_BACKLOG.md); it
needs a profile before any change.

## Display list (`aad-render`)

| Operation | 10 000 | 100 000 |
|---|---|---|
| model space at zoom extents | 13.3 ms | 129 ms |
| layout with one viewport (clipped) | 4.95 ms | 51.9 ms |
| one pan step inside that viewport | 2.72 ms | 51.9 ms |

Linear in entity count. The pan step costs the same as building the layout, because the viewport
content is rebuilt when the view centre moves (since the BUG-002 fix); at 100 000 entities that is
52 ms per pan frame, a candidate for caching.

## DXF read and write (`aad-io`)

| Entities | write | read |
|---|---|---|
| 10 000 | 29.3 ms | 43.3 ms |
| 50 000 | 140 ms | 222 ms |

Linear. The one-shot figures from `aad-cli perf` for 100 000 and 1 000 000 entities are in
[UI_BASELINE.md](UI_BASELINE.md); they agree with these (about 4.4 us per entity to read).

## Not measured

Memory use, cold-start time, DWG read, hatch and dimension rendering at scale, plot and PDF export,
snapping with many active snap modes, and any machine other than this one.

## Reproduce

```text
cargo bench -p aad-engine --bench selection -- --warm-up-time 1 --measurement-time 3
AAD_BENCH_LARGE=1 cargo bench -p aad-engine --bench selection     # adds 1 000 000 entities
```

## Update: PERF-001 fixed

The 40 ms edits were the non-finite-result check, not the edits (profile and fix in
[BUG_BACKLOG.md](BUG_BACKLOG.md)). Same benchmark after the fix, one run:

| Command (about 1 000 objects, 100 000 entities) | Before | After |
|---|---|---|
| move | 41.8 ms | 5.30 ms |
| rotate | 40.3 ms | 5.36 ms |
| erase | 39.2 ms | 5.64 ms |
| recolour | 40.3 ms | 5.61 ms |
| copy | 2.67 ms | 2.43 ms |
| undo of a move | 0.62 ms | 0.48 ms |
| add one line | 386 us | 192 us |

The "finding" paragraph above describes the state before the fix.

## Update: display-list build of text-heavy drawings

Measured with the ignored probes in `apps/aad-cli/tests/perf_probe.rs` (`AAD_PERF_FILE=<drawing>`,
`cargo test --release -p aad-cli --test perf_probe -- --ignored --nocapture --test-threads=1`) on a
drawing with 705 474 primitives, most of it filled TrueType text. Text fill dominated the build.

| Step | Display-list build | Triangle vertices |
|---|---|---|
| Slab trapezoids (`triangulate_evenodd`) | about 1.0 to 1.6 s | 37.3 M |
| Ear clipping (`triangulate_outline`, `earcutr`) | about 0.32 to 0.38 s | 12.1 M |

`triangulate_outline` classifies loops by nesting depth, triangulates each outer boundary with its
holes, and falls back to `triangulate_evenodd` when the result area differs from the outline area or
the input is hostile (too many loops or points). A per-glyph cache was tried and removed: the gain
over ear clipping was small for the added key-quantisation risk.

Still open: DWG parse (1.2 to 1.5 s), DXF write and
read (about 0.55 s), mesh upload (about 170 ms), viewport pan rebuild in layouts, first-pick spatial
index build (70 to 140 ms).
