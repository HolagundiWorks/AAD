# ADR-0002 — DWG/DXF interoperability strategy

- **Status:** Accepted (DXF first; DWG via open-source Rust parser)
- **Context:** Spec §7 / §11 #2 — DWG/DXF round-trip is non-negotiable for
  architects exchanging files. Options: ODA SDK (licensed) vs. LibreDWG (GPL FFI)
  vs. a pure-Rust open-source DWG stack.

## Decision

**DXF first (hand-written `aadt-dxf`), then DWG via [`acadrust`](https://crates.io/crates/acadrust)
(MPL-2.0, pure Rust, R13–R2018), behind the same public import/export API.**

- ASCII DXF covers the entity set AADT models (including multi-path hatch polylines).
- DWG: `import_dwg_path` / `export_dwg_path` map a supported 2D subset
  (Line, Circle, Arc, LwPolyline, Text, Insert, XLine approx.). Hatches and
  dimensions are DXF-only for now. **No ODA SDK** for the open-source product line.
- LibreDWG FFI remains a fallback if a customer file fails `acadrust`.

## Consequences

- Takeoff proves value on DXF without a license tax.
- DWG shops can open files without Autodesk tooling; fidelity is open-source
  best-effort, not ODA-certified.
- Keep `acadrust` unmodified (MPL-2.0); attribute in NOTICE when distributing.

## Alternatives considered

- **ODA SDK:** highest fidelity, licensing cost — rejected for OSS default.
- **LibreDWG FFI from day one:** GPL/complexity — deferred behind Rust parser.
- **DXF-only forever:** unacceptable for many architect handoffs.
