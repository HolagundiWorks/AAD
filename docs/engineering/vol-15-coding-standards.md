# Volume 15 — Developer Coding Standards

Internal engineering handbook. Status: [STATUS.md](./STATUS.md).

## Architectural goals

1. Keep one write path so undo, audit, API, and tests stay coherent.
2. Prefer curated commands + LISP depth over AutoCAD command-count chase.
3. Record durable decisions as ADRs; keep this handbook honest about Current vs Target.
4. GUI chrome uses shared theme primitives — no one-off styling in docks.

## Current rules (enforce in review)

### CAD spine

1. **Mutate only via `Session::feed` → `Command::run` → `Document::transact`.**
   Paint, osnap, and selection hit-tests are read-only.
2. **No nested `transact`** — debug assert panics.
3. **Empty transactions discard** — don’t force undo noise.
4. **OPEN / `set_document` clears undo** — say so in UX copy when relevant.

### Commands & product locks

5. **Curated interactive band** — new daily aliases need ROADMAP justification;
   prefer LISP or CLI-registered packs for depth.
6. **Two catalogs:** if you add a `Tool`, update `command_engine::CATALOG` **and**
   the headless `Command` that accepts the assembled line.
7. **Assist never auto-commits** (ADR-0013).
8. **API writes are command lines only** (ADR-0015) — no entity POST.
9. **Transparent edits:** only view cmds may use apostrophe suspend; don’t invent
   transparent TRIM.

### GUI / chrome

10. Use `gui_theme` primitives (`BtnKind`, `link_button`, `elevation`, `canvas::`,
    `prop_row`, …) — see mono UI work / branding docs.
11. Canvas selection blue stays in `canvas::`; chrome accent stays orange.

### Process

12. Durable product/architecture locks → new or updated ADR under `docs/adr/`.
13. Close handbook gaps by updating [STATUS.md](./STATUS.md) in the same PR.
14. `unsafe` is forbidden at workspace level unless an ADR explicitly allows a crate.

### Tooling (from README)

```
cargo fmt
cargo clippy -- -D warnings
cargo test --workspace
```

There is **no** `CONTRIBUTING.md` yet — this volume is the contributor contract.

## Target / planned

- Optional CI lint forbidding raw chrome `ui.button` in `gui_chrome.rs`.
- PR template checklist excerpt below.
- Windows note: avoid PowerShell `Set-Content` on UTF-8 Rust sources (BOM/ellipsis corruption).

## Gap analysis

Standards are mostly review-enforced, not mechanical. ShilpiDB SoT wording in
the target architecture doc remains dual-narrative (see STATUS).

## PR checklist (recommended)

- [ ] Mutations go through `feed` / `transact`
- [ ] New interactive tool: `CommandDef` + `Tool` + headless `Command` + workflows row
- [ ] Assist paths cannot commit without apply/ACCEPT
- [ ] API: no new write route bypassing `/v1/command`
- [ ] Tests: headless case added
- [ ] Docs: STATUS.md / volume updated if closing a gap
- [ ] ADR if changing a product lock

## Module layout

N/A — process volume. Point feature work at Vol 1–3 first.

## Error handling

Prefer `Result` / `Feedback` over panics; reserve `debug_assert` for programmer
errors (nested txn).

## Performance

Don’t land O(n²) GUI pick “temporarily” without a Vol 13 ticket in STATUS.

## Tests

See [Vol 14](./vol-14-testing.md). New `Command` without a test is incomplete.

## Extension points

1. Add `CONTRIBUTING.md` that links here when onboarding grows.
2. Expand checklist into GitHub PR template when ready.
3. Wave 3 of handbook: deepen any volume that accumulates “next-doc TODOs”.

## Related

[README](./README.md) · [Vol 2](./vol-02-command-engine.md) · [adr/](../adr/) ·
[ROADMAP](../ROADMAP.md)
