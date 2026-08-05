# UX & command interaction

AAD is **command-first** and intentionally close to AutoCAD muscle memory,
without chasing AutoCAD’s full command count (see [ROADMAP](../ROADMAP.md)
*Curated command band*).

| Document | Purpose |
|----------|---------|
| [autocad-research.md](./autocad-research.md) | Deep dive: how AutoCAD command interaction works, and what AAD adopts |
| [command-workflows.md](./command-workflows.md) | Step-by-step workflows for the daily band, assist/retrieval commands, panel docking, units/dims + parity matrix |
| [bricscad-conformance-audit.md](./bricscad-conformance-audit.md) | Section-by-section conformance against the BricsCAD workflow spec (M9 closed every gap) |
| [Industry audit](../audit/2026-07-industry-standards-audit.md) | Grade vs industry baselines + gap register |
| [Brand guidelines](../branding/README.md) | Orange brand system, assets, UI application |
| [Logo guidelines](../branding/logo-guidelines.md) | Mark construction, clear space, do/don’t |

Code owners:

- Space engine — `crates/aadt-app/src/command_engine.rs`
- Click tools / dyn input — `crates/aadt-app/src/gui.rs`
- Object snaps / overrides — `crates/aadt-app/src/osnap.rs`
- Region selection (fence / polygon / lasso) — `crates/aadt-app/src/selection.rs`
- Panel docking / chrome — `crates/aadt-app/src/gui_chrome.rs`, `prefs.rs`
- Headless commands — `crates/aadt-command`
- Assist / retrieval commands — `crates/aadt-assist`, wired in `crates/aadt-cli/src/commands.rs`
- Units / dim labels — `crates/aadt-model/src/units.rs`
- Dim glyphs — `crates/aadt-render` (`dimension_commands`)

Firm suites (`HCW*`, `BPLT*`, room/text tools) are available but **de-emphasized**
vs the curated daily band in the [roadmap](../ROADMAP.md).
