# AADT WinUI shell (AutoCAD-guide CAD stage + HCW tokens)

Canon: [`esti/docs/esti/DESKTOP-WINUI-UX.md`](../../esti/docs/esti/DESKTOP-WINUI-UX.md) ·
CAD interaction: [`docs/ux/autocad-research.md`](./ux/autocad-research.md) ·
[`docs/ux/command-workflows.md`](./ux/command-workflows.md).

**Visual guide:** AutoCAD 2022 light theme (paper-first). HCW tokens for colour /
radius / suite clock; **sizes and orientation** follow the AutoCAD reference.

## Product

| Field | Value |
|-------|--------|
| Window title | **AADT** |
| Brand | **AADT** — in title bar only (not a tall ribbon brand block) |
| Suite | AORMS · activate only in **AORMS Connect** |
| Accent | Radiant Orange `#FF4F18` |
| Theme | `Themes/HcwTheme.xaml` |
| Density | **1×** |

## Geography (AutoCAD map)

```
Title bar 32 — OS drag · AADT · document
Menu 28 — File · Edit · Draw · Modify · View · Automate · Help
Drafting 32 — firm · flr · elem · trc · Sync · Drawing… · Meta
┌──────────────── PAPER (majority ~85%+) ────────────────┐
│  [-] [Top] [2D]                                         │
│     floating CLI center-bottom                          │
│     dyn card @ cursor                                   │
│     overlays (Layers / Props / Meta / …) from Panels    │
└─────────────────────────────────────────────────────────┘
Status 28 — Model · lyr · Panels · ents… · coords · OS OR PO DY · Licence
Clock BR (compact Viewbox 72) clears status strip
```

| Region | Size | AutoCAD peer |
|--------|------|----------------|
| Title | 32 | Application caption |
| Menu | 28 | Menu bar |
| Drafting | **32** | Floor / space / firm context (AADT) |
| Paper | Remainder (~85%+) | Model viewport |
| Floating CLI | Center-bottom · ~480× ~80 · **radius 0** | Command line |
| Status | **28** · aids **24** | Status bar (state + drafting toggles) |
| Panels | Status flyout + View menu | Layer / Properties / Meta palettes |
| Dyn card | Near pointer · **radius 0** | Dynamic Input (F12) |
| Radius | **0** on all buttons, borders, inputs, overlays | AutoCAD-sharp / AADT law |

**Retired (2026-08):** vertical side tab rails (Layers / Props). Managers open as
overlays from **Panels** (status) · **View** menu · **lyr** chip · drafting bar.

## Layers (ISO 13567 / AIA)

| Control | Behaviour |
|---------|-----------|
| Seed ISO/AIA | `STANDARDS` — HCW catalog (~320 layers, ACI + linetype + ISO mm pens) |
| Layer set | `LAYERSET` presets (CONSTRUCTION, STRUCTURAL, SERVICES, …) |
| Row | `name · C{aci} · Cont/Dash… · {lw mm}` · flags |
| Apply | `LAYER name COLOR n LTYPE … LWEIGHT …` |
| New drawing | Auto-seeds standards + CONSTRUCTION set |

Canon: [`docs/standards/hcw-layer-standard.md`](./standards/hcw-layer-standard.md) · FFI ABI **14**.

## Drafting bar (capacity)

| Left | Center | Right |
|------|--------|-------|
| Firm logo+name · **flr** · **elem** · **trc** | drawing title | **Sync** · Drawing… · Meta |

- **flr / elem / trc:** `FLOOR` · `CATEGORY` · `TRACE OFF|BELOW|ABOVE|BOTH|FLOOR <name>`
- **elem** also sets current layer to the category shim (`A-WALL`, `S-COLS`, …)
- **Sync:** Connect Flush + refresh firm chip / seed drawing props
- **Drawing…:** title plate / firm / logo (`aadt_drawing_props_*`)
- **Meta:** AQC/BBS tags — [`AQC-META-TAGS.md`](./AQC-META-TAGS.md)

## Status strip (capacity)

| Left | Center | Right |
|------|--------|-------|
| Model · **lyr** · **Panels** · **Zoom** · **ZL** | ents / rev | coords · OS · OR · PO · DY · **Calc** · Flush · Licence |

**Panels** flyout: Layers · Elements · Levels · Fills · **Meta** · Properties · Plot ·
LISP · History · Tools catalog. Active underline (2px accent) while any overlay is open.

**Zoom** flyout: Extents · Window · In · Out. **ZL** (view lock, on by default):
REGEN / draws keep the current pan/zoom; use Zoom extents (View menu or Zoom) to
refit. Explicit `'ZOOM E` / window still apply.

**Calc** (status icon · Segoe Fluent): in-window panel — expression `+ − × ÷`,
**m** / **ft·in** toggle (suite peer to esti `FloatingCalculator`).

| Mode | Bare number | `'` | `"` |
|------|-------------|-----|-----|
| Metric | metres | feet | inches |
| Imperial (ft·in) | **inches** | **feet** | **inches** (`10'6"` ok) |

Same imperial typing applies to RECTANGLE / CIRCLE / FILLET / WALL / OFFSET dyn input
when drawing units are **ft-in** (model inches).

## Workflow

1. Eyes stay on paper — chrome is peripheral and thin; panes start **collapsed**.
2. Start command from Draw/Modify menus, Tools catalog (Panels), or CLI.
3. Dyn card (DY / F12) accepts distance / `x,y` / keywords at the cursor.
4. Open managers from Panels / View / chips; in-pane tabs switch Layers↔Elements↔…
5. OSNAP · ORTHO · POLAR · DYN live on the status strip (F3 / F8 / F10 / F12).

## Construction lines

| Control | Tag | Flow |
|---------|-----|------|
| XL | Through | Origin → through points |
| Hor | XLH | One point → horizontal XLINE |
| Ver | XLV | One point → vertical XLINE |

## HCW suite peers (kept, compacted)

- ActionDock collapsed by default (menus cover New/Open/Save).
- AnalogueClock + Pomodoro at BR, **0.57×** visual (Viewbox 72) so paper stays primary.
- Licence chip on status strip — Activate only in Connect.
- **Corner radius 0** everywhere (CAD square chrome — AADT exception to suite 8px).
- CAD status hits stay **24** (AutoCAD peer); Comfortable hits opt-in **35**.

## Accessibility (WCAG 2.2 AA — P0)

| Fix | Detail |
|-----|--------|
| Names | `AutomationProperties.Name` on chips, aids, Panels, pane tabs |
| Contrast | CLI/dyn prompts use **Ink**; accent as 2px bar / underline only |
| Labels | Command / dyn / layer / version fields named (not placeholder-only) |
| Live | `LiveSetting` Assertive on prompts; Polite on status/history/coords/layer |
| Landmark | Paper `LandmarkType=Main` |
| Focus | FocusVisualPrimary on tool/aid/quiet styles |
| State | Aid names include on/off; pane tabs + Panels `ItemStatus=selected` |
| Keyboard | **Esc** cancels tool → closes Tools → closes overlays · **F6** cycles paper ↔ CLI ↔ open panel · **Space = Enter** (AutoCAD peer; literal Space only in free-type fields) |
| Comfort | View → **Comfortable hits** — status **35px** / strip **36** (default CAD **24** / **28**) |
| Dirty | Title bar + window title show `filename *` when the session is dirty |
| Cursor | Fixed **72×72** crosshair (clock Viewbox peer) + **pickbox** (5 DIP); hidden when pointer leaves paper |
| Grips | White filled squares + blue edge (AutoCAD peer); not preview crosses |
| XY axes | Removed (no empty-canvas origin cross) |
| LWT | Status **LWT** chip · stroke pack `lw_mm`+ACI (FFI ABI **13**) · on by default |

Residual: Narrator runtime · high-contrast theme · HCW 44 dock density (managers only).

## Non-goals

Full Segoe Fluent icon glyphs for every aid · AutoCAD InfoCenter · multi-document
file tabs (single document this release) · ProtectedCursor crosshair (needs newer WASDK) ·
side tab rails (removed — use Panels).
