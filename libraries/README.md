# Bundled libraries

Runtime assets shipped beside the binary. `aadt-library::default_library_root`
resolves this directory next to the executable, the current directory, or by
walking up from it — so a source checkout and an installed copy behave the same.

| Directory | Contents | Consumed by |
|---|---|---|
| [`blocks/`](blocks/) | Block definitions (DXF / SVG), including the CC0 [open-source plan symbols](blocks/opensource/README.md) | `BLOCKLIB`, `INSERT` |
| `elements/` | Elements catalog (`catalog.json`) plus raster furniture under `furniture/` (PNG / JPEG / WebP) | Elements manager, `IMAGE`, `INSERT` |
| `hatches/` | `.pat` hatch pattern families (brick, block, stone, tile, insulation, metal, floor grids) | Fills manager, `HATCHLIB`, `HATCH` |
| [`details/`](details/) | Standard-detail Markdown sheets indexed into the drawing corpus | `RECOMMEND DETAIL` |
| `fonts/` | Fonts used for drawing text | Text rendering |

Raster entries under `elements/furniture/` are **placed content, not a drawing
surface**: they render and export as `IMAGE` + `IMAGEDEF`, but nothing snaps to
or measures them. Drafted geometry stays vector (see the amended lock in the
[roadmap](../docs/ROADMAP.md#curated-command-band)).

The Windows installer copies this tree verbatim (see
[`installer/`](../installer/README.md)), so adding a file here ships it.
