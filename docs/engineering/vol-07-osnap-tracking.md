# Volume 7 — Object Snap & Tracking

Internal engineering handbook. Status: [STATUS.md](./STATUS.md).

## Architectural goals

1. Running object snaps during point prompts (AutoCAD muscle memory).
2. One-shot keyword / menu overrides including PER and TAN.
3. Ortho and polar constrain the raw cursor before snap.
4. Tracking (OTRACK) is a future capability — don’t pretend F11 is full tracking.

## Current implementation

**File:** [`crates/aadt-app/src/osnap.rs`](../../crates/aadt-app/src/osnap.rs)

### `SnapKind` (priority earlier → later for ties)

`Endpoint | Intersection | Midpoint | Center | Quadrant | Perpendicular | Tangent | Nearest`

Labels: `END|INT|MID|CEN|QUA|PER|TAN|NEA`.

PER/TAN are **override-oriented** (need a base / drag anchor).

### Resolve API

| API | Role |
|-----|------|
| `OsnapState::resolve*` | Throttled running snap |
| `find_osnap` / `find_osnap_in_spaces` | Free helpers |
| `snap_candidates` / `_into` | Candidate generation |
| `find_override_in_spaces` | Forced kind |
| `parse_snap_keyword` | 3+ letter abbrevs + long forms |
| `perp_feet_on` / `tangent_points_on` | PER/TAN geometry |

### Throttle

`Duration::from_millis(8)` — reuse `last_hit` unless pointer moved beyond tolerance
**or** ≥8 ms elapsed. Target snap &lt; 5 ms is aspirational under linear scan.

### Input integration

```mermaid
flowchart TD
  raw[raw_world_point]
  raw --> ortho{F8_ortho}
  ortho --> polar{F10_polar}
  polar --> ovr{snap_override}
  ovr -->|yes| force[find_override_in_spaces]
  ovr -->|no| run{F3_osnap}
  run -->|on| resolve[OsnapState_resolve]
  run -->|off| rawOut[use_constrained_point]
  force --> out[accepted_point]
  resolve --> out
  rawOut --> out
```

- F3 toggles running osnap.
- Shift+RMB / typed `END`… arm one-shot override (Vol 3).
- F11 `osnap_track` in prefs currently **forces osnap+polar** — not alignment rays.

## Target / planned

AutoCAD-style object-snap tracking:

1. Acquire points (hover+shift or click-acquire policy — TBD).
- Draw infinite tracking rays / xlines from acquired points.
- Snap to intersections of rays with geometry / other rays.

Also: SpatialGrid candidates for INT/NEAR (Vol 13).

## Gap analysis

| Feature | Status |
|---------|--------|
| Running END/MID/CEN/QUA/INT/NEA | Done |
| PER/TAN override | Done |
| Keyword + menu | Done |
| True OTRACK paths | **Missing** |
| F11 semantics | Prefs shortcut ≠ tracking |
| Indexed candidates | Missing |

## Data structures

```rust
pub struct OsnapState {
    // scratch + last_raw, last_at, last_hit for throttle
}
```

GUI holds `snap_hit: Option<(SnapKind, Point2)>` for marker paint
(`theme::canvas::snap()`).

## Module layout

```
osnap.rs          kinds, resolve, override, keywords
gui.rs            F-keys, menu, paint_snap_marker, constrain_pick
prefs.rs          osnap, polar, osnap_track
```

## Error handling

- Unknown keyword → not a snap override (falls through to command keyword).
- PER/TAN without base → soft fail + note.
- No candidate within aperture → leave constrained point unsapped.

## Performance

- Throttle 8 ms; still O(entities × samples).
- Plan target: snap &lt; 5 ms — requires indexing + tighter aperture.

## Tests

Unit tests in `osnap.rs` for keywords, PER/TAN, multi-space resolve.

## Extension points

1. Spec acquired-point list + ray math in an ADR before coding OTRACK.
2. Decide F11: keep “enable osnap+polar” or reclaim for true tracking.
3. Candidate stage: `SpatialGrid::query_bbox(aperture)` → existing precise tests.

## Related

[Vol 3](./vol-03-input-events.md) · [Vol 13](./vol-13-performance-spatial.md) ·
[ux/command-workflows.md](../ux/command-workflows.md)
