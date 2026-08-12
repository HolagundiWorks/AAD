# ADR-0008 — Drawing time machine & takeoff DB in `.aadt`

> ✅ **History kept; takeoff tables used by M13.** Drawing checkpoints remain.
> `takeoff_*` run tables in `aadt-store` are written by object-based takeoff
> ([ADR-0014](0014-object-based-takeoff.md)).

- **Status:** Accepted — history + takeoff run persistence
- **Context:** Architectural QS / drafting pain points (revision breaks takeoff;
  RevA/RevB file copies; siloed Excel quantities). ADR-0004 already chose SQLite
  `.aadt` and mentioned takeoff links; session UNDO is RAM-only and title-block
  `revision` text is not machine history.

## Decision

**Local-first version history and takeoff persistence live inside the `.aadt`
SQLite file** (schema **v5**). External databases are optional **publish sinks**,
not the system of record.

### Time machine (Sheets-like)

| Table | Role |
|-------|------|
| `history_checkpoint` | Named / auto / restore_point metadata |
| `history_blob` | Full document snapshot (portable SQLite blob) |
| `history_change` | Short op summary for the log UI |

- Auto-checkpoint on every `SAVE` of `.aadt` (keep last 20 autos; named kept forever).
- Commands: `HISTORY LIST|SAVE|RESTORE|PURGE` (aliases `HIST`, `VERSIONS`).
- GUI: right panel **History Manager** (egui + WinUI) — `.aadt` checkpoints;
  `.vdb` Merkle commits are ADR-0021.
- Title-block `revision` / `revision_history` remain sheet metadata; they are **not**
  conflated with machine checkpoints.

### Takeoff DB + live recompute

| Table | Role |
|-------|------|
| `takeoff_run` | One compute/publish at a `doc_revision` |
| `takeoff_wall` / `_room` / `_opening` / `_structural` | Row quantities |
| `takeoff_delta` | Summary between consecutive runs |

- Property `takeoff_live` (`"1"` default): `SAVE` / `TAKEOFF` persist a run when a
  `.aadt` path is known.
- `TAKEOFF PUBLISH [sink]` marks published and pushes to a sink.
- `SCHEDULE` tags entities (`AADT-SCHEDULE`); `SCHEDULE REFRESH` rebuilds them.

### Sinks (`aadt-takeoff::sink`)

- `none` | `csv:<dir>` | `json:<dir>` | `postgres:<file.sql>` (SQL script for `psql`).
- Prefs / env `AADT_TAKEOFF_SINK`. Drafting never blocks on a live network DB.

### Migration

`load_path` migrates schema v4 → v5 in place (`CREATE TABLE IF NOT EXISTS`).
`save_path` preserves existing history/takeoff tables across rewrite.

## Consequences

- One drawing file replaces copied revision DWGs for rollback/browse.
- Quantities are auditable against `doc_revision` and optional title-block Rev.
- Cloud multiplayer CAD remains out of scope (ADR-0006).

## Alternatives considered

- **Git on DXF:** poor binary/text merge; no takeoff binding.
- **Postgres as SoR:** breaks offline-first product lock.
- **Persist session UNDO:** huge; checkpoints cover long-term rollback.
