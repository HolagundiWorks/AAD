# VastuDB as a standalone database server — implementation draft

- **Status:** Draft (proposal; not yet scheduled)
- **Scope:** Promote VastuDB from an embedded library (`aadt-vdb`, ADR-0011) into
  a **standalone, MongoDB-style database server** — a long-running daemon that
  owns `.vdb` databases and serves concurrent clients over a wire protocol, with
  a shell and a driver. The embedded library stays as the in-process fast path.

## 1. Goal

Today VastuDB is queried **in-process**: a program links `aadt-vdb`, calls
`VastuDb::open(path)`, and runs `query_bbox` / `query_point` against a
`SpatialGrid` held in that program's memory. That is great for one app on one
machine, but there is no way for **several clients** (a desktop app, a CLI, a
script, another service) to share one live dataset, run **server-side spatial
queries** over a large drawing, or treat the database as a **service** you start
once and connect to — the way `mongod` + `mongosh` + drivers work.

This draft designs `vastud` (the daemon), `vastu` (the shell), and
`aadt-vdb-client` (the driver), wrapping the existing engine without changing the
`.vdb` format or the embedded API.

## 2. Principles (inherited from the workspace)

These are hard constraints, not preferences — the server must not regress them:

- **std-only, dependency-free, `unsafe`-forbid.** No `tokio`, no `serde`, no
  external crates. Networking is `std::net::TcpListener` + threads; framing
  reuses the hand-rolled little-endian `Writer` / `Reader` in
  [`aadt-vdb/src/buf.rs`](../../crates/aadt-vdb/src/buf.rs).
- **Local-first, offline-capable.** Binds `127.0.0.1` by default; the embedded
  path keeps working with no server at all.
- **Deterministic core.** Query results and write ordering are reproducible;
  writes go through the existing `Document::transact` op log.
- **Additive.** `.vdb` on disk (v2), the `aadt-vdb` public API, and every
  existing command are unchanged. The server is a new layer above them.

## 3. Concept mapping (MongoDB → VastuDB)

| MongoDB | VastuDB | Backed by (today) |
|---|---|---|
| `mongod` daemon | **`vastud`** daemon | new `aadt-vdb-server` bin |
| `mongosh` shell | **`vastu`** shell | new bin / `aadt` subcommand |
| driver | **`aadt-vdb-client`** (Rust) | new lib crate |
| database | a `.vdb` (one drawing/dataset) | `VastuDb`, `.vdb` file |
| collection | typed section: entities · objects · relations · layers · hatches · dims · audit | `Document`, `ObjectStore`, `Graph`, `AuditLog` |
| document / `_id` | entity (`EntityId`) · engineering object (`ObjectId`) | `aadt-model`, `aadt-object` |
| secondary index | **spatial index** (uniform grid over bboxes) | `SpatialGrid` |
| `find({...})` | spatial (`WITHIN`/`AT`/`NEAR`) + object/graph queries | `query_bbox`, `query_point`, `entities_within`, `Graph::relations_of` |
| wire protocol (`OP_MSG`) | **VDBP** framed binary | reuses `buf.rs` |
| journal | write-ahead op log (Phase C) | new |

## 4. Architecture

```
  clients:  aadt desktop app  ·  vastu shell  ·  aadt --remote  ·  scripts/drivers
                    │
                    │  VDBP  (TCP, length-framed binary — buf.rs codec)
                    ▼
             ┌──────────────────────── vastud ────────────────────────┐
             │  acceptor: TcpListener → thread-per-conn (bounded pool) │
             │  dispatch: opcode → handler                            │
             │  Catalog:  name → Arc<RwLock<Db>>                       │
             │             Db { live: VastuDb-state, path, dirty }     │
             │  durability: snapshot (temp+rename)  →  WAL (Phase C)   │
             └────────────────────────────────────────────────────────┘
                    │
                    ▼
             data dir:  <name>.vdb  files  (+ <name>.wal in Phase C)
```

- **Catalog** maps database names to live, in-memory state guarded by a
  per-database `RwLock` (many concurrent readers, one writer). A small global
  lock guards create/drop.
- **Live state** per database is the same trio the embedded API exposes —
  `Document` + `SpatialGrid` + (`ObjectStore`, `Graph`, `AuditLog`) — kept warm
  so queries never re-read the file.

## 5. Wire protocol — VDBP

Transport is TCP; every message is a length-prefixed frame built with the
existing binary primitives (`u32`, `u64`, `str`, `point`, `raw`), so we get a
compact, versioned, dependency-free protocol for free.

```
frame  = u32 total_len
         u8  opcode
         u32 request_id      // client correlation; server echoes it
         payload…            // opcode-specific, encoded with buf.rs
```

- **Handshake:** `HELLO { proto_version, client }` → `HELLO_OK { proto_version,
  server_version, format_version }`. Version-negotiated like `.vdb`'s
  `FORMAT_VERSION`.
- **Admin:** `PING`, `USE name`, `CREATE name`, `DROP name`, `LIST_DBS`,
  `STATS name`, `CHECKPOINT name`, `SHUTDOWN` (auth-gated).
- **Reads:** `QUERY_BBOX {x1,y1,x2,y2}`, `QUERY_POINT {x,y,tol}`,
  `QUERY_NEAR {x,y,k}`, `GET_ENTITY {id}`, `LIST_LAYERS`, `OBJECTS [filter]`,
  `RELATIONS {object_id | kind}`, `AUDIT`. Reads take the read lock only.
- **Writes:** `INSERT_ENTITY {geom,layer,attrs}`, `UPDATE_ENTITY`,
  `DELETE_ENTITY {id}`, `ADD_HATCH`/`ADD_DIM`, `PROMOTE` (build the Object DB),
  wrapped by `BEGIN` / `COMMIT` / `ABORT` — one server transaction == one
  `Document::transact` unit, so it stays atomic and undoable.
- **Results:** either a single response frame, or a **chunked** stream
  (`RESULT_CHUNK…` + `RESULT_END`) for large query hits, so a window over 100k
  entities streams instead of buffering.
- **Errors:** `ERR { code, message }` — maps `VdbError` plus new server codes
  (unknown db, write conflict, protocol error).

The whole request→response codec is symmetric with `encode_document` /
`decode_document`, so entity payloads on the wire and on disk share one format.

## 6. New crates & layout

- **`crates/aadt-vdb-proto`** — VDBP opcodes, frame read/write, request/response
  types. Depends only on `aadt-vdb` + `aadt-model` + `aadt-geometry`. Shared by
  server and client so they can never drift.
- **`crates/aadt-vdb-server`** — the `vastud` binary: acceptor, catalog,
  dispatch, durability. Depends on `aadt-vdb-proto` + `aadt-vdb`.
- **`crates/aadt-vdb-client`** — blocking Rust driver: `Client::connect(addr)`,
  `.database(name)`, `.query_bbox(...)`, `.insert(...)`, `.transaction(|tx| …)`.
- **`vastu` shell** — a subcommand of the existing `aadt` binary
  (`aadt vastu <addr>`) or a tiny standalone bin, reusing the CLI's REPL plumbing
  in [`shell.rs`](../../crates/aadt-cli/src/shell.rs).

Layering stays acyclic: `proto → {server, client}`; nothing in the CAD core
learns about the network.

## 7. Concurrency, durability, consistency

- **Concurrency:** thread-per-connection bounded by a semaphore (CAD workloads
  are tens of clients, not thousands). Per-database `RwLock` gives parallel reads
  and a single writer — the classic single-writer-per-db model. Async/`mio` is a
  later option only if profiling demands it.
- **Transactions:** a VDBP `BEGIN…COMMIT` maps to one `Document::transact`, which
  already produces an atomic, reversible op log. `ABORT` drops the in-flight
  transaction; nothing half-applied is ever visible.
- **Durability (phased):**
  - *Phase B — snapshot:* on `CHECKPOINT` (or write-then-idle), write the whole
    database with the existing durable **temp-file + rename** path
    (`save_path_with_objects`). Simple; matches the current whole-file model.
  - *Phase C — WAL:* append each committed transaction's op bytes to
    `<name>.wal`; replay on startup; compact into a fresh `.vdb` snapshot
    periodically. This is the `mongod` journal analogue and makes crashes safe.

## 8. App & CLI integration

- **Unchanged default:** the desktop app and `aadt` CLI keep using the embedded
  `aadt-vdb` for local files. No daemon required to draft offline.
- **Optional remote mode:** `aadt --remote host:port` (and a GUI "Connect…"
  action) route `OPEN` / `SAVE` / `WITHIN` / `OBJECTS` / `RELATIONS` to `vastud`
  instead of a local file, via `aadt-vdb-client`. This gives shared datasets and
  server-side spatial queries — the useful half of the removed cloud-collab
  module, but local-first and deterministic.
- Fits the [AI-native platform](../vision/ai-native-platform.md) multi-layer data
  architecture: `vastud` becomes the shared Geometry-DB + Object-DB service the
  other layers (embeddings, retrieval, assist) can query.

## 9. Phased plan

| Phase | Deliverable | Notes |
|---|---|---|
| **A** | `aadt-vdb-proto` + read-only `vastud` + `aadt-vdb-client` + `vastu ping/query` | Framing on `buf.rs`; `HELLO`, `PING`, `USE`, `QUERY_BBOX/POINT`, `GET_ENTITY`, `OBJECTS`, `RELATIONS`. Loads `.vdb` read-only. No writes. |
| **B** | Writes + snapshot durability | `INSERT/UPDATE/DELETE/PROMOTE`, `BEGIN/COMMIT/ABORT`, `CREATE/DROP/CHECKPOINT`, dirty tracking, temp+rename snapshots. |
| **C** | WAL + crash recovery | Append-only op log per db, replay on start, compaction. |
| **D** | App integration | `aadt --remote`, GUI Connect, server-side `WITHIN`, shared datasets. |
| **E** | Hardening | token auth, connection limits/backpressure, admin (`STATS`/`COMPACT`/`SHUTDOWN`), reuse the M9.9 telemetry ideas for server metrics. |

Phase A is a self-contained, testable MVP: a golden test can start `vastud` on an
ephemeral port, `HELLO`, load a fixture `.vdb`, `QUERY_BBOX`, and assert the hit
set matches the embedded `VastuDb::query_bbox` — proving the wire path is
equivalent to the in-process path.

## 10. Non-goals (early)

- Not a general document DB or SQL engine — a purpose-built spatial/CAD object
  store with a small typed query set.
- No sharding/replication — single node until there's a real multi-node need.
- No history/time-machine over the wire (stays a `.aadt` feature, per ADR-0011).

## 11. Risks & trade-offs

- **Thread-per-connection** caps at moderate concurrency; acceptable for CAD, and
  swappable for an event loop later without touching the protocol.
- **Snapshot-before-WAL** means a crash between checkpoints loses the last writes
  in Phase B; Phase C removes that. Ship B, land C before anyone relies on it.
- **Keeping std-only** costs some ergonomics (manual framing, blocking I/O) but
  preserves the workspace's dependency-free, `unsafe`-free guarantee — the whole
  reason VastuDB exists as a custom store in the first place (ADR-0011).

## 12. First commit (if approved)

Scaffold `aadt-vdb-proto` with the frame codec + opcode enum + a round-trip unit
test (encode a `QUERY_BBOX` request and a `RESULT` response through `Writer` /
`Reader` and back), then a read-only `vastud` behind Phase A. Everything else
builds on that seam.
