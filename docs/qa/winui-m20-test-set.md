# WinUI shipping shell — test set (M20)

Headless automation: `cargo test -p aadt-ffi --lib` (includes `m19_*` + `m20_*`).
Manual smoke for the native WinUI app after `scripts/full-build-and-test.ps1`.

## Prerequisites

- VS 2022 + Windows App SDK 1.6
- `AadWinui.exe` + `aadt_ffi.dll` (ABI ≥ **7**) beside the exe

```powershell
.\scripts\make-winui-fixture.ps1
.\scripts\full-build-and-test.ps1 -Launch
```

---

## Automated

| # | Command | Expect |
|---|---------|--------|
| A1 | `cargo test -p aadt-ffi --lib` | All green (`m19_*`, `m20_*`) |
| A2 | `cargo test --workspace --lib` | All green |
| A3 | MSBuild `AadWinui.sln` Debug\|x64 | exe + matching DLL |

---

## Manual — command line (M20.1)

| # | Steps | Pass |
|---|-------|------|
| CL1 | Type `LI` — completions list shows LINE | ☐ |
| CL2 | ↑ recalls prior command after Enter | ☐ |
| CL3 | Empty Enter repeats last command | ☐ |
| CL4 | `LINE` / `DIM` / `DOOR` start tools (same as strip) | ☐ |
| CL5 | `LINE 0 0 50 0` feeds geometry (args, not tool) | ☐ |

## Manual — Elements / Fills

| # | Steps | Pass |
|---|-------|------|
| E1 | Left **Elm** shows doc blocks + catalog rows | ☐ |
| E2 | Select element → **Place** → click → INSERT | ☐ |
| E3 | **Fil** lists patterns; pick ANSI31; Hatch fills with pattern | ☐ |
| E4 | Door / Window strip buttons place seeded blocks | ☐ |

## Manual — annotate thin + grips / OTRACK

| # | Steps | Pass |
|---|-------|------|
| AN1 | **Dim** — three clicks → aligned dimension | ☐ |
| AN2 | **MLeader** — 3 clicks → type note + Enter | ☐ |
| AN3 | Select a line — grip squares visible; drag endpoint | ☐ |
| AN4 | With osnap on, dwell near endpoint — tracking rays appear while drafting | ☐ |

## Prior M18/M19 chrome (smoke)

| # | Steps | Pass |
|---|-------|------|
| C1–C6 | New/Open/Save/Undo — see [winui-m19-test-set.md](winui-m19-test-set.md) | ☐ |
| D1–D6 | Draw tools + hatch/text/osnap | ☐ |
| M1–M7 | Modify + box select | ☐ |

## Out of scope (egui)

Full Annotate/Review/PDF docks, STRETCH/ARRAY, DOOR swing polish.
