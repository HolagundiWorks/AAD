# AAD + ShilpiDB

How [AADT](https://github.com/HolagundiWorks/AADT) uses
[ShilpiDB](https://github.com/HolagundiWorks/shilpidb) as a spatial vector store.

## Split of responsibilities

| ShilpiDB | AAD |
|---|---|
| Codec (`Reader` / `Writer`), `SpatialGrid`, `.vdb` magic | Drawing model, commands, DXF/DWG, undo |
| `shilpid` server, `shilpi` shell, desktop GUI | `aad-vdb` host format + engine commands |
| Opaque `u64` + bbox + payload bytes | Entity handles, layers, geometry |

## Crate: `aad-vdb`

Pinned to one ShilpiDB git revision so the embedded engine and `shilpi-client`
share a single `Bbox` / codec type.

- **Embedded file** — `VdbStore` / `save_path` / `load_path`: AAD format version 1
  after `MAGIC`, then records (id, bbox, payload), then a persisted `SpatialGrid`.
- **Server** — `VdbClient` wraps `shilpi-client` for a running `shilpid`
  (`127.0.0.1:7420` by default in ShilpiDB).

Payloads are host-defined. The engine's `vdb.export` writes
`handle\ttype\tlayer` UTF-8 so queries can label hits; full geometry remains in
DXF/DWG until a richer round-trip lands.

## Commands

| Id | What it does |
|---|---|
| `vdb.export` | Model-space entity bounds → `.vdb` file |
| `vdb.info` | Record count / format version |
| `vdb.query` | Bbox query against a `.vdb` file |

Programmatic calls never open dialogs; pass `{path}` (and `{window}` for query).

## Layers

`aad-vdb` is a standalone L0 crate (no workspace deps). `aad-engine` (L4) depends
on it. See [ARCHITECTURE.md](../ARCHITECTURE.md) and `cargo xtask layers`.

## Related

- Layer naming for agents: [shilpidb-layers-and-agents.md](../standards/shilpidb-layers-and-agents.md)
- ShilpiDB AORMS plan: upstream `docs/AORMS-INTEGRATION.md`
