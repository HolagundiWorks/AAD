# Workspaces, panels and editing behaviour

What is implemented (each item has tests) and what is not. File names are in `crates/ui-egui/src`
unless stated.

## Workspaces

`UiState::workspace` (`WorkspaceMode`, `lib.rs`) is `Classic` (default) or `Pro`. Commands:
`ui.workspace.classic`, `ui.workspace.pro`, `ui.workspace.toggle` (Ctrl+F11), and `ui.workspace`
with `{mode}` (reports the mode when called without one). The status bar has a `Pro | Classic`
selector. The choice is saved with the other preferences (`prefs_json`: `workspace`,
`classicPalettes`).

| | Classic | Pro |
|---|---|---|
| Menu bar, status bar, command line | shown | shown |
| Standard toolbar, property row, Draw (left) and Modify (right) toolbars | shown | hidden |
| Side panels: Properties (left), Layers and Blocks (right, tabs) | shown | hidden unless opened from the menu |
| Drawing area | between the toolbars and panels | the whole window |

Switching changes visibility only (`menus::set_workspace`). The drawing, selection, view and
command history are not touched, and no panel state is lost: entering Pro remembers whether the
panels were open and Classic restores that. Test: `tests/workspace_modes.rs`.

## Bottom status bar

Left: model/layout tabs. Centre: cursor coordinates with the drawing units (`INSUNITS`) and the
number of selected objects. Right: grid, grid snap, ortho, polar tracking, object snap tracking,
object snap, lineweight and transparency display, dynamic input, quick properties, command line
toggle, drafting settings, then the `Pro | Classic` selector and the light/dark toggle. Not
implemented: an annotation or plot scale indicator (the engine has no annotation scale).

## Grid and snap

Grid display and grid snap are off in a new session (`Settings::gridmode`, `snapmode`) and are saved
in the preferences (`grid`, `gridSnap`). Object snap is independent of both and stays on. Typed
coordinates are never quantized. Tests: `crates/engine/tests/grid_defaults.rs`,
`tests::grid_preferences_persist`.

## Property toolbar

`toolbars.rs`, the row under the menu: current layer, colour, linetype and lineweight dropdowns.
They read the drawing database every frame (layer table, loaded linetypes, the standard lineweights).

- With a selection they show the selection's values, or `*Varies*` when they differ
  (`current_props`), and a change runs `properties.set` on the selected handles.
- With no selection they show the current settings and change them for new objects
  (`layer.current`, `color`, `linetype`, `lweight`).
- `properties.set` leaves objects on locked layers unchanged and returns `skippedLocked`; if every
  object is locked it fails with the reason. It refuses a linetype that is not loaded. The toolbar
  reports either on the status line.
- Every change is one undo step. ByLayer and ByBlock are offered and applied as such.

Tests: `crates/engine/tests/properties_set.rs`, `toolbars::tests`.

## Panels

- **Properties** (left): `palettes::properties_section`. Shows the selection's properties and
  edits them through `properties.set`; geometry fields depend on the object type.
- **Layers** (right tab): `palettes::layers_section`, the layer table with the state toggles.
- **Blocks** (right tab): `blocks.rs`. Block definitions with a line preview, object and reference
  counts, `Insert` (starts `INSERT` with the name), `Select references`, and the attribute
  definitions of the selected block. External references are not supported and the panel says so.
  Editing attribute values of a reference is `ATTEDIT`; the panel does not edit them.

The layout is the `craft_ui` docking tree stored in `UiState::docking` (`docking.rs`): panels can
be tabbed, floated, resized, closed and reset; the layout is saved with the workspace file.

### Pinning and auto-collapse

`autocollapse.rs`. `UiState::pin_panels` (default on) keeps the panels open. Unpinned (menu: Window
> Pin Side Panels), they collapse when, for `auto_collapse_ms` (default 1500 ms), the pointer has
not been over a side panel and nothing is busy. Busy means: a drag in progress over a panel, a
popup or dropdown open, or keyboard focus in a widget that lies in a side panel. A thin strip at
each side of the drawing area then brings them back when the pointer enters it; so does opening a
panel from a menu or pinning. A panel the user closed stays closed and shows no strip.

The decision is the pure function `autocollapse::step`, driven each frame by `autocollapse::run`
from the real pointer, focus and popup state; collapsing sets `show_palettes`, so the docking layout
and the canvas rectangle follow. Tests cover the state machine (including that expansion cannot
re-collapse inside the delay, so there is no flicker loop) and the real shell
(`tests/workspace_modes.rs`). Limit: it collapses and restores all side panels together, not one by
one; hiding a single panel is the panel's own close button.

## Command line

Present in both workspaces (View/status toggle). Commands, aliases and shortcuts do not depend on
toolbar visibility: with every toolbar hidden in Pro mode a command typed at the prompt runs as
usual (tested). The command line is an overlay at the bottom of the drawing area, not a docked
bar.

## Text editing

`TEXTEDIT` (`modify2.rs`) edits TEXT, MTEXT, attribute definitions and dimension text in place:
the entity keeps its handle, insertion point, rotation, style, height and justification; MTEXT
formatting codes are shown and kept as typed; one undo step; objects on locked layers are refused.
**Root cause of "imported text cannot be edited":** the command existed but nothing in the drawing
area started it: a double-click on text did nothing. `canvas::edit_selected_text_at` now starts
`TEXTEDIT` on a double-clicked text, mtext, attribute definition or dimension
(`tests/text_double_click.rs`).

Imported DWG text reaches the same entity kinds as DXF text (the DWG is converted to DXF in memory,
see [architecture.md](architecture.md)). No representative architectural DWG is available in this
repository, so editing of real imported DWG files has not been verified beyond that mapping.
Limits: text inside a block definition is not edited in place (there is no block editor or
in-place reference edit); text exploded to geometry, proxy objects and unmodelled types (kept as
`Unknown`) are not text entities and are not editable.

## Hatch selection

**Root cause of "hatches are not selected reliably":** `select::entity_distance` measured the
distance to the lines the hatch draws, so a pattern hatch could only be picked within the pick
aperture of a pattern line or its boundary; only solid fills were hit "inside" (using the bounding
box, not the area). `select::inside_hatch` now tests the hatch area (odd parity across the loops
for the normal style, inside an outer loop for the outer and ignore styles). A click inside the
area picks the hatch, but as a last resort: its boundary geometry or any other object within the
aperture wins, so the hatch and its boundary stay independently selectable. Associative boundaries
are untouched. Test: `crates/engine/tests/hatch_pick.rs` (36 interior points fail before the fix and
pass after, boundary click selects the boundary, outside selects nothing).
`SELECTIONCYCLING` and the hatch edit command (`hatchedit`: pattern, scale, angle) are unchanged.

## Not implemented

- Annotation or plot scale indicator.
- Docked (non-overlay) command line.
- Per-panel auto-collapse; panel pinning applies to all side panels.
- Editing text inside a block definition in place on the drawing (it is edited from the Blocks panel with `blocktextedit`, which changes every reference).
- Hatch boundary editing and boundary validation messages beyond the existing commands.
- True-colour (RGB) values in the colour dropdown (the `…` button offers the 255 indexed colours; use `COLOR`
  for RGB).

## Added after the first release of this page

- **Block definition text:** `blocktext {block}` lists the text objects of a definition and
  `blocktextedit {block, handle, text}` edits one (TEXT, MTEXT, attribute definition). It changes
  every reference, refuses locked layers and non-text objects, and is one undo step. The Blocks
  panel shows an edit field for each. Test: `crates/engine/tests/block_text_edit.rs`.
- **Reference attributes:** selecting one block reference shows its attribute values in the Blocks
  panel; editing one runs `attedit`.
- **Colour:** the `…` button beside the colour dropdown opens a grid of the 255 indexed colours.
