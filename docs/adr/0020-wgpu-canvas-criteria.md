# ADR-0020 — Optional wgpu canvas criteria (L5.2)

- **Status:** Accepted
- **Date:** 2026-08-06
- **Scope:** When to use the `aadt-render` wgpu path vs egui paint / WinUI Direct2D.
- **Related:** [ADR-0001](0001-language-and-ui-stack.md), [ADR-0017](0017-winui-shell-rust-engine.md),
  [Vol 9](../engineering/vol-09-rendering.md), ROADMAP L5.2.

## Context

L5 shipped TrueType outlines and world-height egui TEXT. wgpu already exists as
`--features gpu` (offscreen PNG) and `--features gui` (winit window) on
`aadt-render`, plus CLI `aadt render … --png`. ADR-0017 locks the **shipping**
Windows shell to WinUI + Direct2D. The question is when egui or tools may use
wgpu without becoming the product default.

## Decision

1. **Default interactive shells stay:**
   - Windows shipping: **WinUI + Direct2D** (ADR-0017).
   - Maintenance fallback: **egui** painter over `DrawList`.
2. **Optional wgpu path is shipped** for:
   - Headless / CI PNG via `WgpuRenderer` and `aadt render --features gpu`.
   - Interactive wgpu window via `aadt_render::view::run` (feature `gui`) —
     opened from egui as **File → GPU canvas…** when built with
     `aadt-app` feature `wgpu-canvas`.
3. **Do not switch the default shell to wgpu** unless all of the following hold
   (revisit in a new ADR):
   - Sustained zoom/pan p95 **> 16 ms** on a **≥ 50k** entity drawing with F7 HUD
     on egui (same machine class as CI laptop).
   - wgpu path holds **≤ 16 ms** p95 on the same drawing.
   - WinUI Direct2D cannot meet the budget after a documented investigation.
4. **Glyphs** (L5.1) remain shared (line outlines / egui text); wgpu consumes the
   same outline tessellation as FFI.

## Consequences

- L5.2 is **complete** as: criteria ADR + optional entry points, not a shell swap.
- Perf sign-off at 100k+ entities still uses the gpu feature path for evidence.
- Product messaging: “GPU canvas” is a diagnostic / heavy-drawing tool, not Chrome 2.0.

## Alternatives considered

- **wgpu as egui default now:** rejected — WinUI is the shipping path; egui is
  fallback; no budget failure documented against the criteria above.
- **Remove wgpu:** rejected — needed for headless PNG and large-drawing proof.
