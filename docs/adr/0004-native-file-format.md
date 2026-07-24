# ADR-0004 — Native file format

- **Status:** Accepted (SQLite-backed `.aadt` via `aadt-store`; DXF remains the exchange format)
- **Context:** Spec §7 / §11 #4 — native format: custom binary vs. SQLite-backed.
  Must handle large plan sets, fast undo/redo, crash recovery, and storage of
  AADT-specific metadata (element classifications, wall graph, takeoff links).

## Decision

Use a **SQLite-backed native format** (via `rusqlite` bundled) in crate
`aadt-store`, file extension `.aadt`.

- **Crash recovery & durability** come from SQLite's WAL journaling rather than
  bespoke code — a major reliability win for a desktop drafting tool.
- **AADT metadata** (classifications, wall graph, takeoff links, cost-code
  mappings) maps naturally to relational tables alongside the entity store.
- The hot in-memory [`Document`](../../crates/aadt-model) stays the working
  representation; SQLite is persistence. OPEN/SAVE choose `.aadt` vs `.dxf` by
  file extension.
- Schema version is stored in a `meta` table (`schema_version = 5` as of
  ADR-0008; older files migrate on open).
- **Drawing history** (time machine) and **takeoff runs** are stored in the same
  `.aadt` file — see [ADR-0008](0008-drawing-history-takeoff-db.md).

## Consequences

- Dependency on SQLite (bundled) in `aadt-store` / CLI only — engine crates stay
  free of it.
- Session undo/redo is not persisted across OPEN (same as DXF); long-term rollback
  uses `HISTORY` checkpoints.
- Entity ids are preserved across `.aadt` round-trips (`Document::restore_entity`).
- External takeoff DBs are optional publish sinks, not the system of record.
## Alternatives considered

- **Custom binary format:** potentially the fastest raw I/O, but we would
  reimplement journaling, crash recovery, and indexed partial loads that SQLite
  already provides correctly.
