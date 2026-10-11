# UI test matrix

Status per area. **Automated** = a test in the repository checks it and passes; **Measured** = a
number was recorded ([UI_BASELINE.md](UI_BASELINE.md)); **Manual** = looked at by a person during
development, not repeatable; **Open** = not done; **N/A** = the feature does not exist.

## A. Functional interaction

| Check | Status | Where |
|---|---|---|
| Every menu entry resolves to a command | Automated | `menu_integrity` |
| Every enabled menu entry reacts (or is explained) | Automated | `menu_integrity` |
| Control visible / enabled state | Partly | menu enabled state is read from the registry; toolbar buttons Open |
| Single click | Automated | pick, tools, `drawing_workflow` |
| Double click | Automated | `text_double_click` (the second click is simulated by calling the handler: the harness does not model double-click timing) |
| Right-click context menu | Automated | `context_menu` |
| Hover feedback | Open | |
| Keyboard: Escape, Enter | Automated | `drawing_workflow` |
| Keyboard: Tab, arrows, Space | Open | |
| Keyboard shortcuts | Partly | `control_cmdline_key`; the 27 registry shortcuts are not each exercised |
| Dialog open / cancel / confirm | Partly | `dialog_dispatch`, `style_dialogs`; generated forms are opened by `menu_integrity` but not filled in |
| Dropdown selection | Open | |
| Text editing and validation, focus | Partly | `text_double_click`, `workspace_focus` |
| Disabled actions refuse cleanly | Open | |
| Undo / redo | Automated | `drawing_workflow` (draw then undo then redo gives the identical drawing) |
| Drawing data not corrupted | Automated | save and reopen test; engine properties in `docs/TESTING_STRATEGY.md` |

## B. Canvas workflow

| Step | Status |
|---|---|
| Launch, new drawing | Automated (the harness launches the real UI; a fresh drawing) |
| Draw a line, circle, rectangle, polyline by clicks | Automated |
| Select and deselect | Automated (click, Escape) |
| Window and crossing selection | Automated |
| Move, copy, rotate, erase | Automated through `app.run` on the clicked selection (the command forms; their prompt sequences by click are Open) |
| Edit text | Partly (`text_double_click`) |
| Edit entity properties | Measured only (`properties.set` latency); the panel itself Open |
| Select and edit hatches | Open |
| Zoom extents, fit | Automated; wheel zoom and pan are measured, not asserted |
| Snapping and coordinate entry | Open for the UI (engine snaps have unit tests); typed coordinates are covered by `control_cmdline` |
| Open and close panels without losing the drawing | Automated (`workspace_modes`, `docking_workflows`) |
| Save and reopen | Automated (DXF) |

## C. Classic CAD behaviour

| Behaviour | Status |
|---|---|
| Escape cancels a command | Automated |
| Enter finishes a polyline | Automated |
| Right-click as Enter while a command runs | Open |
| Command repetition | Open |
| Selection before a command | Automated (via `app.run` with the selection's handles) |
| Dynamic prompts | Open |
| Switching tools leaves no stale state | Open |
| Shortcuts do not fire during text entry | Open (the code returns early when egui wants the keyboard; not tested) |

## D. Consistency

Open, apart from status-bar overlap at four window sizes and two scales (`chrome_geometry`) and
control sizing (`shared_controls`). A systematic pass over spacing, typography, icon size and focus
states is not done; the tests that save screenshots (`CRAFT_UI_VISUAL_DIR`) are the starting point.

## E. Visual regression

**Open.** The existing tests can write PNGs but nothing compares them with approved images, so no
baseline was created. Approving images needs a person to look at them, and the harness renders on
the CPU path, which differs from the GPU canvas. Proposed first step: approve the three
`chrome_geometry` sizes as baselines under `crates/ui-egui/tests/snapshots/` with a tolerance, after
review.

## F. Window sizes and scaling

| Size | Status |
|---|---|
| 1400x800, 1100x700, 900x560, 1600x900 | Automated: the canvas stays inside the window and picks correctly (`drawing_workflow`) |
| 1200x760 and 900x560 at 1x and 2x | Automated: status labels do not overlap (`chrome_geometry`) |
| Minimum supported size | Open: no minimum is documented in the repository |
| Menus and dialogs inside the window at small sizes | Open |
| Resize during an active command | Open |

## G. Performance

Measured, see [UI_BASELINE.md](UI_BASELINE.md). Dialog open and close latency, text editing
responsiveness and UI stalls during import and export: Open (the last is expected to show a stall;
see UI-003).

## Environment note

Everything above ran on Windows 11, one machine. A result in this file means "passed there".
