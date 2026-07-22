# Two-Agent Parallel Roadmap

A split of remaining work into two tracks that can run concurrently with minimal
collision: **Track A owns the headless deterministic engine; Track B owns the
interactive surface.**

Product locks shared by both tracks (see [`ROADMAP.md`](ROADMAP.md)): **2D only**,
**lean daily commands**, **LISP for depth**, **egui shell now** (WinUI only later
via FFI).

Why this seam:

- **Disjoint file sets.** Neither track edits the other's crates (see ownership).
- **Different verification.** Track A: `cargo test --workspace`. Track B: display
  + human eyes, plus headless `Session` tests where possible.
- **Preserves the core principle.** Deterministic core stays reproducible
  independent of GUI and AI (spec §8).

---

## Crate ownership

| Crate | Owner | Notes |
|---|---|---|
| `aadt-geometry` | **A** | Kernel. Additive only. |
| `aadt-model` | **A** | Shared vocabulary — see contract. |
| `aadt-dxf` | **A** | Import/export. |
| `aadt-detect` | **A** | Detection, classification, anomalies. |
| `aadt-takeoff` | **A** | Quantities, cost codes, xlsx, schedules. |
| `aadt-ai` | **A** | Orchestration, lifecycle, apply. |
| `aadt-lisp` | **shared** | **A** owns interpreter/host; **B** owns Session/`LISP` wiring. |
| `aadt-render` | **B** | Drawlist, camera, optional wgpu/gui. |
| `aadt-app` | **B** | `Session`, egui shell. |
| `aadt-ffi` | **B** | Thin cdylib over `Session`. |
| `aadt-command` | **B** | Built-ins; do not add bloat commands. |
| `aadt-cli` | **shared** | **A** owns `takeoff`/`run` + golden; **B** owns `app`/`shell`/LISP-in-session. |
| `aadt-docs`, `aadt-pdf`, `aadt-collab` | — | M5–M8 cores done; polish as needed. |

---

## Track A — Deterministic Engine (status)

| Item | Status |
|---|---|
| A1 Room merge/dedup | ✅ `Detection::merged_rooms` |
| A2 DXF entity coverage | ✅ arcs, circles, text, hatch, blocks, dims |
| A3 `.xlsx` + schedule tables | ✅ `to_xlsx`, `SCHEDULE` |
| A4 AI model lifecycle | ✅ `aadt_ai::lifecycle` |
| A5 Anomaly flagging | ✅ `aadt_detect::anomaly` |

**Track A next (optional polish):** AI schedule/notes formatting; DXF edge cases
(multi-path hatch, nested inserts); native `.aadt` SQLite (ADR-0004).

---

## Track B — Interactive Surface (status)

| Item | Status |
|---|---|
| B1 wgpu 2D canvas | Seam ✅ (drawlist/camera); live 100k wgpu **deferred** until needed |
| B2 App shell | ✅ egui command-first shell |
| B3 Interactive commands | ✅ headless curated band; GUI click-tools for daily set |
| B4 Tab-completion | ✅ |
| B5 Live takeoff panel | ✅ consumes `LiveTakeoff` |

**Track B current / next:**

1. ~~Lean egui drafter + Ortho + `rfd` + LISP + FFI~~ ✅
2. ~~Endpoint/midpoint object snap (F3)~~ ✅
3. ~~Interactive OFFSET (`O`)~~ ✅
4. ~~Native `.aadt` (shared with Track A store crate)~~ ✅
5. Viewport text glyphs; DWG later — **not** new obscure builtins.

---

## Interface contract

1. **`aadt-model` is the shared vocabulary.** Track A owns it. Track B mutates
   **only** through `Document::transact` and registered commands.
2. **New entity types:** Track B files a request to Track A — no unilateral edits.
3. Track A: **additive changes only** to `Document` / `Detection` / `Takeoff`
   signatures without flagging first.

---

## Shared-file protocol

| File | Rule |
|---|---|
| `Cargo.lock` | Never hand-merge; regenerate with `cargo check`. |
| Workspace `Cargo.toml` | Announce `[workspace.dependencies]` edits. |
| `README.md` | A owns pipeline; B owns app/UI sections. |
| `ROADMAP.md` | Update your track's status; product locks are shared. |
| `.github/workflows/ci.yml` | Track A owns. |

**Branching.** `track-a/<item>` and `track-b/<item>`, one PR per item, rebase on
`main`. Green bar: `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace`.
