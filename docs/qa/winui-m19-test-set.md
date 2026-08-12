# WinUI shipping shell — test set (M18 / M19)

Headless automation lives in `crates/aadt-ffi` (`cargo test -p aadt-ffi --lib`).
This file is the **manual** smoke set for the native WinUI app. Run after
`scripts/full-build-and-test.ps1`.

## Prerequisites

- VS 2022 + Windows App SDK 1.6
- `AadWinui.exe` at `native/aad-winui/winui/x64/Debug/AadWinui/`
- Matching `aadt_ffi.dll` (ABI ≥ 6) beside the exe

Launch:

```powershell
.\scripts\full-build-and-test.ps1 -Launch
```

Or open `native/aad-winui/winui/AadWinui.sln` → Debug | x64 → F5.

---

## Automated (must pass before manual)

| # | Command | Expect |
|---|---------|--------|
| A1 | `cargo test -p aadt-ffi --lib` | All green (incl. `m19_*`) |
| A2 | `cargo test --workspace --lib` | All green |
| A3 | MSBuild `AadWinui.sln` Debug\|x64 | `AadWinui.exe` produced |

---

## Manual — chrome & file IO

| # | Steps | Pass |
|---|-------|------|
| C1 | App opens; demo rectangle + circle visible on paper | ☐ |
| C2 | Status shows `WinUI (M19)` and entity count > 0 | ☐ |
| C3 | **New** clears paper; status ents → 0 | ☐ |
| C4 | **Open** → pick a `.dxf`; geometry appears; fit view | ☐ |
| C5 | **Save** → write `.dxf`; reopen in another CAD or AAD | ☐ |
| C6 | **Undo** / **Redo** after a LINE | ☐ |

## Manual — Layers / Blocks

| # | Steps | Pass |
|---|-------|------|
| L1 | Open left **Lyr** rail; layer list non-empty | ☐ |
| L2 | Double-click a layer → status `lyr` updates | ☐ |
| L3 | Select layer → **Off** / **On**; geometry hide/show | ☐ |
| L4 | Open **Blk**; if empty, create via CLI `BLOCK …` then list refreshes | ☐ |
| L5 | Select block → **Place** → click paper → INSERT appears | ☐ |

## Manual — draw tools

| # | Steps | Pass |
|---|-------|------|
| D1 | **Line** — two clicks; segment drawn | ☐ |
| D2 | **Pline** — 3+ clicks; **Space** finishes | ☐ |
| D3 | **Rec** / **Circle** / **Arc** / **Xline** | ☐ |
| D4 | **Hatch** — click inside closed rect; fill appears | ☐ |
| D5 | **Text** — click → type `ROOM` + Enter; glyph outlines visible | ☐ |
| D6 | Near endpoint while drawing LINE — snap cross appears / sticks | ☐ |

## Manual — modify & selection

| # | Steps | Pass |
|---|-------|------|
| M1 | Click edge → Properties shows KIND/layer; **Apply** layer change | ☐ |
| M2 | Drag L→R empty→around objects → window select; status `sel` | ☐ |
| M3 | Drag R→L crossing box → crossing select | ☐ |
| M4 | Select object → **Move** → base + dest | ☐ |
| M5 | **Copy** / **Rotate** / **Erase** (with selection) | ☐ |
| M6 | **Offset** — pick entity + side | ☐ |
| M7 | Esc cancels active tool; prompt clears | ☐ |

## Manual — discoverability

| # | Steps | Pass |
|---|-------|------|
| T1 | **Tools…** lists LINE/MOVE/…; click starts tool | ☐ |
| T2 | CLI `LINE` / `PL` / `MOVE` start same tools as strip | ☐ |

## Out of scope (egui only — do not fail WinUI)

Fills manager, Annotate dock, Review/PDF, OTRACK rays, grip edit.

---

## Fixture script (CLI)

Optional headless prep of a drawing for Open tests — run from repo root after
`cargo build -p aadt-cli --release`:

```powershell
.\scripts\make-winui-fixture.ps1
```

Produces `fixtures/winui-smoke.dxf` (rectangle, circle, line).
