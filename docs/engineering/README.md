# AADT Internal CAD Engineering Handbook

Internal engineering documentation for the CAD substrate — **not** end-user manuals.
Companion to ADRs, the [architecture overview](../architecture/overview.md), and
[UX command workflows](../ux/command-workflows.md).

## How to read

1. Start with [STATUS.md](./STATUS.md) — **Current vs Target vs Gap** for every volume.
2. Read **Volume 2 (Command Engine)** and **Volume 3 (Input & Events)** before
   changing any interactive drafting path.
3. Treat [aadt-shilpidb-architecture.md](../architecture/aadt-shilpidb-architecture.md)
   as a **target** design. Live truth is `Document` in `Session` (see Vol 1 / Vol 5).

## Single write-path rule

```
CLI / API / GUI / tests
        │  line string
        ▼
Session::feed  (aadt-app)
        │
        ├─ LISP? → aadt_lisp
        └─ Interpreter::eval_line → Command::run → Document::transact
```

GUI `Tool` + `CommandEngine` only **collect** points and keywords; they commit by
assembling a command line and calling `app_feed` → `Session::feed`.

## Volumes

Wave 1 delivered command-critical full drafts (1–3, 8, 10) plus outlines.
**Wave 2** deepened volumes 4–7, 9, 11–15 to the same engineering template.

| Vol | Title | Depth | File |
|-----|-------|-------|------|
| 1 | CAD Architecture Overview | Full | [vol-01-architecture.md](./vol-01-architecture.md) |
| 2 | Command Engine | Full | [vol-02-command-engine.md](./vol-02-command-engine.md) |
| 3 | Input & Event System | Full | [vol-03-input-events.md](./vol-03-input-events.md) |
| 4 | Geometry Kernel | Full | [vol-04-geometry-kernel.md](./vol-04-geometry-kernel.md) |
| 5 | Entity Database (ShilpiDB) | Full | [vol-05-entity-database.md](./vol-05-entity-database.md) |
| 6 | Selection Engine | Full | [vol-06-selection-engine.md](./vol-06-selection-engine.md) |
| 7 | Object Snap & Tracking | Full | [vol-07-osnap-tracking.md](./vol-07-osnap-tracking.md) |
| 8 | Modify Command Engine | Full | [vol-08-modify-engine.md](./vol-08-modify-engine.md) |
| 9 | Rendering & Viewports | Full | [vol-09-rendering.md](./vol-09-rendering.md) |
| 10 | Undo/Redo & Transactions | Full | [vol-10-undo-transactions.md](./vol-10-undo-transactions.md) |
| 11 | AI & Semantic Layer | Full | [vol-11-ai-semantic.md](./vol-11-ai-semantic.md) |
| 12 | Plugin/API Architecture | Full | [vol-12-plugin-api.md](./vol-12-plugin-api.md) |
| 13 | Performance & Spatial Indexing | Full | [vol-13-performance-spatial.md](./vol-13-performance-spatial.md) |
| 14 | Testing & Validation | Full | [vol-14-testing.md](./vol-14-testing.md) |
| 15 | Developer Coding Standards | Full | [vol-15-coding-standards.md](./vol-15-coding-standards.md) |

## Related docs

| Doc | Role |
|-----|------|
| [architecture/overview.md](../architecture/overview.md) | Crate layer diagram |
| [ROADMAP.md](../ROADMAP.md) | Curated command band, milestones |
| [ux/command-workflows.md](../ux/command-workflows.md) | Step tables for daily tools |
| [api/README.md](../api/README.md) | Local HTTP API (`POST /v1/command`) |
| [adr/](../adr/) | Decisions (esp. 0009 floors, 0011 ShilpiDB, 0013 Assist, 0015 API) |
