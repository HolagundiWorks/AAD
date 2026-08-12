# ADR-0018 — True object-snap tracking (OTRACK)

- **Status:** Accepted
- **Date:** 2026-08-06
- **Scope:** egui drafting viewport (`aadt-app`); headless math in `otrack.rs`.
  WinUI paints H/V (±polar via F10) rays via `aadt_otrack_*` (M20/M21 ABI).
  Temporary tracking from a **live tool anchor** remains a gap (STATUS Vol 7).
- **Related:** [Vol 7](../engineering/vol-07-osnap-tracking.md), ROADMAP Later **L2** ✅.

## Context

F11 (`osnap_track`) only forced running osnap + polar. Users expect AutoCAD-style
**object-snap tracking**: acquire snap points, show alignment paths, and snap to
path intersections while drafting.

## Decision

1. **F11 enables true OTRACK.** Turning it on still turns **osnap** on (tracking
   needs snap candidates to acquire). Polar remains independent; when polar is
   on, tracking rays also include 45° increments.
2. **Acquire policy:** while OTRACK is on and drafting expects a point, a
   running snap under the cursor that dwells ≥ **250 ms** is acquired (max **7**
   points). Esc (when not cancelling a tool phase) or turning OTRACK off clears
   the list. Duplicate points within aperture are ignored.
3. **Rays:** each acquired point emits infinite **horizontal** and **vertical**
   paths; with polar, also paths at **45°** increments through the point.
4. **Resolve:** after running osnap (and one-shot overrides), if OTRACK is on,
   also consider (a) intersections between tracking rays and (b) the nearest
   projection of the cursor onto a tracking ray, within the snap aperture.
   Closest-to-cursor wins; a live osnap hit still wins when equally close or
   closer.
5. **Paint:** dashed tracking lines across the viewport + acquired-point markers
   (distinct from the live snap marker).

## Consequences

- Prefs / F11 copy and status-bar OTRACK chrome mean real tracking, not a
  polar synonym.
- Headless unit tests cover ray math and acquire without egui.
- Interim WinUI host paints OTRACK rays (M20+) and polar via F10 (M21).

## Alternatives considered

- **Keep F11 as osnap+polar only:** rejected — Vol 7 / ROADMAP L2 require true
  paths.
- **Shift-only acquire (no dwell):** rejected as primary — muscle memory wants
  hover acquire; Shift remains available as an optional force-acquire later.
- **Full polar + temporary tracking from tool anchor:** deferred; H/V (+ polar
  angles) from acquired snaps is the L2 exit.
