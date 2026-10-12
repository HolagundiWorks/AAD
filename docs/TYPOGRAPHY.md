# Typography: font presets, roles and migration

Five coordinated font presets, a role system that the interface and drawings resolve through, and a
migration of a drawing's text styles with a preview. This page separates what exists from what is
planned ([status.md](status.md)).

## How fonts work in this repository (audit)

| Area | Finding |
|---|---|
| Rule | Font files are never committed (CLAUDE.md). They are an optional build and packaging input; code and tests work without them |
| Drawing text | `crates/fonts`: the built-in single-stroke font, or TrueType/OpenType outlines found at run time by file stem or family name (`ttf.rs`), with per-character fallback. `TextStyle.font` holds a file or family name. SHX files are not read (the stroke font or `FONTALT` is used). WOFF2 is not read |
| Interface | `theme::install_fonts`: egui's own font plus one system font (Segoe UI, Arial, SF, DejaVu) |
| Packaging | The MSI and portable zip carried no fonts |

## What is built

- **Registry** (`crates/fonts/src/registry.rs`): the five presets with their Sans, Condensed and
  Mono families, licence and upstream records and the notes about what must be verified before
  bundling; the seven roles (Primary, Condensed, Monospace, Heading, Dimension, Schedule,
  Nameplate) mapped to a face and a weight (400 to 600, nothing lighter than Regular for small
  CAD text); `Typography { preset, overrides }`; `resolve(role)` with a fixed fallback order
  (the role's face, then Sans, Condensed, Mono, then the built-in stroke font) that reports
  `substituted` instead of hiding it; `missing_glyphs(family, text)`.
- **Presets**: Engineering (IBM Plex Sans, Sans Condensed, Mono; the default), Architectural
  Editorial (Source Sans 3, Source Code Pro, optional Source Serif 4; no dedicated condensed face,
  so Condensed uses Source Sans 3 and the preset says so), Multilingual Documentation (Noto Sans,
  Noto Sans Condensed, Noto Sans Mono), Compact Interface (Roboto, Roboto Condensed, Roboto Mono),
  Drafting Utility (Fira Sans, Fira Sans Condensed, Fira Mono). Licences other than IBM Plex are
  marked "verify": each release's licence files are checked before packaging.
- **Where fonts are found**: `ttf.rs` searches `CADCRAFT_FONT_DIR`, `<exe folder>\fonts` and
  `%APPDATA%\AAD\fonts` before the system folders, so fonts placed by the installer or the user win.
- **Interface**: `UiState.font_preset` (saved in the workspace file, an unknown value falls back to
  the default). The preset's Sans is the Proportional font, Mono is the Monospace font (command
  bar, values) and Condensed is the egui family `aad-condensed`; a system font stays behind them for
  missing characters. Changing the preset rebuilds the fonts on the next frame; the layout does not
  move. View > Font Preset and `ui.fonts {preset?}` set and report it.
- **Commands** (`crates/engine/src/cmd/typography.rs`): `fonts.presets` (presets, roles, which
  families are installed), `fonts.resolve {preset}`, `fonts.scan` (the drawing's text styles, where
  each is used, whether its font is installed, the role it most likely has, and the styles whose
  purpose cannot be told), `fonts.migrate {preset, roles?, apply?}` (preview, then apply).
- **Migration rules**: only a text style's `font` changes; height, width factor, obliquing and the
  flags are kept, and no entity is touched. Unclassified styles are skipped until a role is given.
  A font that is not installed is never replaced by a different one: the style is skipped with the
  reason. Characters the new font lacks are warned about. Applying is one undo step.
- **Installer**: set `AAD_FONTS_DIR` to a folder with the `.ttf`/`.otf` files and their licence
  texts and `package.ps1` puts them in `<install folder>\fonts` (MSI and portable zip). It refuses
  a folder without fonts or without a licence file. Without the variable the installer carries no
  fonts and the interface keeps the system font until fonts are added.

Tests: `crates/fonts/src/registry.rs`, `crates/engine/tests/typography.rs`,
`crates/ui-egui/tests/fonts.rs`, `apps/cadcraft/src/workspace.rs`.

## Not built yet

| Item | State |
|---|---|
| The font files themselves | Not in the repository (by rule) and not downloaded: obtaining them from the upstream releases and recording version and hash is a step of the packaging process |
| Font weights as separate faces | The interface uses Regular; Medium and Semibold are not loaded, so heading and nameplate weights are recorded but not yet drawn |
| Role use across the interface | Sans and Mono follow the preset everywhere through egui's families; Condensed is available (`theme::CONDENSED_FAMILY`) but toolbar labels and compact metadata do not use it yet |
| Drawing templates and project snapshots | The typography is not yet part of the project template or manifest, and new drawings still start with the built-in font |
| Migration scope | One open drawing, with undo. Not yet: migrating the project's selected drawings, an application-wide default for new projects, a saved migration report, text-bounds recalculation and overlap detection, block attributes and dimension text beyond the style's font |
| DWG/DXF round trip | Not tested with the new fonts; no check of how other CAD software substitutes them |
| Presets other than Engineering | Defined and selectable, but their fonts are unverified until the files are obtained |
