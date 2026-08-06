# ADR-0014 — Quantity takeoff on the Object DB

- **Status:** Accepted
- **Pairs with** [ADR-0005](0005-cost-code-standard.md) (CSI MasterFormat cost
  codes) for BOQ classification.
- **Authorised by** [ADR-0012](0012-ai-native-platform-pivot.md) Phase 4:
  takeoff "on top of objects (not raw geometry)".

## Context

Quantity takeoff distinguishes a drafting tool from a drafting-and-estimating
tool. Measuring **raw geometry** cannot answer the questions a takeoff actually
needs — typed objects, openings, and cost codes can.

Concretely: a wall's *gross* area is `length × height`, which geometry gives you.
Its *net* area is gross minus the doors and windows that pierce it — and a bag of
lines and block inserts has no idea which opening belongs to which wall.
Proximity guesses are not good enough to price from.

Phases 1–3 built the missing structure. Objects carry engineering properties
(`length`, `thickness`, `area`, `perimeter`), and the Graph DB carries
`Door —BelongsTo→ Wall`.

## Decision

Rebuild takeoff as `aadt-takeoff`, reading the **Object DB and Graph DB** rather
than the document's geometry.

- **Opening deduction is a graph lookup.** `host_wall()` follows the
  `BelongsTo` edge. A wall's net area is gross minus the openings the graph
  attributes to it — no proximity heuristic, no guess.
- **An unattributed opening is not deducted.** If the graph records no host wall,
  the opening is measured and scheduled but subtracted from nothing, and the
  command says so. Under-deducting is conservative and visible; guessing a host
  would silently change a priced quantity.
- **Cost codes revive ADR-0005 unchanged.** CSI MasterFormat, mapped by object
  kind then refined by layer name — a drywall partition and a brick wall are both
  `Wall` objects but different trades, and pricing them as one line would be
  wrong.
- **Runs persist in the existing `takeoff_*` tables**, which needed no schema
  change. They key rows by a plain integer id, which is what stable `ObjectId`s
  now provide and geometry never could — a stored row traces back to the object
  it measured.

### What 2D cannot measure, and how it is reported

AADT is a 2D tool (ROADMAP: 2D CAD only — permanent). A plan has no storey height, so wall
height is a `TakeoffConfig` **input**, not a measurement. Openings that carry no
drawn dimensions fall back to configured defaults and are flagged
`measured: false`; `TAKEOFF` reports how many figures were assumed rather than
measured. An estimator can then confirm exactly those, instead of discovering
mid-tender that a number was invented.

## Consequences

- `aadt-store` gains an `aadt-takeoff` dependency for the row-shape bridge,
  consistent with how it already bridges `aadt-object` and `aadt-vecdb`. Layering
  stays acyclic — `aadt-takeoff` does not depend on `aadt-store`.
- Takeoff is only as complete as promotion. Geometry that does not promote to an
  object is not measured — which is honest, and fixable by improving detection
  rather than by heuristics inside the quantity engine.
- Runs are deterministic (objects iterate by id, bill lines sort by cost code),
  so two runs of one drawing produce identical output and revision-to-revision
  diffing is meaningful.

### Implementation note: signed zero

Rust's `Sum for f64` folds from `-0.0`, so summing *no* openings yields `-0.0`,
and `(-0.0).max(0.0)` is still `-0.0`. Arithmetically that is zero, but a bill of
quantities printing `-0.00 m²` reads as a defect and undermines confidence in
every other figure on the page. Quantities are normalised through a `nonneg`
helper, with a test pinning the rendered string.

### Honest limitations

- Volumes assume a prismatic wall of uniform thickness. No lintels, sills, or
  returns.
- Brick count is `volume ÷ nominal brick volume` (230×110×75 mm plus a 10 mm
  joint). It is a modular-brick estimate, not a bond-aware count, and makes no
  wastage allowance.
- Rooms bill floor, ceiling, and wall-finish areas from the room's own perimeter;
  openings are not deducted from *room* finishes, only from walls.
- No pricing. The bill carries quantities and cost codes; rates belong to the
  estimator's system.
