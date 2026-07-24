# UX & command interaction

AiADT is **command-first** and intentionally close to AutoCAD muscle memory,
without chasing AutoCAD’s full command count (see [ROADMAP](../ROADMAP.md)
*Curated command band*).

| Document | Purpose |
|----------|---------|
| [autocad-research.md](./autocad-research.md) | Deep dive: how AutoCAD command interaction works, and what AiADT adopts |
| [command-workflows.md](./command-workflows.md) | Step-by-step workflows for the daily band + units/dims + parity matrix |

Code owners:

- Space engine — `crates/aadt-app/src/command_engine.rs`
- Click tools / dyn input — `crates/aadt-app/src/gui.rs`
- Headless commands — `crates/aadt-command`
- Units / dim labels — `crates/aadt-model/src/units.rs`
- Dim glyphs — `crates/aadt-render` (`dimension_commands`)
