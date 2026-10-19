# O-Draft + ShilpiDB

How [O-Draft](https://github.com/HolagundiWorks/AAD) uses
[ShilpiDB](https://github.com/HolagundiWorks/shilpidb) as a spatial vector store.

DXF/DWG remain the editable drawing source of truth. `.vdb` is the spatial /
agent store (and a shareable index). It is not the sole open/save format.

## Split of responsibilities

| ShilpiDB | O-Draft |
|---|---|
| Codec (`Reader` / `Writer`), `SpatialGrid`, `.vdb` magic | Drawing model, commands, DXF/DWG, undo |
| `shilpid` server, `shilpi` shell, desktop GUI | `aad-vdb` host format + engine commands |
| Opaque `u64` + bbox + payload bytes | Entity handles, layers, geometry |

## Crate: `aad-vdb`

Pinned to one ShilpiDB git revision in [`crates/vdb/Cargo.toml`](../../crates/vdb/Cargo.toml)
so the embedded engine and `shilpi-client` share a single `Bbox` / codec type.

### Bumping the ShilpiDB pin

1. Check out or note the target commit on
   [HolagundiWorks/shilpidb](https://github.com/HolagundiWorks/shilpidb).
2. Set the same `rev = "…"` on both `shilpidb` and `shilpi-client` in
   `crates/vdb/Cargo.toml`.
3. Run `cargo update -p shilpidb -p shilpi-client` (or delete the matching
   entries from `Cargo.lock` and rebuild) so the lockfile matches.
4. Run `cargo test -p aad-vdb` and `cargo test -p aad-engine --lib vdb` (and
   fix any codec / client API breaks before committing).

### Embedded file (AADT host format)

`VdbStore` / `save_path` / `load_path`: after `MAGIC`, a host **format version**
(`FORMAT_VERSION`, currently **2**), then records (id, bbox, payload), then a
persisted `SpatialGrid`. Version **1** remains readable (legacy tab payloads).
`VdbStore::format_version()` reports the version that was loaded (or the current
write version for a newly built store). `vdb.info` / `vdb.import` return that
value.

**Wire sync vs file swap:** AADT’s on-disk host layout (version 2 + grid) is
not the same as `shilpid`’s persistence file (version 1, no grid). Use
`vdb.connect` / `push` / `pull` to talk to a running server. Do not expect an
AADT export file to open as a `shilpid` data file, or the reverse, without a
conversion step.

### Payload (host-owned)

Format **v2** payloads are binary (`aad_vdb::EntityPayload`): layer plus geometry
for Line, Circle, Arc, LwPolyline (vertex-capped), Text/MText (truncated), Insert
(name + transform). Other kinds export as stub type + bbox only. Caps live in
`aad-vdb` (`MAX_RECORDS`, `MAX_PAYLOAD`, vertex/string limits). Import is
best-effort for the kinds above. Model space only; paper layouts are not
exported. Import assigns **new** handles (query ids are export-time handles).

### Server

`VdbClient` wraps `shilpi-client` for a running `shilpid`
(`127.0.0.1:7420` by default). Session remembers the address after
`vdb.connect`; push/pull are explicit (no auto-sync on every edit).
`vdb.push` upserts by id; with `replace: true` it also deletes server ids that
are not in the drawing so the server mirrors model space.

## Commands

| Id | What it does |
|---|---|
| `vdb.export` | Model space → `.vdb` (format v2 payloads) |
| `vdb.import` | `.vdb` → model space (best-effort; one undo step; optional `replace`) |
| `vdb.info` | Record count / **loaded** format version |
| `vdb.query` | Bbox query against a `.vdb` file |
| `vdb.connect` / `vdb.disconnect` | Ping and remember / clear a local `shilpid` address |
| `vdb.push` / `vdb.pull` | Push store to server (`replace?`) / pull into drawing (`replace?`) |
| `vdb.sidecar` | Pref (`enabled?` / `toggle?`): after DXF/DWG save, also write `<stem>.vdb` |
| `vdb.status` | `{connected, addr?, sidecar}` for the shell status chip |

Programmatic calls never open dialogs; pass `{path}` (and `{window}` for query).

## WinUI and agent

WinUI: File > Export / Import ShilpiDB, File > ShilpiDB (Connect / Disconnect /
Push / Pull / Sidecar), Open accepts `.vdb` (import with `replace: true`), and
the status-bar **ShilpiDB** button opens Connect / Push / Pull / Disconnect /
Sidecar. Agent tools: `export_shilpidb`, `query_shilpidb`, `import_shilpidb`,
`connect_shilpidb`, `disconnect_shilpidb`, `push_shilpidb`, `pull_shilpidb`
(see [ai-agent.md](../ai-agent.md)).

## Layers

`aad-vdb` is a standalone L0 crate (no workspace deps). `aad-engine` (L4) depends
on it. See [ARCHITECTURE.md](../ARCHITECTURE.md) and `cargo xtask layers`.

## Related

- Layer naming for agents: [shilpidb-layers-and-agents.md](../standards/shilpidb-layers-and-agents.md)
- ShilpiDB AORMS plan: upstream `docs/AORMS-INTEGRATION.md`
