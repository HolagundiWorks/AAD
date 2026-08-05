# ADR-0015 — The local API is a command line over HTTP

- **Status:** Accepted
- **Authorised by** [ADR-0012](0012-ai-native-platform-pivot.md) Phase 5, which
  scheduled "everything the UI can do, scriptable — extend `aadt-ffi` and add a
  local REST/gRPC service crate (offline drafting never depends on it)".
- **Ships as** ROADMAP M14.1 (`aadt-api`, `aadt serve`).

## Context

Phases 1–4 built the data layers (objects, graph, vectors, corpus, takeoff) and
the assist services on top of them. Everything is reachable from a keyboard and
from LISP, and nothing is reachable from another process. An estimating tool
that wants a BOQ, a site app that wants the door schedule, or a CI job that
wants to check a drawing against a rule pack all have the same options today:
shell out to `aadt run` with a temporary script and scrape stdout, or link the
crates.

Scraping a human-readable transcript is the option people actually take, and it
is the worst one: the output was designed to be read, so every message that gets
reworded breaks a consumer that was never visible to us.

## Decision

Add `aadt-api`: a **loopback-only, token-gated HTTP/JSON server over a live
`Session`**, started explicitly by `aadt serve` and off otherwise.

### Reads project the session; the single write is a command line

There is no `POST /v1/entities`. The only mutation is:

```
POST /v1/command   {"line": "LINE 0,0 5,0"}
```

which feeds the interpreter the same string a user would type. This is the
decision the rest follows from.

A REST-shaped write API — `POST /entities`, `PATCH /entities/12` — is the
obvious design and the wrong one here. It would be a **second door into the
document**, and every invariant the command layer enforces (validation, layer
resolution, one `Document::transact` per edit, an undo record, a history line,
an audit row) would have to be re-enforced behind it, forever, in step. The
first time the two paths disagree, the drawing is only as correct as whichever
door the caller happened to use.

Routing writes through the command line means an API caller is indistinguishable
from a person at the prompt: the same parse, the same errors, the same undo
stack, the same transcript. "Everything the UI can do" is then true by
construction rather than by effort — and it is literally true, because
`aadt serve` hands the server the CLI's own registered session, so `OPEN`,
`SAVE`, `OBJECTS`, `TAKEOFF`, `BOQ`, `SUGGEST`, and `CHECK` are all reachable on
day one without a line of per-command API code. A golden test asserts exactly
that by calling `OBJECTS` — a CLI-registered command, not an interpreter
built-in — over HTTP.

The cost is real and accepted: callers compose command strings rather than
typed request bodies, so the API is only as ergonomic as the command language.
That is the same bargain LISP already made (ROADMAP: "LISP for depth"), and it
keeps the deterministic core as the sole author of geometry.

### HTTP/JSON rather than gRPC, and std rather than a framework

Phase 5 said "REST/gRPC". REST wins because the clients are scripts, spreadsheet
macros, and other people's estimating software — `curl` and one page of docs,
not a protobuf toolchain. ShilpiDB already covers the binary, high-throughput
case over its own wire protocol.

The implementation is std-only: `TcpListener`, a small HTTP/1.1 subset, and a
hand-rolled JSON encoder/decoder. That follows the workspace rule that new heavy
dependencies need an ADR, and the surface genuinely is small — one request per
connection, `Content-Length` bodies, eleven routes. Anything outside the subset
(chunked bodies, HTTP/2) is **refused by status code rather than approximated**,
because a half-implemented HTTP feature on a network surface is worse than an
absent one.

### Single-threaded, and honest about why

A `Session` is not `Send` — the command registry holds `Rc<RefCell<…>>` state
shared between `SAVE` and the Object-DB commands. Rather than work around that,
the server leans on it: a session is one editor on one drawing, and concurrent
mutation would need a document lock producing exactly the serialisation an
accept loop already gives.

So connections are answered one at a time. The consequence is that a slow client
delays every other one, which is why sockets carry read and write timeouts and
why request line, header count, and body size are bounded *before* any
allocation grows to match them.

### Security posture, stated plainly

- **Loopback bind.** `127.0.0.1` only, plus a peer check that drops
  non-loopback connections without a reply. This is a local automation seam,
  not a service; it is not hardened for the internet, so it is not reachable
  from it.
- **Token on every route, health included.** No endpoint is unauthenticated,
  so there is none whose gate can be quietly widened later. The comparison is
  constant-time.
- **The token is not a cryptographic secret.** It is mixed from the clock, the
  pid, and an ASLR-dependent address through SplitMix64 — enough that another
  local process cannot derive it from a clock reading, which is the realistic
  threat on a port any local program may connect to. The workspace takes no RNG
  dependency, and anything that can read this process's memory or the terminal
  the token was printed to has already won. The boundary is the loopback bind
  plus the user's decision to run `aadt serve` at all.
- **`--read-only`** refuses `POST /v1/command` while still serving every read —
  the mode for exposing a drawing to a dashboard or a reviewer.

### A failed command is a successful request

`POST /v1/command` answers `200` with `{"status":"error"}` when the command is
rejected. Mapping a bad CAD command onto `400` would conflate "your HTTP was
malformed" with "your command was", and a client has to tell those apart to know
whether retrying differently could ever help.

## Consequences

- **Offline drafting is unaffected.** Nothing starts the server implicitly; the
  GUI and CLI never call into `aadt-api`. Phase 5's "offline drafting never
  depends on it" holds by construction.
- **`aadt-ffi` stays as it is.** It is the in-process C ABI for native chrome;
  this is the out-of-process seam for other programs. They answer different
  questions and neither replaces the other.
- **Responses are deterministic.** Entities page in id order, layers sort by
  name, attributes serialise through a `BTreeMap`, and non-finite numbers become
  `null`. Two identical calls produce identical bytes, so responses can be
  diffed, cached, and golden-tested.
- **Edits live in memory until a `SAVE` command.** The server does not
  autosave; a caller that wants a file writes one, exactly as at the prompt.
- **Deferred to later Phase 5 increments:** object-level permissions, a signed
  audit trail, and connectors as plugins over this API. The endpoints here are
  their substrate; none of them is assumed by the current design beyond leaving
  room for an authorisation check where the token gate now sits.
