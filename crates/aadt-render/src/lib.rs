//! # aadt-render — 2D rendering layer (substrate, spec §3 / §4.3)
//!
//! GPU-accelerated 2D canvas targeting smooth pan/zoom/redraw (60fps) on drawings
//! with 100k+ entities. Planned backend: `wgpu` (see ADR-0001), driving a custom
//! immediate-mode CAD viewport rather than a retained scene graph.
//!
//! **Status:** skeleton. Real implementation lands in **M1 — CAD Core Substrate**
//! (see `docs/ROADMAP.md`).

/// Performance target from spec §4.3: entities renderable at 60fps.
pub const TARGET_ENTITY_COUNT: u32 = 100_000;
