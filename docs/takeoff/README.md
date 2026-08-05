# Takeoff docs

> **Status:** takeoff was removed with the M3 engine (2026-07) and **rebuilt in
> M13** against the Object DB ([ADR-0014](../adr/0014-object-based-takeoff.md)).
> `TAKEOFF` and `BOQ` are live commands again, measuring typed objects rather
> than raw geometry, with opening deduction driven by the Graph DB. The domain
> research below informed both generations; where it describes the old M3
> pipeline (CSV/XLSX sinks, `SCHEDULE` tables), read it as background rather
> than current behaviour.

| Document | Purpose |
|---|---|
| [Industry BOQ standards](./industry-boq-standards.md) | CSI / UniFormat / export columns |
| [Bluebeam case study](./bluebeam-case-study.md) | What to learn from Revu QTO |
| [ADR-0005](../adr/0005-cost-code-standard.md) | Cost-code decision (revived by M13) |
| [ADR-0014](../adr/0014-object-based-takeoff.md) | Object-based takeoff — the shipping design |
| [Gap closure plan](../plans/2026-07-gap-closure.md) | Completed build order (historical) |
