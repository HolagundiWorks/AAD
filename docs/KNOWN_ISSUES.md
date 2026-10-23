# Known issues

Current state only. Detail, evidence and history are in [BUG_BACKLOG.md](BUG_BACKLOG.md) and
[UI_BUG_BACKLOG.md](UI_BUG_BACKLOG.md). Severity: P0 data loss or crash, P1 core drafting blocked,
P2 significant, P3 minor, P4 cosmetic. Confirmed means reproduced; a hypothesis has not been.

## Confirmed and open

| ID | Sev | Summary | Reproduce | Workaround |
|---|---|---|---|---|
| BUG-001 residual | P3 | In-process DWG reads (no `aad_dwg::isolated::enable()`) are still uncapped. The apps use the worker with a Windows Job Object memory cap (~2 GiB). Upstream `acadrust` AC18 unchecked allocs are not reported yet | Read the alloc-bomb fixture in-process | Use the apps / isolated worker |
| (limit) | P3 | Command-line `open`/`qsave`, batch plot and PDF preview still run their commit/plot work on the session thread. Menu/picker and control-channel `app.open`/`app.save` use workers (control replies are deferred across `pump`) | Batch plot a large set | Wait |
| (limit) | P3 | A full display-list rebuild of every entity at zoom extents is still about 125 ms at 100 000 entities (post-rayon). Far-zoom LOD, frustum cull when zoomed in, and async band rebuild reduce freezes; they do not change full-extents O(n) ([PERFORMANCE.md](PERFORMANCE.md)) | Open or heavily edit a 100k drawing at extents | none |
| (limit) | P3 | Reading a large DWG takes about 1.2 to 1.5 s in the worker (acadrust + DXF round-trip) before the drawing appears; set `AAD_DWG_STAGE=1` for spawn timing. The window stays responsive in the WinUI shell | Open a large DWG | Wait |

## Fixed

BUG-001 (a damaged DWG aborted the program; Windows worker memory is now Job-capped), BUG-002 (stale
MSPACE viewport), UI-001 (stall on the first pick after opening a large drawing), PERF-001 (editing
1 000 objects took 40 ms instead of 5), layout viewport coarse-pan cache (8× cell + LRU), hatch/text
LOD, WinUI async zoom-band rebuild, control-channel and menu save via workers.

## Hypotheses (not reproduced)

- UI-003: file open and save block the UI thread — largely addressed for WinUI picker and control `app.open`/`app.save`; remaining session-thread cases are listed under Confirmed limits above.
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
