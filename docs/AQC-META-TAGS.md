# AADT → AQC / BBS meta tags

**Status:** Canonical contract (AADT writer) · **Updated:** 2026-08-11  
**Parents:** [WINUI-SHELL.md](./WINUI-SHELL.md) · ADR-0009 floors · AQC `aqc-core` takeoff schema

AADT stores takeoff-ready tags on **entity attributes**. New geometry stamps
`LEVEL` + `ELEMENT` from the active floor × category. The Meta Tagger pane and
`OBJMETA` / `AQCMETA` commands set the rest. AQC Estimation / BBS import of these
tags is a later suite wave — this document is the shared field map.

## Commands

| Command | Role |
|---------|------|
| `OBJMETA` / `AQCMETA` / `META` | Report or set keys on the **selection** |
| `EATTEDIT id key value` | Set any attribute on one entity |
| Drafting bar **flr** / **elem** | Active floor × category → layer shim (`A-WALL`, `S-COLS`, …) |

## Attribute keys

| Key | Meaning | AQC peer |
|-----|---------|----------|
| `MARK` | Item mark (`MW1`, `C1`) | `takeoff_item.mark` |
| `WALL_NAME` | Wall name / label | mark / notes |
| `NOS` | Count / nos | quantity count |
| `AQC_CODE` | Rate / item code | `rate_code` / item `code` |
| `COLUMN_NO` | Column number | structural mark |
| `CODE` | Generic element code | `code` |
| `TRADE` | Masonry / Concrete / … | `trade_key` |
| `LEVEL` | Floor name (auto on draw) | `level` |
| `ELEMENT` | Category (Walls / Structural / …) | drafting context |
| `MATERIAL` | Material override | notes / composition |
| `PHASE` | New / Existing / Demolish | phase |
| `FIRERATING` · `IFC` · `UNIFORMAT` · `MASTERFORMAT` | Classification | optional |

## Layer routing

Active **element** category selects the AIA shim layer for new entities:

| Element | Layer shim |
|---------|------------|
| Walls | `A-WALL` |
| Structural | `S-COLS` |
| Lintel | `A-WALL-LINT` |
| BeamLayout | `S-BEAM` |
| Furniture | `A-FURN` |

Move an existing selection into the active space with **CHSPACE** (Props pane).

## Firm branding

Connect `session.json` is licence/hub only. AADT reads `catalog.json` for a
display title and probes `%LocalAppData%\AORMS-Connect\` / `AADT\` for a logo
file, then seeds `DrawingProperties.firm_name` / `logo_path` when empty. Edit via
**Drawing…** on the drafting bar.
