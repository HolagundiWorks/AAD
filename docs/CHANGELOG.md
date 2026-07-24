# Changelog

## Unreleased — HCW developer-guideline audit

Aligned CAD command mutations with HCW Developer Guidelines Track A (transactions / undo, shared defaults, no half-modified drawings):

- **AUTODIM** — isolate + dimensions are one undoable transaction; `RESTORE` skips isolate.
- **DIMCONTINUE** — multi-point chains are one undoable transaction.
- **UNITS / DIMSTYLE / BPLTSTART** — property changes recorded via `Op::SetProperties` inside `transact`.
- **HCWLEGEND / BPLTLAYERS** — layer setup + legend text in one transaction.
- **Save** — drawing properties written inside the same SQLite transaction as geometry.
- **Default dim layer** — canonical `AN-DIMS` (HCW); `A-DIMS` / `A-ANNO-DIMS` still accepted as aliases.
- **Schema migrate** — load/save upgrade via scratch copy + atomic replace (live `.aadt` never left half-migrated).
- **Shared helpers** — `parse_linetype`; named sheet/legend/room constants.
