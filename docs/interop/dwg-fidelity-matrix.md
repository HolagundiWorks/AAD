# DWG / DXF fidelity matrix

**Status:** Living document · 2026-08  
**Canonical store:** `.aadt` / `.vdb`  
**Exchange:** DXF (primary fidelity) · DWG via [`acadrust`](https://crates.io/crates/acadrust) 2D subset (ADR-0002)

> AADT does **not** claim Autodesk-certified DWG or full Dynamic Block action editing.
> This matrix is what sales and QA should quote.

## Legend

| Mark | Meaning |
|------|---------|
| ✅ | Round-trips or exports usable geometry |
| ⚠️ | Degraded (looks correct for authored state; detail lost) |
| ❌ | Skipped / not modeled |

## Entity & feature matrix

| Feature | DXF in | DXF out | DWG in | DWG out | Notes |
|---------|--------|---------|--------|---------|-------|
| LINE / CIRCLE / ARC / LWPOLYLINE | ✅ | ✅ | ✅ | ✅ | Core 2D path |
| TEXT | ✅ | ✅ | ✅ | ✅ | Formatting sanitized on display |
| INSERT (flat) | ✅ | ✅ | ✅ | ✅ | Missing def → skip + warning |
| Nested INSERT in block def | ✅ | ✅ | ⚠️ | ⚠️ | Kept in model; depth-limited expand (8) |
| HATCH (multi-path) | ✅ | ✅ | ⚠️ | ❌ | DWG write skips hatches (count in diagnostics) |
| DIMENSION | ✅ | ✅ | ⚠️ | ❌ | DWG write skips dims |
| IMAGE / IMAGEDEF | ⚠️ | ✅ | ✅ | ✅ | acadrust `IMAGE`+`IMAGEDEF`+reactor; DXF via `export_path` when images present |
| AADT `FillKind` | n/a | n/a | n/a | n/a | `.aadt` only; inferred from pattern on import |
| AutoCAD Dynamic Block actions | ⚠️ | ⚠️ | ⚠️ | ⚠️ | Import: static look / visibility if detectable; **not editable** |
| AADT Dynamic Elements (`BFLIP`, `WIDTH`, `BVIS`) | ✅ | ✅ | ✅ | ⚠️ | Flip baked as negative `scale_x` on export |
| PNG Elements (`Geometry::Image`) | — | ✅ | ✅ | ✅ | IMAGEDEF on DWG; DXF `export_path` uses acadrust when images exist |

## Diagnostics

### Import
- `ImportReport.warnings` (DXF/DWG open)
- Drawing comments tag when entities skipped: `[DWG import skipped N…]`

### Export (DWG)
- `export_dwg_path` returns warnings for skipped hatches, dims, missing inserts, missing image files
- Warnings also appended to drawing `properties.comments` under `[DWG export]`

## Product rules

1. **Author in `.aadt`** for fills, images, dynamics.
2. **Exchange DWG** for geometry + block refs + raster images that partners can open.
3. **Never promise** AutoCAD Dynamic Block grip editing or hatch-perfect DWG write.
4. Drawings with **both** images and hatches/dims: prefer `.aadt`, or DWG for images + DXF (no images) for annotation.

## See also

- [ADR-0002 DWG/DXF strategy](../adr/0002-dwg-dxf-strategy.md)
- [Market gap case study § Gap F](../audit/2026-08-market-gap-case-study.md)
- Implementation: `crates/aadt-dxf/src/dwg.rs`, `crates/aadt-dxf/src/lib.rs`
