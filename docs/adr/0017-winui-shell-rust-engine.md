# ADR-0017 — WinUI 3 shipping shell; Rust engine via FFI

- **Status:** Accepted
- **Date:** 2026-08-06
- **Supersedes (shell portion of):** [ADR-0001](0001-language-and-ui-stack.md)
- **Scope:** **Hybrid** — C++ / WinUI 3 + Fluent 2 + Direct2D chrome; CAD engine remains Rust. A full C++ rewrite of geometry/commands/LISP/ShilpiDB is **out of scope** (separate future ADR + budget only).

## Context

The egui shell shipped drafting power quickly (ADR-0001) but feels non-native on Windows. Product direction is native Fluent chrome and a Direct2D viewport without discarding the shipping Rust Session, LISP, ShilpiDB, or AI-native band (M10–M15).

A complete move of the engine to C++ would rewrite ~24 crates and risk file-format / LISP / test regression for 12–24+ months. That cost is not justified to get WinUI polish.

## Decision

1. **Shipping interactive shell:** C++/WinRT **WinUI 3** + **Fluent 2**, layout aligned with AAD Chrome 2.0 (single top strip, CLI hero, auto-hide docks, Properties primary). Sources live under `native/aad-winui/`.
2. **Viewport:** **Direct2D** on a `SwapChainPanel` (or interim Win32/D2D host). Prefer one Windows graphics stack — not Skia *and* Direct2D.
3. **Engine:** unchanged Rust workspace. The shell drives [`aadt-ffi`](../../crates/aadt-ffi) (`aadt.dll`) — `Session::feed`, drawlist export, selection queries. **No Document mutation from XAML/ViewModels** except through FFI that wraps feed / typed engine APIs.
4. **egui:** maintenance / fallback only (`aadt app` with `--features app`). Not the Windows shipping path once WinUI reaches command parity.
5. **CLI / `aadt serve`:** remain Rust.
6. **Complete C++ engine port:** explicitly **not** scheduled. Optional later ADR with port order geometry → model → command → store → … only after WinUI ships.

## Consequences

- Dual toolchain (MSVC + cargo) in CI for Windows GUI builds.
- FFI surface must grow (full command registration, drawlist lines, dirty/path) and stay versioned.
- Interactive tool FSM currently in egui must migrate behind Session/FFI over time; until then WinUI can run headless-style commands and simple pick loops via `feed`.
- Brand tokens (AAD Chrome 2.0) apply to WinUI resources; orange only for brand/focus.

## Alternatives considered

- **Full C++ rewrite now:** rejected — multi-year, high regression, no drafting gain.
- **Slint chrome + Rust canvas:** viable Rust-only chrome swap; rejected in favour of Fluent/WinUI product ask.
- **Keep egui forever:** rejected — native Windows feel is now a product requirement.
- **MVVM Command Manager owning geometry:** rejected — fights deterministic `Session::feed` / undo.
