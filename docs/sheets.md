# Sheets and batch plotting

A layout tab is a sheet. The commands are in `crates/engine/src/cmd/sheet.rs` (and `projfile.rs`
for the project-aware ones); the Plot window only calls them. There is no separate Layout panel: it
was removed, and a saved panel layout that still names it is read as unusable and the default panel
arrangement is used (other preferences are kept). Sheet setup is done with the commands below.

## Live viewports

Panning or zooming inside an MSPACE viewport changes what the viewport shows, so the drawing is
redrawn at once. (Before, that change was applied without telling the canvas to rebuild, so the
viewport could show stale content until another edit.) Regression test:
`crates/engine/tests/viewport_live.rs`.

## Sheet commands

Used from the command line, menus, scripts and the control channel:

| What | Commands |
|---|---|
| Paper size and orientation | `pagesetup` |
| Viewports: scale (typed as `1:100`), lock, remove, add; in a project, which floors and layouts each shows | `viewport.list`, `viewport.set`, `viewport.add`, `viewport.remove`, `floor.viewport` |
| Title block fields, adding a title block | `sheet.fields`, `sheet.titleblock` |
| Saved layouts (apply as the sheet's title card, delete, save) | `sheet.template.*` |

## Title block

`sheet.titleblock` draws a border around the printable area and inserts the block `AAD-TITLEBLOCK`
(180 x 56 mm of real paper size, lower right). Its fields are block attributes: `PROJECT`, `CLIENT`,
`TITLE`, `FLOOR`, `DRAWN`, `DATE`, `SHEET`, `SCALE`, `REV`. They start from the project details,
the floor of the file and the scale of a viewport added afterwards. `sheet.fields` sets them by
tag; any block with attributes of those tags is filled the same way, so a template with its own
title block works too.

## Saved layouts (sheet templates)

A saved layout is everything on the sheet except its viewports (border, title block, notes), the
blocks they use, and the page setup. `sheet.template.save {name, scope}` keeps it in the drawing
(`drawing`, travels with the file) or as a file for the user (`user`, in the AAD settings folder, or
in `folder` for a folder shared by an office). `sheet.template.apply {name, layout?, fields?}` sets
the page up, places the objects and fills the fields. Applying again replaces what an earlier
template put on that layout, never what you drew. A damaged template is refused and changes
nothing. Viewports are not saved: what they show belongs to each drawing.

## Floors in viewports

`floor.viewport {handle, floors?, categories?, plans?}` freezes the Xref layers of the other floors
in one viewport only, so the main drawing carries one sheet per floor, or a floor's plan with its
beam layout over it. The layer's global visibility (`floor.set`) still applies: a layout drawing
must be shown to appear in any viewport.

## Batch plot (File > Batch Plot)

`plot.batch {items?, sheets?, includeHidden?, folder?}` writes one PDF per item into the project's
`Exports` folder:

- an item is a floor plan or one of its layouts, read from its **saved** file (a warning says when
  the open drawing has unsaved changes); a file with a sheet (a layout with a viewport) plots the
  sheet, otherwise its model space fitted to the paper;
- by default every item marked for printing, shown and not frozen is plotted; the others are
  reported as skipped with the reason, unless `includeHidden` is set;
- `sheets` are layouts of the drawing you are in (the main drawing); their hidden and frozen Xref
  layers stay out of the output. A drawing outside a project can still plot its own sheets.

The Plot window lists the project's floors with their layouts (ticked when marked for printing),
the sheets of the drawing, a folder, and a Plot button; it shows the files written and what was
skipped.

## Not implemented

Revision tables, sheet numbering across a set, templates that carry viewports, a sheet index, and
printing to a physical device (PDF only).
