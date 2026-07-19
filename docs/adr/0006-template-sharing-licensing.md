# ADR-0006 — Template & notes-library sharing / licensing

- **Status:** Proposed (revisit at M5)
- **Context:** Spec §5.3 / §11 #6 — title-block templates and general-notes
  libraries are defined once per firm and reused across projects. How is
  firm-wide sharing licensed and delivered without building the cloud module
  (§11 Phase 7) early?

## Decision

Deliver sharing through a **portable file-based library** first; treat
firm-wide sync as a later, opt-in capability layered on the same format.

- Templates and notes libraries use a **portable JSON/XML format** (spec §7) for
  backup, versioning, and sharing — checkable into a firm's own VCS or shared
  drive.
- **Single-workstation** use needs no infrastructure: import/define once, reuse
  across projects locally.
- **Firm-wide sharing** in the near term is "point AADT at a shared library
  directory" (network share / synced folder). No AADT-hosted cloud required.
- Licensing: per-seat for the application; template/notes libraries are the
  firm's own data and are not separately licensed. A hosted firm-wide **sync**
  service is deferred to the cloud track (M8) as an opt-in add-on.

## Consequences

- Firms get shareable, version-controllable libraries immediately with zero
  backend.
- Real-time multi-user library editing waits for M8; the file format is designed
  to migrate into that service without a redefinition.

## Alternatives considered

- **Build firm-wide sync now:** pulls the deferred cloud module (§11 Phase 7)
  into the local-first timeline, against the spec's non-goals for v1.
