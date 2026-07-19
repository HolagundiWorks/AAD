# ADR-0002 — DWG/DXF interoperability strategy

- **Status:** Proposed
- **Context:** Spec §7 / §11 #2 — DWG/DXF round-trip is non-negotiable for
  architects exchanging files. Options: ODA SDK (licensed, high-fidelity) vs.
  LibreDWG (open, lighter). DXF is ASCII; DWG is a reverse-engineered binary.

## Decision

**DXF first, DWG later, behind a stable import/export API.**

- M0 ships a small, dependency-free reader for the ASCII DXF entity types the
  takeoff slice needs (`LINE`, `LWPOLYLINE`) in `aadt-dxf`.
- M1 extends this to the full in-scope DXF entity set and DXF **export**
  (round-trip). At M1 we may adopt the `dxf` crate to avoid hand-maintaining a
  full reader; the public `aadt-dxf` entry points are shaped so this is a
  drop-in swap.
- **DWG** gets its own milestone. It requires either an **ODA SDK** license
  (high fidelity, cost) or a **LibreDWG** FFI binding (open, lower fidelity).
  Decision deferred until DWG is scheduled, so licensing cost is incurred only
  when the feature is built.

## Consequences

- The differentiator (takeoff) is proven without taking on a heavyweight binary
  parser or a license up front.
- Architects can exchange DXF immediately; DWG-native shops wait for the DWG
  milestone. This is acceptable because DXF is a universal interchange format.

## Alternatives considered

- **ODA SDK from day one:** highest fidelity but a licensing/integration cost
  before the product's value is proven.
- **LibreDWG from day one:** open, but adds FFI complexity and fidelity risk to
  M0 for no differentiator benefit.
