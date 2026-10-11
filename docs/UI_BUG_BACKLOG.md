# UI bug backlog

Findings from the UI testing programme. Confirmed bugs, observations and hypotheses are kept
apart. Severity: P0 data loss, P1 core drafting blocked, P2 significant interaction or usability
defect, P3 minor, P4 cosmetic. See [BUG_BACKLOG.md](BUG_BACKLOG.md) for the non-UI backlog.

## Confirmed defects

**None found in the basic drawing workflow.** The 10 workflow tests and the menu walk pass on the
unmodified application. The only confirmed behaviour worth recording is a measurement:

### UI-001 (P3): the first pointer pick after opening a large drawing stalls one frame

- **Component:** canvas hover or pick; spatial index.
- **Preconditions:** open a drawing with 100 000 entities, then move the pointer over the canvas.
- **Observed:** one frame of 49.5 ms (`ui.inspect` `perf.frameMs`); all later frames 0.3 to 0.4 ms;
  the same run repeated later has a maximum of 0.44 ms. Reproduced once (one session), the
  repeat confirms it is a one-time cost.
- **Expected:** no frame over the display budget while the pointer moves.
- **Suspected cause (hypothesis, not yet traced in a profiler):** the entity spatial index is built
  lazily on the first pick, on the UI thread. The CLI harness shows the same first-pick cost
  growing to 581 ms at 1 000 000 entities.
- **Regression test:** none yet. A fix would build the index when the drawing is opened (or on a
  worker), and could be tested by asserting the first `pick` after `open_drawing` is not the one
  that builds it.
- **Status:** open, not fixed (a fix changes when work happens; needs the owner's agreement to move
  it, and a benchmark before and after).

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
numbers; use the control channel on `cadcraft.exe` (see [UI_BASELINE.md](UI_BASELINE.md)).

## Fixed

Nothing in the UI. (The two earlier non-UI fixes are BUG-001 and BUG-002 in `BUG_BACKLOG.md`.)
