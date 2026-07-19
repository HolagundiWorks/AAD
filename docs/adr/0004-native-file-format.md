# ADR-0004 — Native file format

- **Status:** Proposed (revisit at M1)
- **Context:** Spec §7 / §11 #4 — native format: custom binary vs. SQLite-backed.
  Must handle large plan sets, fast undo/redo, crash recovery, and storage of
  AADT-specific metadata (element classifications, wall graph, takeoff links).

## Decision

Use a **SQLite-backed native format** (via `rusqlite`), not a custom binary.

- **Crash recovery & durability** come from SQLite's WAL journaling rather than
  bespoke code — a major reliability win for a desktop drafting tool.
- **AADT metadata** (classifications, wall graph, takeoff links, cost-code
  mappings) maps naturally to relational tables alongside the entity store,
  which is exactly the extra data the spec calls out (§7).
- **Partial/lazy loading** of large plan sets is straightforward with indexed
  queries; we avoid parsing an entire custom-binary file to open a drawing.
- The undo/redo transaction model (M1) can align with SQLite transactions.

## Consequences

- A dependency on SQLite (embedded, no server) — small and battle-tested.
- Raw sequential throughput may trail a hand-tuned binary; mitigated by indexing
  and by keeping the hot in-memory `Document` as the working representation, with
  SQLite as the persistence/recovery layer. Benchmark at M1 against the 100k+
  entity target.

## Alternatives considered

- **Custom binary format:** potentially the fastest raw I/O, but we would
  reimplement journaling, crash recovery, and indexed partial loads that SQLite
  already provides correctly.
