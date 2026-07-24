# AiADT command workflows

Step-by-step interaction for the daily drafting band. Companion to
[autocad-research.md](./autocad-research.md).

**Global keys**

| Input | Action |
|-------|--------|
| Space / Enter / right-click | Accept typed value, next step, or finish |
| Double-Space | Finish open geometry + repeat previous command |
| Hold Space / Ctrl+Space | Command palette |
| Esc | Cancel tool → Select; clear selection if already idle |
| Tab | Cycle dyn X ↔ Y while drafting a point |
| F1 | Shortcuts sheet |
| F3 | Object snap |
| F8 | Ortho |
| F10 | Polar |
| F11 | Osnap tracking (also enables F3+F10) |

**Selection**

- Click object → select (blue highlight + grips)  
- Shift+click → add/remove  
- Drag L→R → window (enclosed)  
- Drag R→L → crossing (touches)  
- Blocks/components highlight **expanded** geometry  

---

## Draw

### LINE (`L`)

| Step | Prompt | Input |
|------|--------|-------|
| 1 | First point | Click / type coords |
| 2… | Next point or `[U]ndo/[C]lose` | Click, direct distance, `U`, `C`, or Space to finish |

- `U` — undo last vertex (and last committed segment)  
- `C` — close to first point (≥ 3 points)  
- Double-Space — finish and restart LINE  

### POLYLINE (`PL`)

Same vertex pattern as LINE: Space finishes; `U` / `C` supported.

### CIRCLE (`C`)

| Mode | Flow |
|------|------|
| Center (default) | Center → type radius / pick / `[D]`iameter |
| `2P` | Two diameter endpoints |
| `3P` | Three points on the circumference |

### ARC (`A`)

Center → start → end (angles derived from vectors). Catalog-synced for repeat.

### RECTANGLE (`REC`)

| Mode | Flow |
|------|------|
| Corners | First corner → opposite corner |
| Dimensions | `D` → type length → type width → click first corner |
| Relative | After first corner: `@W,H` or `W,H` |

### XLINE (`XL`)

| Mode | Flow |
|------|------|
| Through (default) | Point → through points (repeats) → Space ends |
| Hor / Ver | `H` / `V` → specify a point |
| Ang | `A` → type degrees → specify a point |

### TEXT (`T`)

Insertion point → type value → Space/Enter.

### HATCH (`H`)

Click a closed polyline or circle boundary.

---

## Modify

All of **MOVE / COPY / ROTATE / SCALE / MIRROR / ERASE** support:

- **Noun-verb:** select first, then run the command  
- **Verb-noun:** run command, click objects, **Space** when the set is ready  

### MOVE (`M`)

Select → Space (if verb-noun) → base point → destination.

### COPY (`CO`)

Select → base → destination(s). Empty Space finishes (multi-copy).

### ROTATE (`RO`)

| Path | Flow |
|------|------|
| Angle | Base → **type angle (degrees)** or pick an angle point |
| Reference | Base → `R` → reference angle (2 pts) → new angle (type or 2 pts) |

### SCALE (`SC`)

| Path | Flow |
|------|------|
| Factor | Base → type factor (e.g. `2`) + Space |
| Reference | Base → `R` (or click) → old length (2 pts or type) → new length |

### MIRROR (`MI`)

Select → first axis → second axis → **Erase source?** `[Yes/No] <N>`  
- **N** (default / Space) — keep originals, add mirrored copies  
- **Y** — mirror in place (erase source)  

### OFFSET (`O`)

Type distance + Space → pick object → pick side (repeat) → Space finishes.

### TRIM (`TR`)

Cutting edges (or none = all) → Space → objects to trim → Space finishes.

### EXTEND (`EX`)

Boundary → objects near the end to grow (multi) → Space finishes.

### FILLET (`F`)

Type radius + Space (`0` = sharp) → first line → second line.

### ERASE (`E`)

Select objects → Space confirms erase (or pre-select then `E` / Space).

### STRETCH (`S`)

Requires pre-selection. Window → base → destination.

### Grips

Select an object → click a **grip** (white square) → click new location.  
Moves endpoints, vertices, circle radius/center, insert/text origin, etc.

---

## Annotate / dimensions

### DIM (`DIM` / Align tool)

| Step | Prompt |
|------|--------|
| 1 | First definition point |
| 2 | Second definition point |
| 3 | Dimension line / text location |

Rubber-band shows extensions + dim line. Label uses drawing units (and optional
secondary). Also: `DIMRADIUS`, `DIMDIAMETER`, `DIMANGULAR` (pick / 3-point).

### Continue (`DIMCONTINUE` / `DIMCONT` / `DCO`)

Continues from the **last** aligned/linear dimension (same as AutoCAD DIMCONTINUE):

| Step | Action |
|------|--------|
| 1 | Place a `DIM` (or any aligned chain end) |
| 2 | `DIMCONTINUE` |
| 3 | Click next definition point(s) — dim line offset is preserved |
| 4 | Space / Esc finishes |

Scripted: `DIMCONTINUE x y [x2 y2 …] [layer]`. Each new dim starts at the previous `p2`.

### Quick Dim (`QDIM`)

Select geometry first, then pick the side for the dimension chain:

| Step | Action |
|------|--------|
| 1 | Select wall / opening / column geometry |
| 2 | `QDIM` (or Annotate → Quick Dim) |
| 3 | Click outside the set to choose top / bottom / left / right |

Places consecutive aligned dims between unique stations along that side. Alias: `QUICKDIM`.

### Auto Dim (`AUTODIM`)

Batch-dimension a plan set after isolating structure layers:

| Step | Action |
|------|--------|
| 1 | Select entities (or leave empty for `ALL`) |
| 2 | `AUTODIM` / `AUTODIM SELECTION` / `AUTODIM ALL` |
| 3 | Layers other than walls / windows / doors / structural are turned off |
| 4 | Overall + four-side chain dims written to `AN-DIMS` |
| 5 | `LAYUNISO` or `AUTODIMRESTORE` restores prior layer visibility |

Optional: `AUTODIM … RESTORE` places dims then restores layers immediately.

**Keep-on layer prefixes:** `A-WALL*`, `A-GLAZ*`, `A-WIND*`, `A-DOOR*`, `A-COL*`/`A-COLS*`, `S-COLS`/`S-COL`/`S-BEAM`/`S-JOIST`/`S-WALL`/`S-FNDN`/`S-SLAB`/`S-BRAC`/`S-GRID`, plus dim layers (`AN-DIMS` canonical; aliases `A-ANNO-DIMS`, `A-DIMS`, `S-DIMS`).

Managers: Annotate and Dims panels → Quick Dim / Auto Dim / Restore layers.

### UNITS / DIMSTYLE

| Command | Examples |
|---------|----------|
| `UNITS` | `UNITS` · `UNITS ft-in` · `UNITS m` · `UNITS m SECONDARY on` |
| `DIMSTYLE` | Report or set name, text height, scale, secondary |

Managers: right panel **Units** and **Dims**. Status bar (left) shows the active unit token (`ft-in` / `m`).

| Primary | Model | Typed bare number | Secondary (when on) |
|---------|-------|-------------------|---------------------|
| Feet & inches (`ft-in`) | inches | inches; `12'-6"` ok | metres in `[…]` |
| Decimal meters (`m`) | metres | metres | feet & inches in `[…]` |

### Nameplate template (`TEMPLATE`)

Model-only sheet furniture (no paper space):

| Command | Role |
|---------|------|
| `TEMPLATE LAYERS` | Ensure `TMPL_BORDER`, `TMPL_VIEW`, `TMPL_TEXT` |
| `TEMPLATE FROMBLOCK <name>` | Register block as nameplate (`nameplate_block`) |

Draw border / view frame / `{title}`-style text on those layers, `BLOCK` the geometry, convert, then `INSERT`. Tokens resolve from Drawing Properties when painted. Right panel **Templates**.

### Room labels (`IROOM` / `MROOM` / `IBR` / `MBR` / …)

Room tool (native, from AutoLISP Room Dimension Tool). Format follows drawing `UNITS` (ft-in or metres):

| Command | Role |
|---------|------|
| `IROOM` / `MROOM` | Room-type picker, then two corners |
| `IBR`/`MBR` `IKI`/`MKI` `ILI`/`MLI` … | Named room — two corners |
| `IDO` / `MDO` | Custom name, then two corners |
| `ROOMLABEL name x1 y1 x2 y2` | Non-interactive |
| `IRECT`/`MRECT` · `IHIDERECT`/`MHIDERECT` | Toggle / freeze rectangle layer |
| `ITH`/`MTH` · `ILAYER`/`MLAYER` · `ISET`/`MSET` · `IRESET`/`MRESET` | Settings |
| `IAR`/`MAR` · `MFLOOR` · `MRELABEL` · `IHELP`/`MHELP` | Area label / floor prefix / relabel / help |

Places optional rectangle on `ROOM-RECTANGLES` and centered name + size + area on `ROOM-LABELS`.

### Blocks (`BLOCK` / `WBLOCK` / parametric lite)

| Command | Role |
|---------|------|
| `B` / `BLOCK` | Interactive: select → name → base point (keeps sources) |
| `WBLOCK` | Same, then erase source entities |
| `INSERT name x y` | Place block; seeds param defaults + `VISIBILITY` |
| `BPARAM block key [default] [string\|number]` | Add/update block parameter (e.g. `WNAME`) |
| `BPARAMLIST [block]` | List params / visibility states |
| `BVIS block state [hide i j…]` | Define visibility (hide definition entity indices) |
| `BSETVIS id state` | Set active visibility on an INSERT |
| `BATT id` / `EATTEDIT id key value` | List / edit INSERT attributes |
| `WINLABEL [AT x y] [id…]` | Label INSERTs from `WNAME` (first 2 chars) |

Blocks Manager: create from selection, add params, edit selected INSERT attrs / visibility.

### HCW / BPLT / text tools (HCW-TOOLS)

| Group | Commands |
|-------|----------|
| HCW layers | `HCWLAYERS` `HCWRESET` `HCWPURGE` `HCWAUDIT` `HCWINFO` `HCWLOCK` `HCWUNLOCK` `HCWAUDIT2` `HCWBYBLOCK` `HCWMOVE` `HCWSCHEDULE` `HCWLAYERSTATE` `HCWLEGEND` |
| BPLT (BPAS) | `BPLTSTART` `BPLTLAYERS` `BPLTCOPY` `BPLTCHECK` `BPLTAREA` `BPLTREPORT` `BPLTTITLEBLOCK` |
| Text | `FIXTXT` `FIXTXTH` `TEXTINCREMENT` `TXTALIGN` `TXTDUP` `TXTAUDIT` `TXTEXPORT` `TXTSTYLE` |

`BPLTSTART` switches the drawing to metres and seeds BP-/AP- layers. `WINLABEL` reads `WNAME` on INSERT attributes.

---
## AADT / file (brief)

| Command | Role |
|---------|------|
| `OPEN` / `SAVE` | `.dxf` / `.aadt` (SAVE to `.aadt` auto-checkpoints + persists takeoff run when live) |
| `TAKEOFF` / `TAKEOFF PUBLISH [sink]` | Quantities; publish pins run + optional CSV/JSON/SQL sink |
| `SCHEDULE` / `SCHEDULE REFRESH` | Place linked table / rebuild after geometry edits |
| `HISTORY LIST\|SAVE\|RESTORE\|PURGE` | Drawing time machine (one `.aadt`, many versions) |
| `LISP` / `(…)` | AutoLISP against the live document |

**Workflow — revise without copying files**

1. Draft in a single `.aadt`.
2. `SAVE` (or History → Save version) creates checkpoints.
3. `TAKEOFF` / live SAVE stores quantity runs + Δ vs previous.
4. On a bad edit: History → Restore, or `HISTORY RESTORE <id>`.
5. Title-block Rev text remains for sheets; machine history is the rollback log.

Managers: right panel **Takeoff** and **History**. See [ADR-0008](../adr/0008-drawing-history-takeoff-db.md).

---

## Parity matrix (AutoCAD → AiADT)

Status: **Yes** = daily-usable · **Partial** = useful but gaps · **No** = out of band / later

| AutoCAD expectation | Status | Notes |
|---------------------|--------|-------|
| Enter ≈ Space accept | Yes | + right-click |
| Idle Space repeats | Yes | + Double-Space finish+repeat |
| Keyword `[Options]` | Partial | Core draw/edit covered |
| Noun-verb + verb-noun | Yes | Edit transforms + erase |
| Window / crossing | Yes | L→R / R→L |
| Selection highlight + grips | Yes | Blue glow; inserts expand |
| Dyn input near cursor | Yes | F12 toggle not separate |
| Ortho / Osnap / Polar | Yes | F8 / F3 / F10 |
| LINE Undo/Close | Yes | |
| CIRCLE Diameter | Yes | Also 2P / 3P |
| CIRCLE 2P / 3P | Yes | |
| MIRROR erase source Y/N | Yes | Default N keeps source |
| Grip edit (hot grip) | Yes | Click grip → new location |
| PLINE Arc/Width | No | U/C only |
| RECTANG Dimensions | Yes | |
| XLINE Bisect/Offset | No | Through/H/V/A |
| COPY multi until Enter | Yes | |
| ROTATE typed angle | Yes | |
| ROTATE Reference | Yes | |
| Transform ghost preview | Yes | MOVE/COPY/ROTATE |
| SCALE Reference | Yes | |
| TRIM / EXTEND multi | Yes | |
| DIM 3-point aligned | Yes | Continue/Baseline: Continue Yes |
| DIMCONTINUE chain | Yes | From last aligned/linear |
| QDIM chain | Yes | Select → side pick |
| AUTODIM isolate + set | Yes | `LAYUNISO` / `AUTODIMRESTORE` |
| Alternate units on dims | Yes | Via `dim_secondary` |
| Transparent `'ZOOM` | No | Mode toggles mid-cmd ok |
| Full DIMSTYLE dialog | Partial | Props + managers + cmd |
| Grips edit (stretch grip) | Yes | Hot grip → new point |

---

## Implementation checklist (for contributors)

When changing a workflow:

1. Update `command_engine` catalog steps + finish rule.  
2. Keep `tool_for` / `on_pick` / `commit_dyn` / `tool_prompt` aligned.  
3. Call `engine.accept_point` / `advance_step` / `complete` so Double-Space
   repeat works.  
4. Document the step table in this file.  
5. Prefer keywords with capitalized letters matching the prompt.

Code: `crates/aadt-app/src/command_engine.rs`, `gui.rs`, `gui_chrome.rs`.
