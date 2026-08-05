# Local API (`aadt serve`)

A loopback-only HTTP/JSON API over a live drafting session — the scriptable
surface from AI-native Phase 5. Design rationale and the security posture are in
[ADR-0015](../adr/0015-local-api-layer.md); this page is the reference.

The server is **off unless you start it**. Nothing in the GUI or the CLI depends
on it, and drafting offline never touches it.

## Start it

```bash
aadt serve                          # 127.0.0.1:7421, token printed once
aadt serve plan.dxf --port 8080     # open a drawing first (OPEN's format routing)
aadt serve --read-only              # reads only; POST /v1/command is refused
aadt serve --token dev --once       # fixed token, answer one request, exit
aadt serve --identity erp-bot --role connector   # M14.2 principal + caps
```

`--port 0` asks the OS for a free port; the banner reports the one it got.

```
API listening on http://127.0.0.1:7421 (read/write)
token: 9f3c…
try:   curl -H "Authorization: Bearer 9f3c…" http://127.0.0.1:7421/v1/health
```

## Authentication

Every route requires the token, health included:

```
Authorization: Bearer <token>
```

`X-Aadt-Token: <token>` is accepted as an alternative for clients that cannot
set `Authorization`. Anything else answers `401`.

After the token check, **M14.2** applies the principal's capability set
(`--role viewer|editor|connector|admin`). A viewer can read; `POST /v1/command`
needs `command` (and finer verb checks for `OBJECTS RUN`, `CONNECTOR`, …).
`POST /v1/shutdown` needs `admin`. Health reports `principal` and
`capabilities`.

Object ACLs: `PERMS GRANT alice rw 12` stores `acl.alice=rw` on object #12.
Signed audit: set `AADT_AUDIT_KEY` and run `OBJECTS AUDIT VERIFY`.

Enterprise connectors and site ingest go through the same command door:

```bash
curl … -d '{"line":"CONNECTOR ERP PUSH BOQ out.csv"}'
curl … -d '{"line":"SITE INDEX PHOTO site/wall.jpg --caption north crack"}'
```

## Routes

| Method | Path | Returns |
|---|---|---|
| `GET` | `/v1` | The route index |
| `GET` | `/v1/health` | Liveness, version, mode, entity count, revision |
| `GET` | `/v1/document` | Counts, revision, extents, drawing properties |
| `GET` | `/v1/entities?limit&offset&layer` | A page of entities (default 200, max 5000) |
| `GET` | `/v1/entities/{id}` | One entity, or `404` |
| `GET` | `/v1/layers` | Layers sorted by name, with colour / lineweight / state |
| `GET` | `/v1/selection` | The current selection set's ids |
| `GET` | `/v1/history` | Command lines that ran, oldest first |
| `GET` | `/v1/transcript` | Every feedback entry with its status |
| `GET` | `/v1/commands?prefix` | Command-name completions |
| `POST` | `/v1/command` | Run one command line |
| `POST` | `/v1/shutdown` | Stop the server |

## Running commands

Writes go through one route, carrying the same line you would type at the
prompt — so an API caller gets the same validation, undo record, and history
entry as a person drafting.

```bash
curl -s -X POST http://127.0.0.1:7421/v1/command \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"line":"LINE 0,0 5000,0 A-WALL"}'
```

```json
{
  "status": "ok",
  "message": "LINE added as entity 12",
  "line": "LINE 0,0 5000,0 A-WALL",
  "revision": 341,
  "changed": true,
  "entities": 13,
  "selection": []
}
```

`status` is `ok`, `silent`, or `error`. **A rejected command is still `200`** —
the HTTP request succeeded, and the command inside it did not. Reserve the 4xx
codes for requests that were themselves wrong.

One line per request: an embedded newline is refused, because one response has
one status to report with.

Because the session is the CLI's, the whole command band is reachable — `OPEN`,
`SAVE`, `LAYER`, `OBJECTS`, `RELATIONS`, `TAKEOFF`, `BOQ`, `FIND`, `ASKDOCS`,
`SUGGEST`, `CHECK`, `COMPARE`, `RECOMMEND DETAIL`, and `LISP` / bare `(…)`.

**Edits stay in memory** until a `SAVE` command writes them out. The server
never autosaves.

## Errors

Every failure has one shape:

```json
{"error": "no such route /v1/nope (GET /v1 lists them)", "status": 404}
```

| Status | Meaning |
|---|---|
| `400` | Malformed request, body, or query value |
| `401` | Missing or wrong token |
| `403` | `POST /v1/command` while `--read-only` |
| `404` | No such route, or no such entity |
| `405` | Known route, wrong method |
| `411` / `413` / `431` | Chunked body, body over 1 MiB, oversized headers |
| `505` | Not HTTP/1.x |

## Limits and behaviour

- **Loopback only.** Binds `127.0.0.1`; non-loopback peers are dropped without
  a reply.
- **One connection at a time.** A session is one editor on one drawing, so
  requests serialise. Sockets carry a 10 s read/write timeout.
- **One request per connection** (`Connection: close`); no keep-alive.
- **Bounded input:** 1 MiB body, 8 KiB request line and header lines, 64
  headers. `Content-Length` bodies only — chunked encoding is refused, not
  guessed at.
- **Deterministic responses.** Entities page in id order, layers sort by name,
  attributes serialise in key order, and non-finite numbers become `null`, so
  identical calls produce identical bytes.

## Example: quantities into another system

```bash
TOKEN=…  # from the banner
curl -s -X POST http://127.0.0.1:7421/v1/command \
  -H "Authorization: Bearer $TOKEN" -d '{"line":"OPEN tower.vdb"}'
curl -s -X POST http://127.0.0.1:7421/v1/command \
  -H "Authorization: Bearer $TOKEN" -d '{"line":"BOQ"}' | jq -r .message
```
