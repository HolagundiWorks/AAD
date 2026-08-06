# ADR-0016 — M14 enterprise: permissions, signed audit, connectors, site ingest

- **Status:** Accepted
- **Authorised by** [ADR-0012](0012-ai-native-platform-pivot.md) Phase 5 /
  [ROADMAP M14](../ROADMAP.md#m14--ai-native-phase-5-enterprise--apis-)
- **Builds on** [ADR-0015](0015-local-api-layer.md) (local command-line API)

## Context

M14.1 shipped a loopback, token-gated HTTP API whose **only write door** is
`POST /v1/command`. Completing Phase 5 required object-level permissions, a
signed audit trail, enterprise connectors, and site-data ingest into the
vector/corpus store — without adding a second mutation API.

## Decision

### M14.2 — Object-level permissions

- A **principal** + **capability set** (roles: viewer / editor / connector /
  admin) attaches to `ApiConfig`. The token gate stays first; a capability check
  follows for the route and again for the command verb
  (`aadt_object::command_capability`).
- Per-object ACL lives as `acl.<principal>` props (`r` / `rw` / `-`) on
  `EngObject` — no REST ACL API, no schema table. Grant/revoke via `PERMS`.
- CLI: `aadt serve --identity NAME --role viewer|editor|connector|admin`.

### M14.3 — Signed audit trail

- Optional HMAC-SHA256 hash chain on `AuditEntry` (`prev_hash`, `hash`),
  std-only (`aadt_object::hmac`). Key from `AADT_AUDIT_KEY`.
- Empty hashes = unsigned legacy. `OBJECTS AUDIT VERIFY` checks the chain.
- `.aadt` schema **v10** adds columns; `.vdb` format **v3** writes the fields
  (v2 still loads with empty hashes).

### M14.4 — Enterprise connectors as plugins

- Connectors are **registered commands** (`CONNECTOR LIST|ERP|RFI|DMS`), not a
  second HTTP write surface. They emit sidecar payloads a host ERP / RFI / DMS
  system can pick up. Geometry still changes only through the command band.
  Each push/export appends an audit row.

### M14.5 — Site data → corpus

- Photos and reports become **text chunks** in `CorpusDb` (captions + PDF
  literal-string extract). Commands: `SITE INDEX PHOTO|PDF|REPORT` and
  `ASKDOCS INDEX PHOTO|PDF|REPORT`. Retrieval stays `ASKDOCS` / `FIND`.
- No OCR model; no geometry `VectorDb` pollution.

## Consequences

- Offline drafting unchanged; server still opt-in.
- Assist still proposes only; signing does not auto-commit.
- Explicit ACL deny (`-`) overrides editor caps for that object; admin bypasses.
- New heavy crypto crates still need an ADR — HMAC-SHA256 stays in-tree.
