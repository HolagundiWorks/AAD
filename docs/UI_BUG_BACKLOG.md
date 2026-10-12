# UI bug backlog

Findings from the UI testing programme. Confirmed bugs, observations and hypotheses are kept
apart. Severity: P0 data loss, P1 core drafting blocked, P2 significant interaction or usability
defect, P3 minor, P4 cosmetic. See [BUG_BACKLOG.md](BUG_BACKLOG.md) for the non-UI backlog.

## Confirmed defects

**None found in the basic drawing workflow.** The 10 workflow tests and the menu walk pass on the
unmodified application. The one confirmed issue is a measured stall, now fixed:

### UI-001 (P3): the first pointer pick after opening a large drawing stalls one frame - FIXED

- **Component:** canvas hover or pick; spatial index (`crates/engine/src/spatial.rs`).
- **Observed:** with 100 000 entities open, one frame of 49.5 ms when the pointer first moved over
  the canvas; every later frame 0.3 to 0.4 ms.
- **Root cause (confirmed by reading the code):** `spatial::index` builds the R-tree lazily into a
  global cache the first time anything picks, snaps or window-selects, on the UI thread.
- **Fix:** `spatial::warm` builds the index on a background thread; `canvas::ensure_list` starts it
  when the display list is (re)built, so it runs alongside the list build. One warm-up at a time,
  a no-op on wasm and for spaces under `MIN_INDEXED`. Nothing about what the index contains
  changed. A pick that arrives before the warm-up finishes still builds it itself, as before.
- **Regression test:** `spatial::tests::warm_builds_the_index_without_a_pick` (the index is cached
  with no pick, after `warm`). It asserts the precondition that nothing cached it first; it cannot
  fail on the old code because `warm` did not exist, so the evidence of the fix is the measurement.
- **Before / after, real app, GPU canvas, 100 000 entities, same hover run, release builds:**
  maximum frame 49.45 ms before; 0.52 ms and 0.70 ms in two runs after (median 0.34 ms,
  p95 0.47 and 0.50 ms). One machine. Both "after" runs started hovering several seconds after the
  drawing opened, so a pointer move within the first ~50 ms after open can still meet the index
  being built; that window is not measured.
- **Related checks:** `cargo xtask ci` passes.
- **Left open:** the display list build (about 122 ms at 100 000 entities) still runs on the UI
  thread when a drawing opens or changes.

## Hypotheses (not verified)

### UI-003 (P2 if confirmed): opening, saving and plotting block the UI thread
`CadApp::open_path` runs the open on the thread that draws the window. The reader takes about
0.5 s at 100 000 entities and 5 s at 1 000 000 in the CLI harness. Not timed through the UI yet,
and there is no progress state. Needs a measured reproduction in the real app before a fix.

### UI-004 (P4): `Ctrl+O` is declared by two commands
`open` and `ui.open` both carry `Cmd+O`; the key handler runs `ui.open`. No misbehaviour observed;
only the duplicate declaration is certain.

### UI-005 (P3, reported earlier, unreproduced): wide Properties panel in a layout tab
Seen once during development; not reproduced and not in any test.

## Observations (design questions, not defects)

- 89 of 385 commands have no menu entry and no alias; they are reachable by id only (and through
  toolbars, tool sets and the agent). Whether that is intended is a product decision.
- A click on empty canvas opens a selection window rather than deselecting; Escape deselects. It
  is the classic model and is encoded in `drawing_workflow`. Not a defect.
- Interaction tests cannot start a gesture on the canvas's first pixel column: the press goes to
  the docked panel's edge. Real users can hit this too when a drawing is flush with the panel
  border; whether that is a usability problem needs a person to judge.

## Tooling limitation (not a product defect)

### UI-002: the test harness paints the canvas on the CPU
`egui_kittest` has no GPU target in this application, so harness frame times are about 40 times
the GPU app's at 100 000 entities (16 ms against 0.4 ms). Do not use harness numbers as product
numbers; use the control channel on `aad.exe` (see [UI_BASELINE.md](UI_BASELINE.md)).

## Fixed

Nothing in the UI. (The two earlier non-UI fixes are BUG-001 and BUG-002 in `BUG_BACKLOG.md`.)
