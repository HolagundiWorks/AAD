# ADR-0021 — Merkle versioning inside ShilpiDB (`.vdb` v4)

- **Status:** Accepted
- **Date:** 2026-08-11
- **Supersedes (partially):** ADR-0011 consequence “history stays a `.aadt` feature”
  for the `.vdb` path; ROADMAP M10 “no history in `.vdb`”.
- **Related:** [ADR-0008](0008-drawing-history-takeoff-db.md) (`.aadt` full-blob
  checkpoints — unchanged), [ADR-0011](0011-vector-database-store.md)

## Context

Working on a drawing by copying RevA/RevB files is the pain ShilpiDB should remove.
`.aadt` already has Sheets-style checkpoints (ADR-0008), but each checkpoint stores a
**full document blob**. ADR-0011 left `.vdb` without a time machine. Architecture
already lists “revision history” as owned by ShilpiDB; the on-disk format did not.

## Decision

**Format version 4** of `.vdb` embeds a **content-addressed Merkle store** in the
same file as the working tip:

| Object | Role |
|--------|------|
| `blob` | Deterministic entity bytes ([`encode_entity`](../../crates/aadt-vdb/src/codec.rs)) or document shell / Object-DB section bytes |
| `tree` | Sorted `(entity_id → blob_hash)` + shell hash + objects-section hash |
| `commit` | `tree_hash`, `parent`, timestamp, kind (`auto`/`named`), label, summary |
| `ref` | `HEAD` → tip commit |

- **Hash:** BLAKE3 (32-byte object id). The `blake3` crate is an explicit exception to
  ADR-0011’s “no external crate” rule for this integrity surface only; the format
  remains `unsafe`-free and SQLite/`serde`-free.
- **Working tip** stays materialized at the front of the file (document + spatial
  index + object sections) for fast OPEN — same layout as v3, then a `MERK` trailer.
- **SAVE** auto-commits when the tip tree hash differs from `HEAD`’s tree; identical
  re-SAVE does not create an empty commit. Prior CAS objects are merged across
  rewrites; unchanged entities **reuse** blob hashes (dedup across versions).
- **HISTORY LIST|SAVE|RESTORE|PURGE** on a `.vdb` path operate on Merkle commits.
  `.aadt` HISTORY remains ADR-0008 checkpoints.
- **PURGE** keeps the last 20 `auto` commits (named kept); drops unreachable objects.
- **UNDO** stays session RAM-only (not a Merkle walk).

### GUI

- **History Manager** — egui right-dock tab + WinUI right-rail **Hist** pane
  (Properties | History). LIST/SAVE/RESTORE/DIFF/PURGE via the `HISTORY` command
  for both `.aadt` checkpoints and `.vdb` Merkle commits.
- **Save commit point** creates a named Merkle commit (auto-creates
  `%LOCALAPPDATA%\AADT\untitled.vdb` when no drawing path exists).
- **Selecting a version** runs `HISTORY DIFF` — highlights added/modified entities
  on the tip and ghosts removed geometry from the baseline.

### Out of scope

- Upstreaming CAS into the standalone `shilpidb` engine repo
- Remote sync / multiplayer
- Migrating `.aadt` checkpoints into Merkle

## Consequences

- One `.vdb` file replaces copied revision drawings for rollback/browse.
- v3 files still open; first SAVE upgrades to v4 with a root commit.
- Callers that only need the tip ignore the trailer; HISTORY APIs load the CAS.
