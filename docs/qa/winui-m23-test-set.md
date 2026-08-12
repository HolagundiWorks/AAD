# WinUI M23 test set — optional gap close (ABI v10)

Verify after `cargo test -p aadt-ffi --lib` and a Debug WinUI build with a
fresh `aadt_ffi.dll` (ABI ≥ 10).

## Automated

| # | Check | Expect |
|---|-------|--------|
| A1 | `cargo test -p aadt-ffi --lib` | `m23_*` + abi_version_is_10 pass |
| A2 | Catalog | `WPOLYGON` / `CPOLYGON` / `FENCE` lines present |

## Manual (AadWinui)

| # | Steps | Expect |
|---|-------|--------|
| M1 | Draw lines → **WP** → click 4 corners → Space | Enclosed entities selected |
| M2 | **CP** / **Fence** similarly | Crossing / fence hits |
| M3 | Hatch a closed rect → solid fill visible under strokes | Accent fill paint |
| M4 | Start LINE → type `'ZOOM E` → Enter | Extents; LINE still active |
| M5 | Mid-tool `'ZOOM` → drag window | View reframes; tool continues |
| M6 | QAT **ZoomE** / **Review** / **Plot** | Extents; LIST or count note; PLOTSETUP |

## Build

```powershell
cargo test -p aadt-ffi --lib
cargo build -p aadt-ffi --release
# copy target\release\aadt_ffi.dll beside AadWinui.exe
msbuild native\aad-winui\winui\AadWinui.sln /p:Configuration=Debug /p:Platform=x64
```
