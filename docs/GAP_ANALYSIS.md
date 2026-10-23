# Gap analysis: documentation against implementation

State as of 2026-10-23. Sources: [status.md](status.md), [KNOWN_ISSUES.md](KNOWN_ISSUES.md),
[UI_BUG_BACKLOG.md](UI_BUG_BACKLOG.md), [PERFORMANCE.md](PERFORMANCE.md). Nothing here is a
claim that a feature works; it lists where the documents and the code disagree or stop.

## Not built (documented as planned)

| Area | Gap |
|---|---|
| Project wizard | Manifest, planning and generation exist as commands; there is no wizard UI, no templates, no opening-schedule reconciliation and no nameplates ([PROJECT_WIZARD.md](PROJECT_WIZARD.md)) |
| Floor projects | Floor selector, Floor View Manager and opening-block generation ([projects.md](projects.md)) |
| Panels | Opening Schedule, Drawing Explorer, Xrefs, Measurement, Project Navigator |
| Layer protection | Edits inside block definitions bypass the lock and freeze check ([ACTIVE_DRAWING_SPACE.md](ACTIVE_DRAWING_SPACE.md)) |
| Agent | Take-off and schedule commands exist but are not in the agent registry |
| File formats | DWG is read, not written; 3D solids are reported, not converted |
| Platforms | macOS, Linux, web |

## Open defects and limits

| Item | Where | Note |
|---|---|---|
| Batch plot / PDF preview / cmdline open | WinUI / host | Menu picker and control `app.open`/`app.save` use workers; batch plot and PDF preview remain synchronous |
| DWG parse 1.2 to 1.5 s | `aad-dwg` worker | Stage with `AAD_DWG_STAGE=1`; speed is mostly `acadrust` + DXF bridge. Windows worker has a ~2 GiB Job Object cap |
| Full-extents display list ~125 ms @ 100k | `aad-render` | LOD + frustum + async band help navigation; full walk stays O(n) |
| Quality gate does not pass on the working tree | `cargo xtask ci` | `cargo fmt --check` fails on uncommitted files of the ShilpiDB work, and clippy reports `manual_range_contains` at `crates/vdb/src/file.rs:146` |

## Documentation drift

- Two shells exist (WinUI default, egui behind `--features egui`); commands reach both through the
  engine, but the assistant UI and the `ui.*` automation methods exist only in egui.
- `UI_BASELINE.md` and `UI_UX.md` describe the egui app; the WinUI shell is covered by `status.md` and `WCAG_UI_AUDIT.md`.

## Performance plan

1. Done: glyph outlines by ear clipping, display list of the heavy drawing from about 1.0–1.6 s to
   about 0.32 s ([PERFORMANCE.md](PERFORMANCE.md)).
2. Done: asynchronous open, save and plot in the WinUI shell (parse and write on a worker, commit on the UI thread).
3. Done (interactive path): WinUI host caches the display list and GPU mesh (pans update uniforms;
   edits use `aad_render::rebuild` in model space); layout viewports use an 8× pan cell + LRU;
   hatch/text LOD and model frustum cull; async zoom-band rebuild; spatial warm-up on list rebuild
   ([PERFORMANCE.md](PERFORMANCE.md)).
4. Done (I/O path): control-channel `app.open`/`app.save` deferred across `pump`; WinUI menu save
   via `SaveFileAsync`; DWG worker Job Object memory cap on Windows.
5. Still open: absolute DWG/DXF CPU (profile with `AAD_DWG_STAGE` / split DXF benches before claiming
   wins); batch plot and PDF preview off the session thread.
