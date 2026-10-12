# Known issues

Current state only. Detail, evidence and history are in [BUG_BACKLOG.md](BUG_BACKLOG.md) and
[UI_BUG_BACKLOG.md](UI_BUG_BACKLOG.md). Severity: P0 data loss or crash, P1 core drafting blocked,
P2 significant, P3 minor, P4 cosmetic. Confirmed means reproduced; a hypothesis has not been.

## Confirmed and open

| ID | Sev | Summary | Reproduce | Workaround |
|---|---|---|---|---|
| BUG-001 residual | P2 | A damaged DWG can no longer abort the program, but the worker process that reads it has no memory limit, and code that does not call `aad_dwg::isolated::enable()` still reads in-process. Not reported upstream to `acadrust` | Read `apps/aad-cli/tests/fixtures/dwg/alloc-bomb-ac18-section-size.dwg` in-process | Use the apps (they enable the worker); keep DXF copies |
| (limit) | P3 | Opening, saving and plotting block the window, with no progress display. About 0.5 s at 100 000 entities and 5 s at 1 000 000 for the DXF read alone (command-line harness) | Open a large DXF | Wait; the UI-thread cost inside the app is not yet timed (UI-003) |
| (limit) | P3 | Panning inside a layout viewport rebuilds the viewport content every frame: 52 ms at 100 000 entities ([PERFORMANCE.md](PERFORMANCE.md)) | Pan in an MSPACE viewport on a large drawing | Work in model space |
| (limit) | P3 | The display list for a 100 000-entity drawing takes about 125 ms to build on the UI thread on every edit or zoom-band change | Edit a large drawing | none |

## Fixed

BUG-001 (a damaged DWG aborted the program), BUG-002 (stale MSPACE viewport), UI-001 (stall on the
first pick after opening a large drawing), PERF-001 (editing 1 000 objects took 40 ms instead of 5).

## Hypotheses (not reproduced)

- UI-003: file open and save block the UI thread long enough to feel frozen on large files.
- UI-004: `Ctrl+O` is declared by two commands; no misbehaviour seen.
- UI-005: an overly wide Properties panel in a layout tab, seen once, not reproduced.
- The GitHub workflows `cad-ci.yml` and `cad-nightly.yml` have not run on GitHub yet.
- The release installer was installed, run and uninstalled silently on the development machine, but not on a clean machine, and the wizard pages were not exercised ([RELEASE_AND_PACKAGING.md](RELEASE_AND_PACKAGING.md)).
- Neither package ships `NOTICE` (third-party notices); both are unsigned.
- Dependencies are not audited for advisories on this machine (`cargo audit` is run by the nightly
  workflow only).

## Not implemented (by design for now)

Anything listed under "Planned" in [status.md](status.md): macOS, Linux and web builds, the local
language-model runtime in the apps, the remaining hcwCAD-KIT tools, the floor-project wizard,
user coordinate systems, SHX fonts.

## Layer protection and drawing spaces (added with the context bar)

- **Confirmed gap:** objects inside **block definitions** are not covered by the lock and freeze
  check; editing a block's content can change objects on a locked layer inside it
  ([ACTIVE_DRAWING_SPACE.md](ACTIVE_DRAWING_SPACE.md), section 4). Workaround: lock by not editing blocks
  while a protective drawing space is active.
- Layers are referenced by name in drawing spaces. A layer renamed by hand-editing the DXF shows as
  `missing` in the space.
- A saved panel arrangement from builds before the Layout panel was removed or Properties moved is
  not migrated; it falls back to the default arrangement (other preferences are kept).
