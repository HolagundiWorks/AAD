# AADT CAD + ShilpiDB Developer Architecture

- **Version:** 1.0
- **Status:** Design specification (target architecture)
- **See also:** [overview.md](./overview.md) · [ADR-0011 ShilpiDB](../adr/0011-vector-database-store.md) · [ShilpiDB standalone server](../plans/shilpidb-standalone-server.md) · [layers & agents](../standards/shilpidb-layers-and-agents.md)

AADT CAD is a 2D engineering drafting application built around **ShilpiDB**.
Unlike traditional CAD that treats a drawing as a binary file (DWG/DXF), AADT
stores every engineering entity as a structured object inside ShilpiDB.
**ShilpiDB is the source of truth; the CAD application is the editor.**

## Core principles

Geometry-first architecture · metadata-driven editing · local-first workflow ·
entity-based collaboration · zero cloud dependency while editing · spatial query
as a first-class operation · AI-ready object model · no monolithic drawing
document.

## Layered architecture

```
CAD UI
  ↓
Command System
  ↓
Document Controller
  ↓
Transaction Manager
  ↓
ShilpiDB  ──►  Spatial Grid · Geometry Store · Metadata Store
```

### ShilpiDB owns / does not own

| Owns | Does **not** own |
|------|------------------|
| Geometry, layers, blocks | Rendering |
| Object metadata | UI |
| Spatial indexing | Commands |
| Topology, relationships | Mouse input |
| Object IDs | Snaps |
| Transactions, revisions | Toolbars |

## Drawing model

```
Drawing → Project → Building → Level → Zone → Layer → Entity
```

There is **no binary drawing object in memory** — a drawing is a collection of
entities.

## Entity model

Every entity carries: GUID · entity type · layer · bounding box · geometry ·
properties · relationships · revision · visibility · selection state · lock
state · owner · created-by · modified-by · timestamp.

### Geometry types

- **Supported:** line, polyline, circle, arc, ellipse, spline, text, dimension,
  leader, hatch, block, block reference, image reference, construction line, ray,
  viewport.
- **Future:** 3D solid, surface, mesh.

### Metadata (per entity)

Project · building · floor · zone · discipline · category · family · material ·
color · lineweight · linetype · revision · user data · custom properties.

## Spatial index

Every entity automatically updates the **SpatialGrid**. Supported queries:
window, crossing, nearest, radius, intersection, containment, visibility.
Spatial-index updates are **transactional**.

## Layers

`id · name · description · discipline · visibility · lock · color · lineweight ·
linetype · print state · transparency · revision`.

## Block model

```
Block → Definition → Geometry → Attributes → Insertion Points → References
```

A block **reference** stores only transform, scale, rotation, attributes, and a
reference ID — geometry stays shared in the definition.

## Selection pipeline

```
Mouse → Screen coords → View transform → SpatialGrid query
      → Candidate objects → Geometry hit test → Selection
```

## Transactions

```
Begin → Commands → Validation → Commit
      → Spatial index update → Revision update → Undo history
```

Transactions must be **atomic**.

### Undo

Stores operation · object IDs · old values · new values · timestamp · user.
Undo **never stores complete drawings**.

## Command pipeline

```
User command → Selection → Validation → Transaction begin
  → Geometry update → Metadata update → Spatial index update
  → Commit → Refresh view
```

## Collaboration

Editing is **entity-based**. Modes: read · write · annotation · administrator.
Every write acquires a lock.

### Lock hierarchy

```
Project → Building → Level → Zone → Layer → Entity
```

The system always prefers the **smallest** lock. An entity lock stores entity ID
· owner · mode · timestamp · lease · status. Only locked entities are editable.

## Object relationships

Relationships are stored **separately** and are queryable — e.g. *wall contains
door*, *door inside wall*, *beam supports slab*, *window belongs_to room*.

### Topology engine (future)

connected · adjacent · inside · outside · touches · contains · intersects ·
supports · hosted-by.

## Rendering

The renderer **never owns geometry**. It requests visible entities and receives
geometry · style · transform · visibility · selection. The renderer is
**stateless**.

```
Camera → Visible bbox → SpatialGrid query → Visible entities → Render queue → GPU
```

## Import / export

```
DXF → Parser → Entity builder → Metadata builder → ShilpiDB → Spatial index
```

```
ShilpiDB → Entity reader → Exporter → DXF / SVG / PDF / DWG
```

Import/export **communicates only through ShilpiDB** — nothing bypasses it.

## File format

Working format **`.vdb`**: geometry · metadata · spatial index · block
definitions · layer table · revision history · configuration.

## Identity & revisions

- Every entity has a local ID, a GUID, and a revision number. **Object IDs never
  change.**
- Every commit creates a revision: number · timestamp · user · modified objects ·
  reason · previous revision.

## Performance goals

| Operation | Target |
|-----------|--------|
| Entity lookup | < 1 ms |
| Spatial query | < 5 ms |
| Window selection | < 10 ms |
| Pan / zoom | 60 FPS |
| Undo | < 50 ms |
| Open drawing (100k entities) | < 3 s |

## Memory & threading

- **Loaded:** geometry, metadata, spatial grid. **Lazy / on demand:** large
  assets, images.
- **Threads:** UI (input), rendering (GPU), database (ShilpiDB), background
  (import / export / save / spatial rebuild). No UI operation blocks on disk I/O.

## Error recovery

Transaction rollback · autosave · crash recovery · corruption detection ·
revision recovery.

## Future modules

Constraint solver · parametric objects · object templates · plugin SDK ·
distributed ShilpiDB (read replicas). **Not planned:** BIM authoring, multi-user
sync as a product, 3D solids/meshes.

**Already shipped from this list:** semantic search (`aadt-vecdb` — `FIND`,
`ASKDOCS`) and the assist layer (`aadt-assist` — `SUGGEST` / `CHECK` / `ASK` /
`COMPARE` / `RECOMMEND DETAIL`), both local, offline, and gated so they propose
rather than commit ([ADR-0013](../adr/0013-ai-service-layer.md)).

## Design rules

1. ShilpiDB is the single source of truth.
2. Geometry is never duplicated between modules.
3. Every edit is transactional.
4. Every entity is spatially indexed.
5. Every entity has immutable identity (GUID).
6. Rendering is read-only.
7. Import/export communicates only through ShilpiDB.
8. Commands modify entities, never pixels.
9. Collaboration operates on entities, not files.
10. All future features (AI, ERP integration) build on the existing entity
    and metadata model rather than introducing parallel data structures.

---

## Implementation status (spec → today)

Where the current workspace already realises this target, and where it is
aspirational. Crate references are the concrete starting points.

| Area | Status | Today |
|------|--------|-------|
| ShilpiDB store + `.vdb` | **Built** | `aadt-vdb` (ADR-0011): native vector records, on-disk layer table, block/hatch/dim round-trip |
| Spatial index | **Built (subset)** | `aadt-vdb` `SpatialGrid::query_bbox` / `query_point`; `WITHIN` command. *Missing:* nearest / radius / intersection query APIs |
| Transactions + undo | **Built** | `aadt-model` `Document::transact` op-log; exact undo/redo. Ops are per-change (not whole-drawing) as specified |
| Entity model | **Partial** | `aadt-model::Entity { id, layer, space, geometry, attributes }`. *Missing:* GUID, owner/created-by/modified-by, timestamp, per-entity revision, lock state |
| Metadata | **Built (subset)** | HCW catalog (discipline/element/IFC/Uniformat/MasterFormat) + per-object `OBJMETA` (material/fire/phase). *Missing:* project/building/zone/family fields as first-class |
| Layers | **Built (subset)** | `aadt-model::Layer` + HCW 331-layer catalog. *Missing:* per-layer transparency, print state, description, revision |
| Blocks | **Built** | `aadt-model` `Block` definition + `Insert` reference (shared geometry, transform-only refs) |
| Selection pipeline | **Built** | `box_select` / `pick_entity` (SpatialGrid-assisted) → geometry hit test |
| Command pipeline | **Built** | `aadt-command` interpreter; GUI `command_engine` drives it; edits go through `transact` |
| Rendering (read-only) | **Built** | `aadt-render` + GUI paint request visible entities; renderer holds no geometry |
| Import / export | **Built (subset)** | `aadt-dxf` DXF + DWG (DWG export now writes the layer table); PDF via `aadt-pdf`. All import seeds through the `Document` |
| Relationships / topology | **Built (subset)** | `aadt-graph` (M10): persisted typed relations (`ConnectedTo` / `BelongsTo` / `Contains` / `Bounds` / `Supports`) with forward/reverse queries and traversal, carried in `.aadt` / `.vdb` (current schema v10 / format v3); `OBJECTS` / `RELATIONS` surface them. *Missing:* constraint-aware edges and `Supports` population (needs slabs) |
| Semantic / AI services | **Built (subset)** | `aadt-embed` / `aadt-vecdb` (M11) embeddings, `FIND`, BM25 corpus; `aadt-assist` (M12) gated suggestions, revision narration, detail recommendation. *Missing:* ANN index for 100k+ vectors, PDF/photo ingestion |
| Revision model | **Partial** | `.aadt` (SQLite) keeps history/checkpoints; per-entity revision numbers and commit-reason metadata are not yet modelled |
| Collaboration / locks | **Not started** | Entity-lock hierarchy and multi-user modes are future; the [standalone ShilpiDB server](../plans/shilpidb-standalone-server.md) is the enabling plan |
| Error recovery | **Built (subset)** | Autosave + crash-recovery prompt + save-before-exit; durable `.vdb`/`.aadt` writes (temp+rename). *Missing:* corruption detection, revision recovery UI |

### Nearest gaps to close first

1. **Entity identity** — add a stable GUID + audit fields (owner, timestamps,
   revision) to `Entity`, since design rules 5 and 9 and the collaboration model
   all depend on it. This is the load-bearing prerequisite.
2. **Spatial query surface** — extend `SpatialGrid` with nearest / radius /
   intersection to match the selection and topology needs.
3. **Relationship store** — a queryable, separately-stored relationship table
   (rule 10) so AI / ERP features build on it rather than forking the model.
