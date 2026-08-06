# Volume 12 — Plugin / API Architecture

Internal engineering handbook. Status: [STATUS.md](./STATUS.md).

## Architectural goals

1. Extensions are **commands** (and LISP) on one `Registry`.
2. HTTP API drives a live `Session` — reads project it; writes only via command line
   (ADR-0015).
3. No second mutation API (`POST /v1/entities` rejected).
4. Substrate stays lean — heavy Assist/I/O packs register from CLI.

## Current implementation

### Extension mechanisms

| Mechanism | How |
|-----------|-----|
| `Registry::register` | `Box<dyn Command>`; later aliases can shadow |
| CLI `register(interp)` | OPEN, SAVE, HISTORY, Assist, libs, SERVER |
| LISP | `Session::try_feed_lisp` — `(…)`, `LISP`, `LISPLOAD` |
| Local API | `aadt-api` HTTP/JSON over Session |

There is **no** stable `.dll` plugin ABI yet — “plugin” means registered commands
or scripts.

### API routes ([`aadt-api/src/routes.rs`](../../crates/aadt-api/src/routes.rs))

| Method | Path | Role |
|--------|------|------|
| GET | `/`, `/v1`, `/health`, `/v1/health` | Index / health |
| GET | `/v1/document`, `/entities`, `/entities/{id}`, `/selection`, `/layers`, `/history`, `/transcript` | Reads |
| GET | `/v1/commands?prefix=` | Completion |
| **POST** | **`/v1/command`** | **Sole write** — `{"line":"..."}` → `session.feed` |
| POST | `/v1/shutdown` | Stop server |

Auth: `Authorization: Bearer` or `x-aadt-token` on every route.
Default port `7421` (loopback). Multiline command bodies rejected.
`read_only` mode → 403 on write.

```mermaid
flowchart LR
  http[HTTP_client]
  api[aadt_api]
  sess[Session]
  feed[feed_line]
  http --> api --> sess --> feed
```

### CLI-registered names (sample)

`OPEN`, `SAVE`, `HISTORY`, `MAPMETA`, `FIND`, `ASKDOCS`, `WITHIN`, `SUGGEST`,
`CHECK`, `COMPARE`, `RECOMMEND`, `ASK`, `TAKEOFF`, `BOQ`, `OBJECTS`, `RELATIONS`,
`PERMS`, `CONNECTOR`, `SITE`, `LAYERSET`, `STANDARDS`, `LAYERREPORT`,
`LAYERLEGEND`, `OBJMETA`, `ADOPTLAYERS`, `SECTION`, `BLOCKLIB`, `HATCHLIB`,
`SERVER`.

## Target / planned

Keep command-line write path forever for invariants (undo, audit, validation).
M14.2–5 shipped behind that door ([ADR-0016](../adr/0016-m14-enterprise.md)):
permissions, signed audit, `CONNECTOR` plugins, site corpus ingest — not raw
entity CRUD.

Optional future: plugin ABI ADR (load commands dynamically) — not scheduled.

## Gap analysis

| Idea | Status |
|------|--------|
| Entity POST | Rejected |
| External SDK | Not shipped |
| M14.1–5 API / enterprise | Done ([ADR-0015](../adr/0015-local-api-layer.md), [ADR-0016](../adr/0016-m14-enterprise.md)) |
| Dynamic plugins | Open |

## Data structures

API responses include `status`, `message`, `line`, `revision`, `changed`,
`entities`, `selection` (see `aadt-api` JSON helpers).

## Module layout

```
aadt-api/     lib, routes, http, json
aadt-cli/     commands::register, SERVER host
aadt-command/ Registry trait surface
aadt-lisp/    script host
```

Docs: [`docs/api/README.md`](../api/README.md), [ADR-0015](../adr/0015-local-api-layer.md).

## Error handling

Map `Feedback` to HTTP status/body; never panic the host Session on bad JSON.
Token failures → 401/403.

## Performance

In-process Session — long commands block the API loop; prefer short lines or
document timeouts when adding heavy Assist routes.

## Tests

`aadt-api` route/unit tests; CLI registration tests; golden scripts that hit feed.

## Extension points

1. New capability → implement `Command` + register (CLI pack if deps are heavy).
2. New read route is fine; new write route that bypasses `feed` is not.
3. LISP host: document allowed side effects before expanding FFI into model.

## Related

[Vol 2](./vol-02-command-engine.md) · [Vol 11](./vol-11-ai-semantic.md) ·
[ADR-0015](../adr/0015-local-api-layer.md)
