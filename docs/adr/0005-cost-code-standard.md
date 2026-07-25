# ADR-0005 — Cost-code mapping standard

- **Status:** Accepted (CSI MasterFormat built-in; firm overrides)
- **Context:** Spec §5.2 / §11 #5 — cost-code mapping is the differentiator that
  positions AADT against PlanSwift/STACK, not just AutoCAD LT. Which standard(s)
  to support out of the box vs. fully custom per-firm mapping?

## Decision

Ship **CSI MasterFormat** as the built-in cost-code taxonomy, **plus** a
fully custom per-firm mapping layer.

- MasterFormat is the dominant North American construction-spec standard;
  supporting it out of the box lets estimators import AADT quantities directly.
- Mapping is expressed as a rules layer (element type + attributes → cost code)
  stored with the project (see ADR-0004), so firms can override or extend the
  defaults without code changes.
- The export path (Excel/CSV, spec §7) carries cost codes as columns for direct
  import into estimating software.

## Consequences

- One curated standard to maintain initially; additional standards (e.g.
  UniFormat, or regional equivalents) can be added as further built-in rule sets
  later without changing the mapping mechanism.
- Firms not using MasterFormat rely on the custom mapping layer from day one.

## Alternatives considered

- **Custom-only (no built-in standard):** maximum flexibility but no
  out-of-the-box estimator interop — weakens the PlanSwift/STACK positioning.
- **Multiple standards at launch:** more surface area than M3 warrants; defer
  until demand is shown.
