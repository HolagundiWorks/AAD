# ShilpiDB layers and AI agents

Guidance for storing O-Draft drawings in `.vdb` so assistants can reason over
spatial hits. Implemented behaviour stays in [status.md](../status.md); this
note is the convention the export payload and future codecs follow.

## Today

`vdb.export` writes one record per model-space entity with a finite bbox:

- **Id** — the entity handle as `u64`
- **Bbox** — `aad_doc::entity_bounds`
- **Payload** — format **v2** binary (`EntityPayload`: layer + geometry for Line,
  Circle, Arc, LwPolyline, Text, MText, Insert; other kinds as stub type name).
  Legacy v1 UTF-8 `handle\ttype\tlayer` remains readable.

`vdb.query` returns ids and decoded labels for hits in a window. Agents should
still use `spatial.*` on the live drawing for geometry detail; `.vdb` is the
durable spatial index / shareable store. Allow-listed agent tools:
`export_shilpidb`, `query_shilpidb`, `import_shilpidb`, `connect_shilpidb`,
`disconnect_shilpidb`, `push_shilpidb`, `pull_shilpidb`.

## Layer names

Prefer the drawing's real layer table (no Autodesk stock names). For agent
workflows, keep discipline layers stable (`A-WALL`, `A-DOOR`, …) so a bbox hit
plus layer is enough to classify an object without opening DXF.

## Planned (not implemented)

- Continuous edit-time sync into `shilpid`
- Replacing DXF/DWG as the primary document format
- Embedding fields beside the spatial grid (ShilpiDB roadmap)
- Upstream ShilpiDB entity schema migration (O-Draft keeps host payloads until that stabilizes)
