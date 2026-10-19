# Take-off recorded in ShilpiDB

O-Draft records its take-off in a ShilpiDB database, and O-CoreOp reads it from there. The contract
(version 1, revised 2026-10-19) is `docs/TAKEOFF-SHILPIDB.md` in the O-CoreOp repository; the hand-off that
asked for this is `docs/HANDOFF-O-DRAFT-TAKEOFF.md` there. This page says what exists on the O-Draft side.

## Commands

### `takeoff.record`
`{server?, path?, file?, annotate?, paper?, level?, bearing?}` (Architecture > Take-off > Record in ShilpiDB;
also reachable from scripts, the control channel and MCP).

- It runs the same collection step as `bridge.export` (`cmd/bridge.rs`, `collect`), so a record and the
  bridge file's row are the same object.
- **Target.** By default the shared `shilpid` on `127.0.0.1:7420`, started (with `shilpi-http` on 7421) when
  nothing answers; the programs are found through `SHILPIDB_BIN` (a folder), `%LocalAppData%\ShilpiDB\bin`,
  then `PATH`, and the data file is `%LocalAppData%\OSAS\takeoff\takeoff.vdb`. `server: "host:port"` names
  another server and never starts one. `path` writes a `.vdb` file instead; `file: true` puts it beside the
  saved drawing (`<drawing>.takeoff.vdb`). A file that has records but no header is refused, so an entity
  store written by `vdb.export` is never overwritten.
- **Protocol** (the contract has no transactions, so the header is the completion marker): claim the slot by
  writing the header with `"state": "recording"` and reading it back; write every row; delete this slot's rows
  that are no longer measured; delete the old annotated-PDF chunks and write the new ones; write the header
  again with `"state": "complete"` (and `annotated_pdf`); save. A recording that dies in between leaves the
  drawing `recording`, which O-CoreOp refuses to import.
- The result gives the target, `slot`, `headerId`, counts per kind, `written`, `added`, `deleted` (and
  `changed` / `unchanged` for a file), the PDF's size, anything `skipped` (two rows with the same number) and
  the warnings. A drawing that cannot be plotted still records; the result then warns that there is no
  annotated drawing.

### `takeoff.annotate`
`{path?, paper?}` (Architecture > Take-off > Annotated Drawing (PDF)) writes the annotated drawing as a PDF
(default `<drawing>.takeoff.pdf`). It works on a copy of the drawing: each measured item is drawn again on the
layer `TAKEOFF-ANNOT` in the colour of its kind (walls red, openings green, columns blue, lintels magenta,
rooms cyan, slabs orange) with its mark and measure beside it, and a title line (drawing, revision, date) and a
legend with counts stand below the drawing. The drawing itself is not changed.

## Records

Everything is millimetres, whatever the drawing units. The id is `(kind << 56) | (slot << 32) | n`.

| Kind | Record |
|---|---|
| 1 | Header, one per drawing, `n = 0`: `format` `hcw-takeoff-shilpi`, `version` 1, `units` `mm`, `state`, `source` (`app` `O-Draft`, `drawing`, `drawing_id`, `project_office_id` while the drawing is linked to an AORMS project, `exported`, `revision_sha256` while the saved file still matches its recorded revision), and the bridge file's `project`, `levels`, `beam_depths_mm`, `opening_schedule`; `annotated_pdf` (`sha256`, `bytes`, `chunks`, `pages`, `generated`) once a PDF is stored |
| 3 to 8 | Wall, opening, column, lintel, room, slab: the payload is the bridge row as written; the box is the item's bounds in millimetres (the room's box when one is drawn, else its name text; zeros when it has no geometry) |
| 9 | A chunk of the annotated PDF: raw bytes, at most 512 KiB, `n` from 0, zero box; at most 64 MiB in all |

**Slot.** One database holds many drawings, so each owns a 24-bit slot: `SlotFor(drawing_id)` is the 32-bit
FNV-1a of the id folded as `(h ^ (h >> 24)) & 0xFFFFFF` (1 if that is 0; the contract's two test vectors are
unit tests). When the header at that slot belongs to another drawing the next slot is tried (after the last
comes 1). The slot is kept in the drawing (`takeoffSlot` in the project data, with its `drawing_id`) so it
never changes; a kept slot that another drawing has taken since is not trusted.

**Number.** `n` is the entity handle of the measured thing when it fits in 32 bits, else the 32-bit FNV-1a of
its hex text: the measured line or polyline of a take-off row, the block insert of an opening, the room's name
text. Each row's `ref` is a letter and that handle in hex (`W1A2`, `O3F`, `C41`, `L52`, `R6B`, `S77`), so the
same thing gets the same `ref` and id every time, including after the take-off is run again. A saved take-off
row without a handle keeps its old position-based `ref`, is not recorded, and the export warns about it.

A rectangular slab (four right angles, straight sides) also carries `length_mm`, `breadth_mm` and
`"rectangular": true`; any other outline carries only `area_m2`. Only measured facts are recorded: no finishes,
rates or costs.

## Tests

`crates/engine/tests/takeoff_record.rs` (eleven tests): the records equal the bridge rows and the header is
`complete`; ids carry kind, slot and number; recording again keeps the ids and one changed wall changes one
record; a wall no longer measured is deleted; refs survive a rebuilt take-off; millimetres for an imperial
drawing; rectangular and L-shaped slabs; two drawings in one database stay apart and each keeps its slot; the
default file, refusal of an entity store and of an unreachable server; an empty drawing writes the header only;
the annotated drawing (a stand-in plotter receives the measured items on `TAKEOFF-ANNOT`, a 1.2 MB PDF is
stored as three chunks that add up, and recording again without it removes them). Unit tests in
`cmd/takeoffrec.rs` cover the contract's slot test vectors, id layout, chunking and the slot probe.

One more test is ignored (it needs the server program): it records to a running `shilpid` three times, once
after erasing a wall (`AAD_SHILPID_ADDR=127.0.0.1:7430 cargo test -p aad-engine --test takeoff_record -- --ignored`,
with `shilpid --bind 127.0.0.1:7430 --data x.vdb`).

## Rows O-CoreOp asked for (done)

After reading a database O-Draft wrote, O-CoreOp asked for four changes (`HANDOFF-O-DRAFT-TAKEOFF.md` section 8). All four are in `bridge.export` and therefore in `takeoff.record`:

- **Rooms** carry `level`, `length_mm` and `breadth_mm`, and `rectangular`: the exact sides of a rectangular `ROOM-RECT` outline, else the sides of the outline's box with `"rectangular": false`. A room with no drawn outline sends its area only and the export warns.
- **Openings** carry `wall_ref`: the measured wall nearest to the opening's insertion point, within half the wall's thickness plus 150 mm. An opening that is on no measured wall gets none and the export warns.
- **Walls** carry `thickness_mm`: 230 mm on `MEASURE-FULLBRICK` and 115 mm on `MEASURE-HALFBRICK`. This is the nominal thickness the layer implies, not a measured value, and the export says so in a warning.
- A ventilator is written as `Window`; `source.drawing` is `(unsaved)` for a drawing that has not been saved.

Checked: `rows_carry_what_o_coreop_needs_to_use_them_without_guessing`; the failure test `a_recording_that_dies_half_way_leaves_the_drawing_recording_and_the_next_one_completes_it`; and `apps/aad-cli/tests/takeoff_pdf.rs`, which uses the real plotter: the PDF is stored in chunks that reassemble to the bytes and SHA-256 the header names, and an ignored test run against a real `shilpid` recorded two drawings with their PDFs.

## Not done

- **PDF underlay and calibration**: built for vector PDFs, see [pdf-underlay.md](pdf-underlay.md); scanned PDFs are not supported. (Old text follows.) O-CoreOp's PDF panel is gone; O-Draft had no PDF underlay or scale
  calibration yet, so a project that only has PDF drawings cannot be measured. That is the next piece of work.
- Not checked by O-CoreOp yet: the database with a real PDF and two drawings (O-Draft checked them itself, section "Rows O-CoreOp asked for").
- The header's `levels` rows also carry `ceiling_mm` and `lintel_bottom_mm` from the bridge file; O-CoreOp ignores them.
- No menu entry yet in the WinUI shell other than the engine registry's (the commands appear under
  Architecture > Take-off from the registry).
- Reading a recorded take-off back into the drawing, and selecting the measured item from its box.
- A check for two drawings that measure the same floor (the double-count risk) is not built on either side.
