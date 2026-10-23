# WinUI design tokens

Canonical token system for `apps/aad-winui`. Visual direction: **professional CAD density** with
**Fluent** restraint — graphite/charcoal dark workspace, neutral off-white light workspace, and a
restrained Windows blue accent.

## Resource dictionaries

Merged from `App.xaml`:

| File | Contents |
|---|---|
| `Themes/ColorTokens.xaml` | Semantic brushes + title-bar colours (Light / Default / Dark) |
| `Themes/TypographyTokens.xaml` | Font sizes; default faces Segoe UI / Consolas |
| `Themes/SpacingTokens.xaml` | 4 px scale + common paddings |
| `Themes/ControlTokens.xaml` | Control height, icon button, splitter, radii |
| `Themes/ControlStyles.xaml` | Icon buttons, status/side-panel chrome, AI/CAD channel labels |

Code-built UI uses `Density`. At launch, `ShellFonts.Resolve()` upgrades to **Segoe UI Variable** and
**Cascadia Mono** when those font files are installed (WinUI has no CSS-style `FontFamily` fallbacks).
`ShellTheme.VerifyTokens()` logs any missing semantic brushes to `%TEMP%\aad-winui.log`.

## Colour direction

| Role | Dark | Light |
|---|---|---|
| App / title bar | `#1B1B1B` | `#F3F3F3` |
| Elevated surface | `#2C2C2C` | `#FFFFFF` |
| Accent | `#60CDFF` | `#0078D4` |
| Drawing canvas | `#000000` | `#FFFFFF` (CAD preference; not shell chrome) |
| AI channel edge | Accent blue | Accent blue |
| CAD channel edge | Success green | Success green |

**No Mica/acrylic on floating command chrome or the drawing HWND.** Solid fills keep overlays
readable over the canvas.

## Dual command channels (visual)

| Channel | Cue |
|---|---|
| Fixed AI bar | Top accent border + `AiChannelLabelStyle` (“Ask”) |
| Cursor CAD field | Left green edge + mono type (`CadChannelBrush`) |

Behaviour: [COMMAND_BAR.md](COMMAND_BAR.md).

## Spacing scale

| Token | DIP |
|---|---|
| `SpaceXs` … `SpaceXxl` | 2, 4, 8, 12, 16, 24 |

## Intentionally out of scope

- CAD canvas entity colours (host renderer).
- Drawn icon geometry.
- A permanent large chat panel (would steal drawing space).

## Related

- [WINDOW_SIZING.md](WINDOW_SIZING.md) — content-fit + persisted tool window sizes
- [COMMAND_BAR.md](COMMAND_BAR.md)
- [UI_UX.md](UI_UX.md)
