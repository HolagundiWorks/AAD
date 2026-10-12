# User interface

The interface as built (egui / eframe, GPU canvas). The control-by-control inventory and the
measured behaviour are in [UI_INVENTORY.md](UI_INVENTORY.md), [UI_TEST_MATRIX.md](UI_TEST_MATRIX.md)
and [UI_BASELINE.md](UI_BASELINE.md); workspace details in [workspaces.md](workspaces.md).

## Layout

| Region | Notes |
|---|---|
| Menu bar | 12 menus: File, Edit, View, Insert, Format, Tools, Draw, Dimension, Modify, Architecture, Window, Help (282 entries) |
| Toolbars | Classic only: Draw and Modify (with Dimension and Leader) as two vertical bars on the left, and a property row under the menu |
| Context bar | Both workspaces: current layer (searchable), floor, drawing space, panel pin/auto-hide ([ACTIVE_DRAWING_SPACE.md](ACTIVE_DRAWING_SPACE.md)) |
| File tabs and layout tabs | Drawings along the top; Model and layout tabs along the bottom, with New layout and a layout list |
| Canvas | GPU-drawn; crosshair, snap markers, grips, selection window |
| Panels | Layers, Properties and Blocks (one tabbed stack on the right), Plot window; docked, floatable, closable, pin and auto-collapse |
| Command line and history | Typed commands with prompts; F2 opens the history |
| Status bar | Toggles (snap, ortho, polar, tracking, dynamic input), coordinates, units popup, messages |
| Widgets | Architect calculator and Pomodoro timer as floating overlays |

Two workspaces: **Classic** (toolbars and side panels) and **Pro** (full-screen drawing area with
command line and status bar), switched with `Ctrl+F11`. Switching changes visibility only. Two
themes, light (white drawing area) and dark (black), with no other colour customisation. The interface is English only.


## Working with commands

Pick a tool from a menu or toolbar, or type its name or alias at the command line. The prompt
asks for points, objects or options; click on the canvas or type coordinates. Escape cancels,
Enter finishes (a polyline, for example), and right-click acts as Enter while a command runs;
otherwise it opens the shortcut menu (Shift or Ctrl with right-click opens the snap menu).
Selection before a command is supported. Scripts and the control channel run the same commands
without dialogs.

## Mouse

Click picks (Shift removes); a click on empty space or a drag opens a selection window (left to
right: inside only; right to left: crossing); the wheel zooms about the pointer; middle-drag pans;
double-click on text edits it; grips on a selected object edit it; Tab cycles snap candidates.

## Keyboard shortcuts

`Ctrl+N`, `Ctrl+O`, `Ctrl+W`, `Ctrl+S`, `Ctrl+Shift+S`, `Ctrl+P`, `Ctrl+Z`, `Ctrl+Shift+Z` / `Ctrl+Y`,
`Ctrl+X/C/V`, `Ctrl+Shift+C/V`, `Ctrl+A`, `Delete`, `Ctrl+1` Properties, `Ctrl+9` command line,
`Ctrl+Q`, `Ctrl+F11` workspace, `F1` command reference, `F2` history, `F3` object snap, `F8` ortho,
`F10` polar, `F11` object snap tracking, `F12` dynamic input (from the command registry; complete
list in [UI_INVENTORY.md](UI_INVENTORY.md)). Shortcuts do not fire while a text field has focus.

## Dialogs

Managers (layers, text and dimension styles, plot styles, units), generated input forms for the
architecture commands ([input-forms.md](input-forms.md)), About, language, command reference, and
the "Save changes?" prompt on close.

## Window sizes

Tested: 1400x800, 1100x700, 900x560 and 1600x900 (canvas stays inside the window and picks
correctly), and 1200x760 and 900x560 at 1x and 2x scale (status labels do not overlap). No minimum
window size is defined or documented.

## Limitations

- Opening, saving and plotting run on the UI thread and have no progress display.
- Files are opened through a platform file dialog supplied by the app; there is none in headless runs.
- The Plot window and dynamic input have no automated UI test.
- No screenshot baselines are approved, so visual regressions are not detected automatically.

## Automation coverage

Click-driven drawing workflow (`drawing_workflow`), a walk over every menu entry (`menu_integrity`),
docking, workspaces, context menu, dialogs, widgets, status-bar geometry, and the control channel.
See [TESTING.md](TESTING.md) and [UI_TESTING_STRATEGY.md](UI_TESTING_STRATEGY.md).
