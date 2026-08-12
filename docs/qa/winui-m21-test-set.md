# WinUI shipping shell — test set (M21)

Headless: `cargo test -p aadt-ffi --lib` (`m19_*` / `m20_*` / `m21_*`).
Manual after `scripts/full-build-and-test.ps1 -Launch` (ABI ≥ **8**).

## Automated

| # | Expect |
|---|--------|
| A1 | `aadt_ffi_version() == 8` |
| A2 | Catalog has WALL, no STRETCH |
| A3 | LINE chains; WALL finishes; selection lines ≥ 4 floats |

## Manual — honesty / daily

| # | Steps | Pass |
|---|-------|------|
| H1 | Tools… lists WALL/DIM/LINE — not STRETCH/ARRAY | ☐ |
| H2 | LINE: 3+ clicks make segments; Space ends | ☐ |
| H3 | Select line — blue highlight | ☐ |
| H4 | WALL: type `0.23` Enter, click path, Space — A-WALL pline | ☐ |
| H5 | Draw wall line; Door → place on wall — opening cut | ☐ |
| H6 | FILLET: type `0.5` then two lines | ☐ |
| H7 | F8 ORTHO / F10 POLAR / F3 OSNAP status chips | ☐ |
| H8 | Delete with selection → erase | ☐ |

See also [winui-m20-test-set.md](winui-m20-test-set.md). Audit: [2026-08-winui-doc-gap.md](../audit/2026-08-winui-doc-gap.md).
