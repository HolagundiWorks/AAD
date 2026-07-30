# ADR-0011 — Custom vector database store (`.vdb`)

- **Status:** Accepted (an *additional* native format alongside SQLite `.aadt`,
  ADR-0004; DXF/DWG remain the exchange formats)
- **Context:** A CAD drawing *is* vector data — points, lines, arcs, polylines,
  circles, text, block inserts. The request was to **store the drawing as a vector
  database**: a purpose-built store that holds the geometry as native vector
  records with its own spatial index, rather than as opaque text payloads inside
  SQLite rows. `aadt-store`'s `.aadt` (ADR-0004) stores geometry as one text blob
  per entity and has no spatial query capability.

## Decision

Add a **custom, self-contained on-disk vector database**, crate `aadt-vdb`, file
extension **`.vdb`**.

- **Native vector records.** Every entity is a fixed-layout binary record
  (tagged-union geometry + layer + model space + attributes). Layers, blocks,
  hatches, dimensions, floors/spaces, the layer→category map, drawing meta, and
  properties round-trip at the same fidelity as `.aadt`.
- **Built-in spatial index.** A uniform grid over entity bounding boxes is part of
  the file; the drawing's spatial extents are persisted and the bucket structure
  is rebuilt on load. Window/point queries run `O(cells + hits)` instead of a
  linear scan (`SpatialGrid::query_bbox` / `query_point`).
- **Fully custom, dependency-free.** Hand-rolled little-endian binary I/O — no
  SQLite, no `serde`, no external crate; the workspace's `unsafe_code = "forbid"`
  is preserved. Header is `"AADTVDB\0"` + a `u32` version.
- **Durable writes.** Saved to a sibling temp file and renamed, so a failed write
  never truncates an existing database.
- **Used as storage + queried.** `OPEN`/`SAVE` recognize `.vdb` (CLI + app file
  dialogs). The `WITHIN x1 y1 x2 y2` command (aliases `VSELECT`/`VWINDOW`) selects
  entities in a window via the index; LISP `(aadt-within x1 y1 x2 y2)` returns the
  count. `Vdb::open` exposes `query_bbox` / `query_point` / `entities_within`.

## Consequences

- The drawing can be stored as, and queried from, a database whose primary unit is
  a vector record with a spatial index — the literal "drawing as a vector
  database." `.aadt` (SQLite, with history/checkpoints and the embeddings tables
  of ADR-0010) remains the default working format; `.vdb` is a first-class
  alternative you can `SAVE`/`OPEN`.
- `aadt-vdb` depends only on `aadt-model` + `aadt-geometry`; `aadt-cli` and
  `aadt-lisp` gain a dependency on it. Layering stays acyclic.
- The `.vdb` format is versioned (`FORMAT_VERSION`); it does not yet persist the
  drawing time-machine (history checkpoints) — that stays a `.aadt` feature.

## Alternatives considered

- **Keep only SQLite `.aadt`.** Rejected for this task: the ask was explicitly a
  *custom* vector database storing the geometry, with spatial indexing SQLite
  text payloads don't provide.
- **An in-memory spatial index over `.aadt`.** A smaller change (acceleration
  only, no new format); would not make the drawing "stored as a vector database."
- **An embedded key/value or `serde`-based format.** Rejected to keep the format
  fully owned, dependency-free, and `unsafe`-free.
