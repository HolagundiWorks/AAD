# AutoCAD interaction model — research notes for AiADT

This document captures how **classic AutoCAD / AutoCAD LT** command interaction
works, and which rules AiADT deliberately mirrors. It is a design reference for
product and implementers — not a license to clone AutoCAD UI chrome or every
command.

Primary sources (Autodesk Help / industry practice):

- [Entering commands](https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-LT/files/GUID-945C8F74-064C-4EA6-99E1-232E6003CE27.htm)
- [Command window prompts & aliases](https://help.autodesk.com/view/ACDLT/2026/ENU/?guid=GUID-BB0C3E79-66AF-4557-9140-D31B4CF3C9CF)
- [Dynamic Input](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-Core/files/GUID-3EBD4C17-F0A5-49FA-B131-4AABE2E727DB.htm)
- [Dimension / alternate units](https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-LT/files/GUID-1B946C9E-0AB1-40E6-8675-80A51851BEA1.htm)
- [Status bar drafting aids](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-Core/files/GUID-E3B34B0A-EA98-45E9-937A-BF5FEF4152DB.htm)
- Noun/verb vs verb/noun selection (`PICKFIRST`) — Autodesk Options + community docs

---

## 1. The command loop (AutoCAD)

AutoCAD’s drafting loop is prompt-driven:

```
Idle → start command (alias / full name / UI)
     → prompts for points, distances, angles, objects, keywords
     → Enter / Space / right-click accepts
     → Esc cancels
     → idle Space / Enter often repeats the last command
```

### 1.1 Enter ≈ Space ≈ right-click “Enter”

At the command prompt, **Enter** and **Spacebar** both:

- Start a typed command
- Accept a typed value
- Accept the **default option** shown in `<angle brackets>`
- Advance to the next step / finish when the prompt allows

Right-click is commonly configured to act as Enter (user preference).

**AiADT mapping:** Space is primary; Enter and right-click call the same
`apply_space_single` path. Double-Space = finish + repeat (AiADT-specific
acceleration on top of AutoCAD’s “press Space when idle to repeat”).

### 1.2 Keyword options in brackets

Prompts look like:

```text
Specify next point or [Close/Undo]:
Specify radius or [Diameter]:
```

Rules users internalize:

1. **Capital letters** in the option name are the shortcut (`C`lose, `U`ndo, `D`iameter).
2. Type the shortcut + Enter/Space — do **not** need the full word (though full words often work).
3. Empty Enter/Space accepts the **default** (often “specify a point”).
4. Options are **modal** to the current step of the current command.

**AiADT mapping:** Dyn-input / floating prompt shows `[U]ndo/[C]lose`,
`[D]iameter`, `[R]eference`, `[H]or/[V]er/[A]ng`, RECTANGLE `[D]imensions`.
Keywords are handled in `commit_dyn` while the matching `Tool` is active.

### 1.3 Aliases (.pgp)

Short aliases (`L`, `C`, `M`, `CO`, `SC`, `TR`, …) resolve to full commands.
Suggestion lists show `C (CIRCLE)`.

**AiADT mapping:** `command_engine` catalog aliases + `tool_for` tokens; Hold-Space
palette fuzzy-matches the same catalog.

### 1.4 Transparent commands

AutoCAD allows `'ZOOM`, `'PAN`, `'OSNAP`, etc. mid-command (apostrophe prefix).
After the transparent command ends, the original resumes.

**AiADT status:** Not implemented as apostrophe-transparent. Modes F3/F8/F10/F11
toggle without ending the tool (partial substitute). True `'ZOOM` mid-LINE is a
future gap.

---

## 2. Selection syntax: noun-verb and verb-noun

| Mode | Sequence | AutoCAD control |
|------|----------|-----------------|
| **Noun-verb** | Select objects → run modify command | `PICKFIRST = 1` (default in modern AutoCAD) |
| **Verb-noun** | Run modify command → select objects → Enter when selection done | Always available for most edits |

Both must feel natural. Experienced users mix them constantly.

Selection mechanics users expect:

- Click = pick nearest object (pickbox)
- L→R window = **enclosed** only
- R→L crossing = **touched**
- Shift = add/remove
- Esc clears selection when idle
- Selected objects **highlight** (historically dashed/blue grips; grips at vertices)

**AiADT mapping:**

- Edit tools (`MOVE`/`COPY`/`ROTATE`/`SCALE`/`MIRROR`/`ERASE`) support **both**
  modes via a `collecting` phase when selection starts empty.
- Window / crossing box selection matches L→R / R→L.
- Selection highlight: blue glow + grips (`paint_selection`); inserts expand
  block geometry so components highlight as wholes.

---

## 3. Dynamic Input (near-cursor UI)

AutoCAD Dynamic Input (F12 / `DYNMODE`) puts:

1. **Pointer input** — typed coordinates near the cursor  
2. **Dimension input** — live length/angle while drafting  
3. **Dynamic prompts** — same options as the command line  

Second points default to **relative polar** (no `@` required); `#` forces absolute.

**AiADT mapping:** Floating dyn box (`dyn_lines` / `paint_dyn_box`) shows prompt,
typed buffer, and live length/angle. Tab cycles X↔Y. Direct distance entry places
the next point along the crosshair from the tool anchor. Absolute `#` prefix is
not required for basic drafting; comma `x,y` and `@W,H` (RECTANGLE) are supported.

---

## 4. Drafting aids (status bar)

| Aid | AutoCAD | Typical shortcut | AiADT |
|-----|---------|------------------|-------|
| Object snap | Endpoint, mid, center, … | F3 | Endpoint / midpoint (F3) |
| Ortho | Lock H/V | F8 | F8 + Shift temporary |
| Polar tracking | Lock to angle increments | F10 | Soft axis constrain (F10) |
| Osnap tracking | Align from acquired snaps | F11 | F11 enables osnap+polar together |
| Dynamic Input | Near-cursor prompts | F12 | Always-on dyn box while drafting |
| Grid / Snap | Grid display / snap spacing | F7 / F9 | Not primary (out of daily band) |

---

## 5. Core draw-command patterns

### 5.1 LINE

```text
LINE
Specify first point:
Specify next point or [Undo]:
Specify next point or [Close/Undo]:
```

- Continuous segments until Enter/Space  
- `U` undoes last segment  
- `C` closes to first point (≥ 2 segments)  
- Relative `@dx,dy` and direct distance are common  

**AiADT:** Same flow with Space finish; `[U]` / `[C]` in dyn input; segments commit
as they are drawn (Undo pops last vertex + document `UNDO`).

### 5.2 CIRCLE

Default: center → radius. Option `D` = diameter instead of radius. Other modes
(2P, 3P, TTR) exist in AutoCAD but are secondary.

**AiADT:** Center → typed radius / pick / `[D]`iameter. 2P/3P/TTR not in band.

### 5.3 POLYLINE

Vertex chain with `[Undo]`, `[Close]`, plus Arc/Width options in full AutoCAD.

**AiADT:** Vertex chain; `[U]` / `[C]`; Space finishes. Arc/Width keywords deferred.

### 5.4 RECTANGLE

Corners, or Dimensions (length/width), often with relative `@W,H`.

**AiADT:** Corner→corner, `D` dimensions workflow, `@W,H`.

### 5.5 XLINE

Construction line through a point; modes Hor / Ver / Ang / Bisect / Offset.

**AiADT:** Through + H / V / A; Space ends. Bisect/Offset modes not implemented.

---

## 6. Modify-command patterns

Typical verb-noun:

```text
MOVE
Select objects: …  (Enter when done)
Specify base point:
Specify second point or <use first point as displacement>:
```

| Command | After selection | AiADT notes |
|---------|-----------------|-------------|
| MOVE | Base → destination | Both selection modes |
| COPY | Base → destinations (repeat until Enter) | Explicit Space finish; multi-copy |
| ROTATE | Base → angle (typed or reference) | Typed degrees + pick supported |
| SCALE | Base → factor **or** Reference | `R` = old length → new length |
| MIRROR | First axis → second axis | Erase source Y/N not yet prompted |
| OFFSET | Distance → object → side (repeat) | Space finishes |
| TRIM | Cutting edges → Enter → objects | Space advances / finishes |
| EXTEND | Boundary → objects | Multi until Space |
| FILLET | Radius → two lines | In catalog + interactive |
| ERASE | Select → Enter | Verb-noun or noun-verb |

---

## 7. Dimensions & units (AutoCAD)

### 7.1 Placing dimensions

Classic linear/aligned:

1. First definition point  
2. Second definition point  
3. **Dimension line location** (offsets the dim line; places text)  

Geometry rendered: extension lines, dimension line, arrowheads/ticks, text
oriented to the dim line. Style controlled by `DIMSTYLE` / variables
(`DIMSCALE`, `DIMTXT`, …).

**AiADT:** `DIM` is 3-point aligned; renderer draws extensions, dim line, arrows,
rotated text; `dim_scale` / `dim_text_height` from drawing properties.

### 7.2 Units vs dim style

Important AutoCAD distinction:

- **Drawing units** (`UNITS`) — how coordinates are interpreted / displayed  
- **Dimension style primary units** — how dim *text* is formatted (can differ!)  
- **Alternate units** (`DIMALT`) — second system in `[brackets]` on the label  

Architectural convention: when format is feet-inches, **1 drawing unit = 1 inch**.

**AiADT (simplified, intentional):**

| Primary units | Model coordinates | Bare typed length | Dim style default |
|---------------|-------------------|-------------------|-------------------|
| `ft-in` | **inches** | inches (`'` / `"` honored) | `ARCHITECTURAL` |
| `m` | **metres** | metres | `METRIC` |

Secondary display toggles the other system in brackets (Units / Dims managers,
`UNITS … SECONDARY on|off`, `dim_secondary` property). Angular dims stay in
degrees and do not show secondary linear units (matches AutoCAD).

---

## 8. What AiADT deliberately does *not* copy

From product principles ([ROADMAP](../ROADMAP.md)):

- Ribbon / toolbar forests and every AutoCAD alias  
- Full DIM* suite (continue, baseline, jogged, …) as GUI chrome  
- Annotative scaling system / paper-space viewports (later)  
- 3D, solids, meshes  
- Script apostrophe-transparent zoom as a requirement for M1  

Depth beyond the daily band goes into **LISP** and headless commands, not more
GUI tools.

---

## 9. Design rules for new AiADT commands

When adding or polishing a command, match AutoCAD unless there is a documented
reason not to:

1. **Prompt ownership** — every step has a clear dyn prompt with `[Options]`.  
2. **Enter/Space/right-click** share accept/finish.  
3. **Esc** cancels to Select and clears the engine.  
4. **Keywords** use the capitalized letter shown in brackets.  
5. **Modify commands** support noun-verb *and* verb-noun.  
6. **Multi-instance** tools (COPY, TRIM objects, OFFSET, EXTEND) finish on
   empty Space, not after the first success.  
7. **Typed values** preferred for distances/angles/factors when muscle memory
   expects them (ROTATE degrees, SCALE factor, CIRCLE radius).  
8. **Engine catalog** stays in sync with the GUI tool (palette + Double-Space
   repeat).  
9. **Selection must highlight** so users trust the set they are about to edit.  
10. Prefer a **curated** command over a half-finished AutoCAD clone.

See [command-workflows.md](./command-workflows.md) for the live step tables and
parity matrix.
