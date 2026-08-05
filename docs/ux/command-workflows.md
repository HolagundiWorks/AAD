# AAD command workflows

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

All of **MOVE / COPY / ROTATE / SCALE / MIRROR / ARRAYRECT / ARRAYPOLAR / ARRAYPATH / ERASE** support:

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

### ARRAYRECT (`AR` / `ARRAY`)

Select → Space (if verb-noun) → **rows** → **columns** → **dx** → **dy**.

- Type integers for rows/cols (minimum 1).  
- Spacing: type a number, or pick two points (uses distance).  
- Feed: `ARRAYRECT SELECTION rows cols dx dy` (also `ALL` / id / group).  

### ARRAYPOLAR (`ARRAYPOL`)

Select → **center** → **count** (≥ 2) → **fill angle** (degrees; empty Space = 360) → **Rotate items?** `[Y/N] <Y>`.

- **Y** (default) — rotate each copy about the center.  
- **N** — translate only (orientation unchanged).  

### ARRAYPATH (`APATH`)

Select → pick a **path** (line / polyline / arc / circle) → **count** (≥ 2) → **Align?** `[Y/N] <Y>`.

- Open paths sample from start to end (`i/(count−1)`).  
- Closed paths / circles use `i/count` (no duplicate at the seam).  
- **Y** — rotate items to the path tangent; **N** — translate only.  

### OFFSET (`O`)

Type distance + Space → pick object → pick side (repeat) → Space finishes.

### TRIM (`TR`) — AutoCAD Quick mode

Default **Quick** mode (like AutoCAD `TRIMEXTENDMODE=1`):

1. Start `TRIM` / `TR`
2. Click the **portion to remove** (all drawing entities act as cutting edges)
3. Repeat as needed · **Space** finishes

Options while trimming:

| Input | Action |
|-------|--------|
| `[C]` / `CUTTING` | Standard mode — select cutting edges, Space, then objects |
| **Shift+click** | Extend instead of trim (AutoCAD shift-select) |
| Space | Finish |

Clicking between **two** crossings removes the middle segment and splits the object (AutoCAD behavior).

### EXTEND (`EX`)

Boundary → objects near the end to grow (multi) → Space finishes.

### FILLET (`F`)

Type radius + Space (`0` = sharp) → first line → second line.

### ERASE (`E`)

Select objects → Space confirms erase (or pre-select then `E` / Space).

### STRETCH (`S`)

AutoCAD-style: the **crossing window** is the selection — it both picks the
objects and decides which endpoints move.

| Step | Action |
|------|--------|
| 1 | Specify the crossing window: first corner → opposite corner (click, type `x,y`, or drag). Endpoints inside the window will move; the rest stay put. |
| 2 | **Space** to accept the window (or window again to reselect) |
| 3 | Base point |
| 4 | Destination |

Pre-selected objects skip step 1 (noun-verb). If the pre-selection was a plain
rubber-band box (not individual clicks), that box is replayed as the crossing
window, so it still stretches rather than moves — matching AutoCAD. Any other
pre-selection (individual picks, or a selection that's since changed) moves as
a whole, since there's no window to derive per-endpoint moves from.

**Element filter.** At the crossing-window prompt, type a keyword to limit the
window to a building-element group (toggle on/off; combine several):

| Key | Group | Layers |
|-----|-------|--------|
| `W` | Walls | `*WALL*` (incl. lintels) |
| `N` | Windows | glazing / windows / doors (`A-GLAZ`, `A-WINDOW`, `A-DOOR`) |
| `S` | Structure | `S-*` family (columns, beams, footings) |
| `A` | All | clears the filter |

The **Walls / Windows / Structure** toggles in the top bar show or hide those
same groups; hidden (frozen) layers also drop out of every selection, STRETCH
included.

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
## AAD / file (brief)

| Command | Role |
|---------|------|
| `OPEN` / `SAVE` | `.dxf` / `.dwg` / `.aadt` / `.vdb` (SAVE to `.aadt` auto-checkpoints; `.vdb` also carries the Object DB) |
| `TAKEOFF` / `BOQ` | Quantities and bill of quantities over the Object DB (M13) |
| `HISTORY LIST\|SAVE\|RESTORE\|PURGE` | Drawing time machine (one `.aadt`, many versions) |
| `LISP` / `(…)` | AutoLISP against the live document |

**Workflow — revise without copying files**

1. Draft in a single `.aadt`.
2. `SAVE` (or History → Save version) creates checkpoints.
3. `TAKEOFF` measures the promoted objects and reports what it assumed.
4. On a bad edit: History → Restore, or `HISTORY RESTORE <id>`.
5. Title-block Rev text remains for sheets; machine history is the rollback log.

Managers: right panel **History**. See [ADR-0008](../adr/0008-drawing-history-takeoff-db.md).

---

## Assist & retrieval (AI-native, local)

Everything here is **local and offline**. Retrieval commands cite content that
already exists; assist commands emit proposals that a human accepts. No command
in this section writes geometry on its own ([ADR-0013](../adr/0013-ai-service-layer.md)).

| Command | Role |
|---------|------|
| `FIND <text>` / `FIND SIMILAR <id>` / `FIND DUPES` | Semantic search over labels, geometry, congruent duplicates; sets the selection |
| `ASKDOCS <question>` / `ASKDOCS INDEX <path>` / `ASKDOCS SOURCES` | BM25 retrieval over indexed spec documents; every passage cites source + paragraph |
| `SUGGEST DIMS\|LIST\|WHY id\|ACCEPT id\|REJECT id\|ACCEPTALL` | Proposal surface; only `ACCEPT` mutates, inside one undo transaction with an audit row |
| `CHECK [rule\|RULES]` | Deterministic code/standards rule packs; advisory, cites governing passages |
| `ASK <phrase>` | Routes natural language to a command *string* and shows it — never executes |
| `COMPARE <before-path>` (`DIFF`, `REVCOMPARE`) | Narrates changes from a saved revision to the live drawing |
| `RECOMMEND DETAIL [query]` (`DETAIL`, `DETAILS`) | Cites matching standard-detail sheets from `libraries/details/` |
| `OBJECTS` / `RELATIONS <id>` | Inspect the Object DB and its typed relationships |
| `MAPMETA` | Confidence-gated element → layer/category classification with reconciliation |

### COMPARE — revision narration

| Step | Prompt / action |
|------|-----------------|
| 1 | `COMPARE <before-path>` — any openable drawing (`.dxf` / `.dwg` / `.aadt` / `.vdb`) |
| 2 | Fingerprint diff vs the live document: entities added / removed / unchanged, grouped by layer and kind |
| 3 | Detection deltas appended (walls / rooms / openings / structural / furniture) |
| 4 | With a corpus indexed, each changed layer contributes a cited passage |
| 5 | Registered as one advisory proposal — `SUGGEST LIST` shows it; it can be rejected (audited) but never accepted |

The change set is the deterministic M7 diff; the narration only adds
engineering language around it. `aadt compare <before> <after>` prints the same
note from the CLI.

### RECOMMEND DETAIL — standard-detail recommendation

| Step | Prompt / action |
|------|-----------------|
| 1 | `RECOMMEND DETAIL [query]` — bundled sheets in `libraries/details/` are indexed on first use |
| 2 | With no query, the topic is inferred from the selection's layers (wall / door / window / room) |
| 3 | BM25 returns the best-matching passages, each cited by source and paragraph |
| 4 | Registered as advisory proposals — nothing is inserted into the drawing |

Add project detail sheets by dropping Markdown into `libraries/details/`, or
index anything else with `ASKDOCS INDEX <path>`.

---

## Transparent commands (`'ZOOM` / `'PAN`)

An apostrophe prefix runs a **view** command without cancelling the one already
in progress — AutoCAD's transparent-command convention.

| Step | Action |
|------|--------|
| 1 | Mid-command (say `LINE` waiting for the next point), type `'ZOOM` |
| 2 | Pick a window, or use `A` / `E` (extents), `P` (previous), or a scale factor like `2` / `0.5x` |
| 3 | The view reframes and the interrupted command resumes at exactly the prompt it was on |
| 4 | Esc during the transparent command also returns to the interrupted prompt, not to idle |

`'ZOOM E`, `'ZOOM P`, and `'ZOOM <factor>` complete in one line. Bare `'ZOOM`
waits for a window pick and stays transparent until it gets one.

Only view commands are transparent. Anything that could edit geometry is
refused with a message rather than run non-transparently — silently cancelling
the user's line would be worse than an error.

---

## Panel docking (M9.6)

Each side manager carries its own mode, persisted in `prefs.json`:

| Mode | Behaviour | How to get there |
|------|-----------|------------------|
| Docked | Edge panel; collapses to an icon rail | Pin button, or Dock from a floating panel |
| Floating | Free window, position and size remembered | Float button in the panel header |
| Auto-hide | Edge rail that expands on hover and collapses shortly after the pointer leaves | Unpin button in the panel header |

Opening a manager by command (`LA`, `BLOCKS`, `D`, the units chip, …) reveals it
in whatever mode it is currently in, rather than forcing it back to docked.

---

## Parity matrix (AutoCAD → AAD)

Status: **Yes** = daily-usable · **Partial** = useful but gaps · **No** = out of band / later

| AutoCAD expectation | Status | Notes |
|---------------------|--------|-------|
| Enter ≈ Space accept | Yes | + right-click |
| Idle Space repeats | Yes | + Double-Space finish+repeat |
| Keyword `[Options]` | Partial | Core draw/edit covered |
| Noun-verb + verb-noun | Yes | Edit transforms + erase |
| Window / crossing | Yes | L→R / R→L |
| Fence / WPolygon / CPolygon | Yes | `FENCE` / `WP` / `CP` tools (click corners → Space) |
| Select box window / crossing | Yes | Near-straight drag L→R window, R→L crossing |
| Free-drag curved lasso | Yes | Curved Select drag → `select_in_polygon` (same L→R / R→L modes) |
| Selection highlight + grips | Yes | Blue glow; inserts expand |
| Dyn input near cursor | Yes | F12 / DYN status chip |
| Ortho / Osnap / Polar | Yes | F8 / F3 / F10 |
| Relative `@dx,dy` | Yes | All point tools (anchor or origin) |
| Natural language ASK | Yes | `aadt-assist` pattern router; shows the routed command, never runs it |
| LINE Undo/Close | Yes | |
| CIRCLE Diameter | Yes | Also 2P / 3P |
| CIRCLE 2P / 3P | Yes | |
| MIRROR erase source Y/N | Yes | Default N keeps source |
| Grip edit (hot grip) | Yes | Click grip → new location |
| Multi-mode grips | Yes | Space cycles Stretch → Move → Rotate → Scale → Mirror on a hot grip (M9.3) |
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
| Transparent `'ZOOM` | Yes | `'ZOOM` / `'PAN` suspend and resume the running command |
| Full DIMSTYLE dialog | Partial | Props + managers + cmd |
| Auto-type `DIM` | Yes | `DIM <id>` infers aligned / radius / diameter; GUI hover-to-auto-dim (M9.4) |
| Osnap overrides + menu | Yes | `END/MID/INT/CEN/QUA/NEA/PER/TAN` keywords, SHIFT+right-click menu (M9.1) |
| Live hatch preview | Yes | Internal-point pick with translucent preview (M9.5) |
| Floating / auto-hide panels | Yes | Per-side `PanelMode`, persisted (M9.6) |

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
