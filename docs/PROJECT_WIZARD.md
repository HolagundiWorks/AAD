# Project Setup Wizard, drawing sets, Xrefs and company profiles

This is the architecture audit and the state of the work for the Project Wizard specification. It
separates what exists from what is planned ([status.md](status.md) holds the overall status).

## Audit: what the repository already has

| Area | Where | State |
|---|---|---|
| Project folder, floor files, layout files, main drawing, manifest `project.aad.json` | `crates/engine/src/cmd/projfile.rs` (`project.setup`, `project.open`) | Implemented, tested. The manifest is untyped JSON (`serde_json::Value`) with floors and layouts keyed by name |
| Xrefs: attach, reload, detach, bind, overlay, relative paths | `cmd/xref.rs` | Implemented, tested. Reads a drawing into a block; no clipping or fading |
| Floors: activate, flags, overlays | `cmd/projfile.rs` (`floor.*`) | Implemented, tested |
| Project data in the drawing (details, floors, heights, schedules) | `cmd/project.rs`, `Drawing::project` | Implemented |
| Schedules and take-off | `cmd/schedule.rs`, `cmd/takeoff.rs` | Implemented; no opening *type* field, generated opening blocks or reconciliation report |
| Sheets and batch plot | `cmd/sheet.rs`, `plot.batch` | Implemented; no title block or nameplate with fields |
| File formats | `crates/io` | DXF read and write; DWG read. The project files AAD writes are DXF |
| Wizard UI, template manager, company profiles, nameplates, recovery | none | Not built (before this work) |

Constraints found: identities in `project.aad.json` are names, not ids (renaming a floor breaks its
links); there is no company concept; no title-block resource exists, so the nameplate has to be built
from existing entity types (blocks with attributes are supported by the drawing model, images are
not); generated drawings are DXF, so the default file extension is `dxf` and `dwg` is accepted in
the naming rules only for when DWG writing exists.

## What is built (step 1 and 2 of the implementation sequence)

`crates/engine/src/projectplan/`, tested in `projectplan/tests.rs` and
`crates/engine/tests/project_plan_cmd.rs`:

- **Typed manifest** (`ProjectConfig`, schema version 1, `project.json`): project, floors, drawing
  sets, floor-by-set matrix, Xref rules, schedules, naming rules, generation status. Unknown or
  missing fields take defaults; a malformed, oversized or newer-schema file is an error message, not
  a crash. Stable ids (`new_id`), display names kept separate from ids and file names.
- **Floors**: heights and elevations are optional values; a missing one is reported, never
  defaulted. `resolve_elevations` derives `Z(n+1) = Z(n) + H(n)` for floors without an elevation,
  keeps a given elevation as it is, and rejects floors that are not above the one below.
- **Planner** (`plan`): expands a configuration into folders, drawings and Xrefs. Drawing ids are
  derived from project, set, floor and kind, so they survive renames and re-planning. File names
  and drawing numbers come from patterns (`{project}`, `{set}`, `{floor}`, `{seq:03}`). Xref rules
  (`host` selector, `target` selector, attach or overlay) are expanded into explicit edges with
  relative paths; nothing is inferred from file names. Checks: unique ids, names and codes, safe
  paths (no `..`, drive letters or reserved device names), file-name collisions, size limits, and
  Xref cycles (Kahn's algorithm, no recursion).
- **Preflight** (`preflight`): read-only report of the root folder and which planned files already
  exist, so generation can refuse to overwrite them silently.
- **Company profiles** (`projectplan::company`): typed profile, validation, `companies/<id>/company.json`
  with atomic writes, listing, load by id (never by name), managed logo asset (PNG, JPEG or SVG up
  to 5 MB), and `nameplate_lines` that leaves empty fields out.
- **Commands**: `project.plan` (validate and preview, writes nothing) and `company.validate`.

## Decisions to confirm

1. **Two manifests.** `project.json` (this schema) and `project.aad.json` (the existing floor
   manifest) describe overlapping things. The proposal is to make `project.json` authoritative and
   have the generator write `project.aad.json` from it for the existing floor commands, then migrate
   those commands to ids.
2. **File format.** DXF now; DWG when the engine can write it.
3. **Nameplate without images.** The logo can be a managed asset and a path in the title block
   attributes, but cannot be drawn in the sheet until the drawing model has an image entity.

## Not built yet

| Step | Item |
|---|---|
| 3 | Wizard UI (six steps, draft persistence, review tree and floor-by-set matrix), template manager (built-in, company, project snapshot; compare and apply updates) |
| 5 | `DrawingGenerator` and `XrefService`: creating the files through the engine, attaching Xrefs after the targets exist, reload and verify, the staged lifecycle `Draft → Validating → Generating → Linking Xrefs → Verifying → Ready`, the recovery log and interrupted-creation detection |
| 6 | Opening schedule types, generated opening blocks, reconciliation of schedule and drawing instances, CSV export |
| 7 | Nameplate template with named fields, binding to company and project data, update policy (future drawings or linked nameplates), linked, overridden and frozen states |
| 8 | Xref maintenance (missing, reload, who references what, repath after a move), project packaging with a profile snapshot |
