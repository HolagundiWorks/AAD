# Suite integration: what AAD has, what the contracts need

This page maps AAD onto a four-application suite (AAD drawing, AOS office management, AQC
quantities and costing, ShilpiDB drawing database) with one shared AI runtime and shared project
identity. It describes AAD's side only. The AOS, AQC and ShilpiDB repositories have not been audited
for this page: where it names their behaviour, the source is an AAD document or AAD code, and each
such statement is marked.

Nothing here is a claim that a cross-application workflow exists. Section 5 lists what is built, what
is missing and what the first milestone needs.

## 1. Ownership

| Data | System of record | AAD's part |
|---|---|---|
| Drawing geometry, layers, blocks, layouts, undo, DXF/DWG | AAD (`aad-doc`, `aad-engine`) | Owns it. Other applications read it through commands, never through the document model |
| Project identity, studio, client, fees | AOS / AORMS portal (not audited) | Pulls project details and floors on request (`aorms.open`, `aorms.pull`); never writes to the portal |
| Quantities and costs | AQC (not audited) | Produces measured quantities (`takeoff.*`) and a hand-off file (`bridge.export`); does not price |
| Reusable details and spatial records | ShilpiDB | Exports a drawing's entities to `.vdb`, queries it, imports it, and can push to or pull from a running `shilpid` (`vdb.*`) |
| AI conversation, tool permissions, audit | Shared AI runtime (not built) | AAD has its own tool registry and audit log (`aad-agent`) for its own tools |

Rules AAD already follows and the suite should keep: portal data reaches the drawing only as project
details and floors, one undo step, never as geometry ([aorms.md](../aorms.md)); a hand-off file never
changes the drawing it came from; imported text and tool results are data, not instructions
([ai-agent.md](../ai-agent.md)).

## 2. What exists in AAD

| Capability | Where | State |
|---|---|---|
| Every action is a command with a JSON form, usable without a window | `crates/engine/src/cmd/*`, `Session::execute` | Implemented. This is the seam the suite should call |
| Control channel (loopback JSON lines) | `aad-host`, [control-protocol.md](../control-protocol.md) | Implemented: `engine.execute`, `cmdline.*`, `drawing.inspect`, `app.open`, `app.save` |
| MCP server over the same commands | `crates/mcp`, [mcp.md](../mcp.md) | Implemented |
| Typed agent tools with schema validation, risk classes, confirmation and an audit log | `crates/agent` | Implemented for AAD's own tools. Quantity take-off tools are reserved but not registered (`agent::PLANNED`) |
| AORMS sign-in shared with AQC (contract `2026-10-aqc-1`), project pull | `crates/aorms`, `engine/src/cmd/aorms.rs` | Implemented and unit-tested against a scripted server. The credential store on Windows and two apps refreshing at once are **not verified** |
| Quantity take-off from drawn geometry, rates, bill, opening schedule | `takeoff.*`, `schedule`, `openings.*` ([hcw-kit-port.md](../hcw-kit-port.md)) | Implemented; reads `MEASURE-*` layers |
| Hand-off to and from AQC | `bridge.export`, `bridge.import`; file `hcw-aqc-bridge` version 1, millimetres, beside the drawing as `name.aqc.json` | Implemented. Import previews first and never changes geometry. Rooms are not exported |
| Spatial index and ShilpiDB exchange | `aad-vdb`, `vdb.*` | Implemented; see [aadt-shilpidb-architecture.md](aadt-shilpidb-architecture.md) |
| Spatial AI tools (overview, window, nearest, describe) | `crates/agent` | Implemented |
| Drawing-level change record (revision, checksum) | `revision.rs`, `drawing.revision`, `<file>.aadrev.json` | Implemented |
| Stable cross-application project and artifact references | none | **Missing**: the only shared key is the AORMS project office id stored in the drawing's project data |

## 3. Contracts the suite needs from AAD

These are proposals. Each is a thin layer over commands that exist; none requires the shared
runtime to touch the document model.

| Contract | Proposed content | Built on |
|---|---|---|
| `DrawingRef` | project id, drawing id, path, DXF/DWG format, byte size | `Session` document state, `aorms.open` link |
| `DrawingRevision` | `DrawingRef` + SHA-256 of the saved bytes + saved time + entity count | **new**: computed at save in `file.rs` (`commit_save`) and stored beside the file, not in the DXF |
| `QuantityTakeoff` | the saved `takeoff.*` results: item, group, length or area or count, unit, source layer, handles, rounding rule | `takeoff.list`, `bridge.export` |
| `AuditEvent` | request number, tool, arguments, outcome, drawing revision before and after | `agent::AuditEntry`, extended with the revision |

An estimate built from a take-off must record the `DrawingRevision` it used, the measurement rules
(metric rounding to 1 cm, imperial to 1/8 in, deduction layers) and the rate schedule. AAD can
supply the first two; the third belongs to AQC.

## 4. The AI boundary

AAD's rule is that the model proposes and deterministic commands calculate ([ai-agent.md](../ai-agent.md)).
For a shared runtime that means:

- AAD registers its tools under a `cad.*` namespace with the schemas it already validates. The
  registry already declares a risk class and whether a call needs confirmation; a shared runtime
  should read those, not replace them.
- Write tools stay behind `Session::execute`, so undo, the locked-layer rules and the
  panic guard (`guard.rs`) apply to every caller.
- Offline mode: AAD's assistant runs a local model (`aad-llm`, llama.cpp) and sends no drawing
  content anywhere. A hosted model is not wired in and nothing in AAD requires one.
- The runtime must not get raw document access, arbitrary paths or SQL. The control channel and MCP
  already limit callers to named commands; the suite should use them as they are.

## 5. First milestone against AAD

Milestone 1 is "drawing to draft estimate". AAD's side, step by step:

| Step | AAD today | Gap |
|---|---|---|
| Open the project's linked drawing | `app.open`, `aorms.open` | A launcher passing a project id to AAD; none exists |
| Resolve the same project in AQC | the AORMS office id is stored in the drawing | AQC side not audited |
| Extract measurements | `takeoff.start` and `takeoff.*` over `MEASURE-*` layers | Needs a stated list of supported geometry; unsupported objects must be reported, not skipped silently (today `takeoff` reads the measure layers only) |
| Pass them on | `bridge.export` | Done: carries the drawing revision and rooms (AQC side not verified) |
| Calculate quantities and cost | AQC (not audited) | Not AAD's part |
| Draft in AOS | AOS (not audited) | Not AAD's part |
| Reject unauthorised writes, keep an audit trail | agent confirmation policy and audit log | Audit log is in memory per session; not persisted or shared |

## 6. Order of work in AAD

1. `DrawingRevision` at save, written beside the file; checked by `drawing.revision`. **Done.**
2. Rooms and the revision in `bridge.export`, as additive keys of format version 1. **Done.** Whether AQC accepts the extra keys is not verified.
3. Register the existing `takeoff.*`, `schedule` and `room.log` commands as agent tools (read-only
   ones first), with schemas, in the existing registry.
4. Persist the audit log next to the drawing revision it refers to.
5. A project-id launch argument for `aad`, so a launcher can open the linked drawing.

Each step lands with tests, and `docs/status.md` changes only when it exists.

## 7. Not decided here

The shared repository layout, the IPC protocol between applications, the shared identity and role
model, the indexing service and the installer coordination belong to a specification that covers all
four repositories. They need an audit of AOS, AQC and ShilpiDB first.
