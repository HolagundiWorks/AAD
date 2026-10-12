# UI inventory

What the interface contains, taken from the running application (the control channel:
`ui.menu.list`, `ui.elements`, `engine.commands`) and the source on 2026-10-12, release build of
commit `6dd0f04`'s workspace. Expected behaviour is taken from the command registry (`CommandSpec`)
and the documents named in each row; nothing is invented. "Tested" says what an automated test
covers today.

## Counts

| | |
|---|---|
| Commands in the registry | 398: 351 engine commands (135 interactive with prompts) and 47 UI-only commands; 286 have a menu entry (checked by `menu_integrity`) and 27 a shortcut |
| Commands with neither a menu entry nor an alias | 89 (reachable only by id on the command line, a toolbar, a tool set or the agent) |
| Menu entries | 286 now (282 when the per-menu counts below were taken) in 12 menus: File 17, Edit 13, View 24, Insert 7, Format 25, Tools 17, Draw 45, Dimension 19, Modify 34, Architecture 59, Window 19, Help 3 |
| Controls visible at launch (`ui.elements`) | 66: 61 buttons and 5 tabs; every one has a label |
| UI-only commands | `crates/ui-egui/src/menus.rs::UI_COMMANDS` |

## Screens and regions

| Region | Source | Purpose | Tested |
|---|---|---|---|
| Title bar and window controls | `chrome.rs` | integrated on Windows | `chrome_geometry` (overlap at 4 sizes, 2 scales) |
| Menu bar | `menus.rs` | 12 menus above | `menu_integrity` (every entry), `dialog_dispatch` |
| Toolbars (Draw, Modify, Annotation, ...) | `toolbars.rs` | tool buttons; Classic only | `workspace_modes` (presence), not per button |
| File tabs, layout tabs (Model, Layout1...) | `chrome.rs` | switch drawings and layouts; New layout and Layout list buttons | partly in `docking_workflows` |
| Canvas | `canvas.rs` | drawing, selection, snaps, crosshair, grips, selection window | `drawing_workflow` |
| Properties panel | `palettes.rs` | context-sensitive properties; left in Classic | `workspace_modes` (presence) |
| Layers / Blocks panel | `layers.rs`, `blocks.rs` | right in Classic | presence only |
| Plot window | `plotwindow.rs` | batch plot (the Layout panel was removed) | `docs/sheets.md`; engine tests; no UI test |
| Command line and history | `cmdline.rs` | typed commands, prompts, history window (F2) | `control_cmdline*` |
| Dynamic input | `dyninput.rs` | prompts near the cursor | none |
| Status bar | `chrome.rs` | toggles, coordinates, messages | `chrome_geometry` |
| Context menus | `context_menu.rs` | right-click menu, snap menu | `context_menu` |
| Dialogs | `dialogs.rs`, `formdialog.rs`, `managers.rs`, `plotstyles.rs`, `units_panel.rs` | managers (layers, text, dimension, plot styles, units), generated input forms, About, language, commands reference | `dialog_dispatch`, `style_dialogs`; forms opened by `menu_integrity` (not filled) |
| Widgets | `widgets.rs` | Calculator and Pomodoro overlays | `widgets` |
| Docking | `docking.rs` | drag, float, close, reopen, reload of panels | `docking_workflows` (11 tests) |
| Start tab | `lib.rs` | recent files, new drawing | `open_bytes` |
| Close / "Save changes?" | `closing.rs` | unsaved-changes flow | none |

## Keyboard shortcuts (registry)

Written `Cmd+` in the registry and shown as `Ctrl+` on Windows (`shortcut_label`).

`Ctrl+N` new, `Ctrl+O` open, `Ctrl+W` close, `Ctrl+S` save, `Ctrl+Shift+S` save as, `Ctrl+P` print,
`Ctrl+Z` undo, `Ctrl+Shift+Z` and `Ctrl+Y` redo, `Ctrl+X/C/V` clipboard, `Ctrl+Shift+C` copy with base
point, `Ctrl+Shift+V` paste as block, `Ctrl+A` select all, `Delete` erase selection, `Ctrl+1`
Properties, `Ctrl+9` command line, `Ctrl+Q` quit, `Ctrl+F11` Pro/Classic, `F1` command reference,
`F2` history, `F3` object snap, `F8` ortho, `F10` polar, `F11` object snap tracking, `F12` dynamic
input.

Recorded oddity: `Ctrl+O` is declared by both `open` and `ui.open` (the key handler runs `ui.open`).
Whether this ever misbehaves is **unverified**; see UI-004 in the backlog.

## Canvas interactions

Click picks (Shift removes); an empty click opens a window; a drag opens a window (left to right:
inside only, right to left: crossing); wheel zooms about the pointer; middle-drag pans; hover
highlights; grips on a selected object (click for a hot grip, then a point); double-click on text
starts TEXTEDIT; right-click is Enter while a command runs, otherwise the context menu;
Shift/Ctrl+right-click is the object snap menu; Tab cycles snap candidates. Tested in
`drawing_workflow` and `text_double_click`: pick, empty-click window, window, crossing, drawing by
clicks, Escape, Enter, zoom extents. Not tested: wheel, middle-drag pan, grips, snaps, right-click
as Enter, Tab cycling.

## States

| State | Where | Tested |
|---|---|---|
| Empty drawing | `Session::default()` | `drawing_workflow` starts here |
| Disabled menu items | `CommandSpec::enabled`, 4 disabled in the empty-selection state (Paste, Paste as Block, Paste to Original Coordinates, Clear) | `menu_integrity` skips disabled entries by design |
| Error shown on the command line | `echo` of the engine error | `menu_integrity` (counts messages as an effect) |
| Loading | none: opening, saving and plotting run on the UI thread, there is no progress state | see UI-003 |
| Unsaved changes prompt | `closing.rs` | none |

## Findings from the inventory

- All 282 menu entries resolve to a command (automated).
- Nine entries show nothing in a headless run; each is explained in `menu_integrity.rs`
  (`EXPECTED_SILENT`): three need the platform file dialog, six show something that is already
  showing. The "already showing" reason is **not separately verified** with the panel closed.
- 89 commands have no menu entry and no alias (see above): a discoverability question for the
  product owner, not a defect.
- No control without a label was found among the 66 visible at launch.
- Tooltips, icon sizes and per-button toolbar behaviour have **not** been inventoried yet.
