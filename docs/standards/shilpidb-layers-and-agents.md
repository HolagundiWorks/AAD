# ShilpiDB layers and AI agents

Guidance for storing O-Draft drawings in `.vdb` so assistants can reason over
spatial hits. Implemented behaviour stays in [status.md](../status.md); this
note is the convention the export payload and future codecs follow.

## Today

`vdb.export` writes one record per model-space entity with a finite bbox:

- **Id** — the entity handle as `u64`
- **Bbox** — `aad_doc::entity_bounds`
- **Payload** — UTF-8 `handle\ttype\tlayer` (tab-separated)

`vdb.query` returns those fields for hits in a window. Agents should still use
`spatial.*` on the live drawing for geometry detail; `.vdb` is the durable
spatial index / shareable store.

## Layer names

Prefer the drawing's real layer table (no Autodesk stock names). For agent
workflows, keep discipline layers stable (`A-WALL`, `A-DOOR`, …) so a bbox hit
plus layer is enough to classify an object without opening DXF.

## Planned (not implemented)

- Payload codec with enough geometry to rebuild simple entities
- Live sync from the session into an embedded or `shilpid` store
- Embedding fields beside the spatial grid (ShilpiDB roadmap)
