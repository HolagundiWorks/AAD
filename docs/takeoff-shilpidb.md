# Take-off recorded in ShilpiDB

O-Draft records its take-off in a ShilpiDB database, and O-CoreOp reads it from there. The contract
(version 1) is `docs/TAKEOFF-SHILPIDB.md` in the O-CoreOp repository; the hand-off that asked for this
is `docs/HANDOFF-O-DRAFT-TAKEOFF.md` there. This page says what exists on the O-Draft side.

## Command

`takeoff.record {path?, server?, level?, bearing?}` (Architecture > Take-off > Record in ShilpiDB; also
reachable from scripts, the control channel and MCP).

- It runs the same collection step as `bridge.export` (`cmd/bridge.rs`, `collect`), so a record and the
  bridge file's row are the same object.
- **File** (default): `<drawing>.takeoff.vdb` beside the saved drawing, or `path`. The file is rewritten
  so the database equals the current take-off. A file that is not empty and has no header record is
  refused, so an entity store written by `vdb.export` is never overwritten. Records of a kind this
  version does not write are kept.
- **Server**: `server: "host:port"` of a running `shilpid`. Every record is inserted (an existing id is
  replaced), take-off records that are no longer measured are deleted, then the server saves.
- The result gives counts per kind, `written`, `added`, `changed`, `unchanged`, `deleted`, anything
  `skipped` (two rows with the same entity handle) and the export's warnings.

## Records

| Id | Record |
|---|---|
| `(1 << 56) \| 0` = 72057594037927936 | Header: `format` `hcw-takeoff-shilpi`, `version` 1, `units` `mm`, `source` (`app` `O-Draft`, `drawing`, `drawing_id`, `exported`, `revision_sha256` while the saved file still matches its recorded revision), and the bridge file's `project`, `levels`, `beam_depths_mm`, `opening_schedule` |
| `(kind << 56) \| n` | One row: 3 wall, 4 opening, 5 column, 6 lintel, 7 room, 8 slab. The payload is the bridge row as written; the box is the item's bounds in millimetres (the room's box when one is drawn, else its name text; zeros when it has no geometry) |

`n` is the entity handle of the measured thing: the measured line or polyline of a take-off row, the
block insert of an opening, the room's name text, or the first face line of a committed parametric
wall. Each row's `ref` is a letter and that handle in hex (`W1A2`, `O3F`, `C41`, `L52`, `R6B`,
`S77`), or `PW-<instanceId>` for a parametric wall, so the same thing gets the same `ref` and id
every time, including after the take-off is run again. A saved take-off row without a handle keeps
its old position-based `ref`, is not recorded, and the export warns about it.

Everything is millimetres, whatever the drawing units. A rectangular slab (four right angles, straight
sides) also carries `length_mm`, `breadth_mm` and `"rectangular": true`; any other outline carries only
`area_m2`. Only measured facts are recorded: no finishes, rates or costs.

### Parametric walls

Walls committed with `param.commit` are collected by the same `bridge.collect` step. Linked, current
instances become wall rows with `thickness_mm`, `layer` `A-WALL` and `ref` `PW-<instanceId>`; stale
and detached instances are left out with a warning. Brick take-off walls still omit thickness (they
measure centre lines). Details: [parametric.md](parametric.md). Hand-off for the O-CoreOp team:
[HANDOFF-O-COREOP-PARAMETRIC-TAKEOFF.md](HANDOFF-O-COREOP-PARAMETRIC-TAKEOFF.md).

## Tests

`crates/engine/tests/takeoff_record.rs` (nine tests): the records equal the bridge rows; the header id and
the kind in the top byte; recording again keeps the ids and counts, and one changed wall changes one
record; a wall no longer measured is deleted; refs survive a rebuilt take-off; millimetres for an imperial
drawing; rectangular and L-shaped slabs; the default path, refusal of an entity store and of an unreachable
server; an empty drawing writes the header only.

One more test is ignored (it needs the server program): it records to a running `shilpid` twice and after
erasing a wall (`AAD_SHILPID_ADDR=127.0.0.1:7430 cargo test -p aad-engine --test takeoff_record -- --ignored`).
It was run against the installed `shilpid`, and the file it left was served read-only through
`shilpi-http`, where `GET /records/72057594037927936` returned the header.

## Not done

- **PDF take-off.** O-CoreOp's PDF panel is gone; O-Draft has no PDF underlay or scale calibration yet, so
  a project that only has PDF drawings cannot be measured. That is the next piece of work.
- Reading the recorded take-off back into the drawing (optional in the hand-off).
- `wall_ref` on openings (the wall each opening belongs to) is not written.
- The header's `levels` rows also carry `ceiling_mm` and `lintel_bottom_mm` from the bridge file; the
  contract lists only name, height, slab thickness and beam depth. O-CoreOp's reader has not been run
  against a database O-Draft wrote.
- An annotated image of the measured drawing for O-CoreOp's reports.

Open questions for the owner are in section 7 of the hand-off (one file beside the drawing or a served
database; several drawings per project; who owns the PDF underlay; the annotated drawing).
