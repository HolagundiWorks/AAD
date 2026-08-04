# HCW CAD Layer Standard

A discipline-based layer convention (ISO 13567 / AIA CAD Layer Guidelines,
simplified for production) that is the platform's **canonical built-in
catalog**. It is defined in [`crates/aadt-standards/src/catalog.rs`](../../crates/aadt-standards/src/catalog.rs)
as `STANDARD_LAYERS` and seeded by `apply_standard_layers`.

## Discipline prefixes

| Prefix | Discipline |
|--------|------------|
| `A` | Architecture |
| `S` | Structural |
| `C` | Civil |
| `M` | Mechanical (HVAC) |
| `E` | Electrical |
| `P` | Plumbing |
| `F` | Fire Fighting |
| `T` | Telecom / ELV |
| `L` | Landscape |
| `I` | Interior Design |
| `X` | Existing (phase) |
| `D` | Demolition (phase) |
| `G` | General |
| `R` | Survey / Reference |

## Layer format

`X-ELEMENT[-MODIFIER]` — discipline prefix, element, optional modifier
(`A-WALL`, `A-WALL-FIRE`, `S-COLUMN`, `C-CONTOUR-MAJOR`). Colours are ACI
(AutoCAD Color Index); the standard uses traditional CAD **screen colours** that
plot black under a color-dependent plot style, with the pen weight carrying
legibility. Lineweights are millimetres on the AutoCAD/ISO pen ladder
(`0.13`–`0.70` in normal use).

## Rich metadata (BIM / takeoff / AI)

Each layer carries a semantic `ElementType` that is the single source of truth
for the richer metadata an AI-native platform needs, so the ~320-layer catalog
stays maintainable:

| Metadata | Source | Example (`A-WINDOW`) |
|----------|--------|----------------------|
| IFC class | `ElementType::ifc()` | `IfcWindow` |
| Uniformat II | `ElementType::uniformat()` | `B2020` |
| MasterFormat division | `ElementType::masterformat()` | `08` |
| AI recognition tags | `ElementType::ai_tags()` | `window, opening, glazing` |
| Phase | `StandardLayer::phase` | `New` / `Existing` / `Demolish` |
| Plot intent | `StandardLayer::plottable` | `true` (viewports/defpoints `false`) |

Material, fire rating, and storey are genuinely per-instance and are left to the
object model; the layer pins discipline, element, phase, and plot intent. This
schema feeds automated drafting, quantity takeoff, clash detection, BIM
interoperability, and AI recognition without relying solely on geometry.

### Per-object overrides (`OBJMETA`)

Values that vary object to object live on the entity's attribute map, not the
layer. Set or report them on the current selection:

```
SELECT …                              # build a selection
OBJMETA MATERIAL Reinforced Concrete  # set an override on every selected object
OBJMETA FIRERATING 120min
OBJMETA PHASE Existing                # New | Existing | Demolish
OBJMETA                               # report effective metadata for the selection
```

Reporting merges the layer's catalog metadata (discipline, element, IFC class,
phase) with the object's overrides — so a wall on `A-WALL` reads as `IfcWall`
even before any override, and `phase` falls back to the layer's phase until the
object sets its own. Keys: `MATERIAL`, `FIRERATING`, `PHASE`, `IFC`, `UNIFORMAT`,
`MASTERFORMAT`. Writes are undoable.

## Layer sets (visibility presets)

Discipline visibility presets are applied with `LAYER SET <name>` /
`apply_layer_set`:

`CONSTRUCTION`, `STRUCTURAL`, `CIVIL`, `SERVICES`, `ELECTRICAL`, `PLUMBING`,
`FIRE`, `ELV`, `LANDSCAPE`, `INTERIOR`, `DEMOLITION`, `PRESENTATION`, `IMPORT`.

The top-bar **Walls / Windows / Structure** toggles and the STRETCH element
filter operate on layer-name groups drawn from the same convention.

## Export

Emit the whole standard for QA, hand-off, or spec coordination:

```
LAYERREPORT standard.csv    # every column, one row per layer
LAYERREPORT standard.md     # grouped by discipline, one table each
```

Format follows the file extension. Also available programmatically as
`aadt_standards::catalog_csv()` / `catalog_markdown()`.

Draw a key of the layers actually used **in a drawing** (name, count, ACI,
linetype, lineweight, and catalog discipline/element) onto a `G-LEGEND` layer:

```
LAYERLEGEND [x y [height]]    # default origin 0,0 · text height 250
```

## AI agents & ShilpiDB

How the layer semantics and layer sets let AI agents query and act on items in
the `.vdb` vector store — scoping by discipline, matching by element/IFC, and
falling back to embeddings for unlabelled geometry — is covered in
[shilpidb-layers-and-agents.md](./shilpidb-layers-and-agents.md).

## Legacy compatibility

Earlier AIA names (`A-GLAZ`, `A-COLS`, `S-COLS`, `A-ANNO-*`, `E-LITE`,
`M-HVAC-SUPP`, `P-SANR`, …) are retained in the catalog as recognised aliases so
imported drawings, detection maps, and fixtures keep resolving. Prefer the HCW
names (`A-WINDOW`, `A-COLUMN`, `S-COLUMN`, `A-TEXT`, `E-LIGHT`, `M-DUCT`,
`P-SOIL`, …) for new work.

On import, CAD layers are namespaced under `ORG-DWG_LAYER-*` (the `IMPORT` set)
so they never clobber the standard. `ADOPTLAYERS` consolidates the easy wins:
any import whose base name already **is** a standard layer
(`ORG-DWG_LAYER-A-WALL` → `A-WALL`) is moved onto it and the namespaced layer
removed. Undoable. Remaining firm-specific layers stay for manual mapping.
