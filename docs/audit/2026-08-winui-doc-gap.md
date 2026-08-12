# Audit — WinUI shell vs documentation (2026-08-07)

Deep compare of shipping WinUI + `aadt-ffi` against ROADMAP, STATUS, UX workflows,
and BricsCAD audit. BricsCAD scorecard remains **egui-scoped**.

## Verdict

M16–M20 ABI surface is real and usable for a thin daily band. Several ✅ exits
overstated CAD parity (DOOR without wall cut, LINE 2-pt only, catalog listing
non-startable tools, no selection highlight, FILLET r=0, OTRACK polar off).

## Doc lies (fixed or demoted in M21)

| Claim | Reality pre-M21 | M21 action |
|-------|-----------------|------------|
| DOOR/WINDOW place | Bare INSERT | Wall cut via `place_opening_pose` + `cut_wall_opening` |
| LINE multi-vertex | Auto-commit 2-pt end | Chain: commit segment, continue until Space |
| Tools catalog | Full CATALOG incl. WALL/STRETCH | Filter to `INTERACTIVE_TOKENS` (+ INSERT) |
| Selection visible | Gray strokes only | Selection overlay lines + accent paint |
| FILLET | Radius hard-coded 0 | Typed radius / last radius |
| WALL on WinUI | Catalog only; feed fails | Interactive WALL → POLYLINE + attrs |
| OTRACK = L2 | Polar always false | Polar toggle (F10) |
| Ortho | Missing | F8 ortho on picks |

## Accurate deferred (egui)

STRETCH/ARRAY FSMs, full Annotate/Review/PDF, multi-mode grips, DOOR swing Tab,
WP/CP/FENCE, osnap keyword menu, hatch solid paint, IMAGE bitmap blit,
transparent `'ZOOM`/`'PAN`, dyn near-cursor.

## Dual catalog (STATUS Vol 2)

`Registry` ⊃ `CATALOG` ⊃ `INTERACTIVE_TOKENS`. M21 keeps the three layers but
makes Tools UI honest (`INTERACTIVE_TOKENS` only).

## Ranked remaining backlog (post-M23)

1. Hatch solid/pattern D2D paint · 2. WP/CP/FENCE tools · 3. Review/PDF thin port ·
   → **closed in M23** (full Review/PDF docks + IMAGE blit remain Later).
4. Apostrophe `'ZOOM` suspend (optional; wheel + `'ZOOM E` cover it) · 5. IMAGE bitmap blit

M22 closed: STRETCH, ARRAY*, IMAGE picker, REVCLOUD, multi-mode grips, swing Tab,
Shift+select. M23 closed items 1–3 (+ thin transparent nav).
