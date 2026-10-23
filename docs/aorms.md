# AORMS connection

O-Draft can sign in to AORMS (the office-management web portal) and pull project information into a drawing through engine commands. It is optional: without a session, O-Draft is a complete standalone drafting program. The wire contract and the rules shared with the other AORMS desktop app (AQC) are in `docs/esti/AORMS-SUITE-SIGNIN.md` in the AORMS repository (contract `2026-10-aqc-1`).

## In the interface

**Removed.** There is no AORMS menu, sign-in dialog, status-bar chip or silent start-up restore in WinUI or egui. Use `engine.execute` / scripts / MCP / the control channel for `aorms.*` if needed.

## Commands


Crate `aad-aorms` (layer 3, native only; the web build compiles it to "not available") and the engine commands in `crates/engine/src/cmd/aorms.rs`.

| Command | Does |
|---|---|
| `aorms.signin {email, password, baseUrl?}` | Password sign-in, then this app's session (`app: "aad"`; one active session per user per app, so O-Draft and AQC do not replace each other). `baseUrl` must be https, or http to this computer; default `https://aorms.in` or `$AORMS_URL`. |
| `aorms.restore` | Silent start-up from the shared sign-in. Never takes a session over. |
| `aorms.signout` | Ends the shared sign-in (the other AORMS app finds it empty and asks to sign in). |
| `aorms.status` | Signed in / connected, studio, role, plan, and the project this drawing is linked to. |
| `aorms.projects` | The studio's projects. |
| `aorms.open {projectOfficeId}` | Pulls the project's details (title, owner, architect, address, project id) and, when the project's take-off holds levels, the floors (floor-to-floor, slab, beam depth; ceiling and lintel are kept or derived). Links the drawing to the project. |
| `aorms.pull` | The same, for the linked project. |
| `aorms.drawings` | Lists the project's current drawings on the portal. |
| `aorms.apply {projectOfficeId, seed, settings}` | The drawing half of a pull whose network half ran elsewhere (the dialog fetches on a worker thread); one undo step. |

Rules: portal data reaches the drawing only on `aorms.open` / `aorms.pull`, only project details and floors, never drawn geometry; the password is used once and never stored, logged or returned; every server answer is treated as untrusted (size caps, defensive parsing, no panics).

## Licence

The AORMS login is the licence: a studio on the Studio plan or higher, not expired, gets connected mode. Anything else gets a clear reason from `aorms.signin` and O-Draft carries on unchanged. The check is made by the server; O-Draft is open source, so a client-side check could be removed.

## Shared sign-in

The refresh token is kept in one OS credential shared with other AORMS desktop apps: target `AORMS/Connect`, blob `{"v":1,"refreshToken","email","updatedAt"}` (`updatedAt` Unix seconds). Each app keeps only its own session id in its own entry (`AORMS/Connect/aad`). Refresh tokens rotate, so before refreshing the client re-reads the shared credential, writes the rotated token back immediately, and on a refusal retries once if the shared token changed in the meantime (implemented and unit-tested).

The OS store (`KeyringStore`) is behind the cargo feature `keyring-store` (engine feature `aorms-keyring`); the desktop app turns it on, library users and CI do not (then the sign-in is kept in memory only and is lost when O-Draft closes). **Not yet verified:** the credential store itself on Windows, and two apps refreshing within seconds of each other.

## Tested

* `cargo test -p aad-aorms` (25 tests: contract handling, restore without stealing a session, rotation, busy/offline/refused refresh, hostile answers, mapping) and `cargo test -p aad-engine aorms` (12 tests: commands against a scripted server, no geometry changed, hostile parameters, signed-out behaviour).
* `cargo test -p aad-ui-egui --test aorms_ui` (15 tests: engine helpers still work; connect UI is gone — no AORMS menu, menu/typed commands do not open a dialog; JSON `aorms.*` still runs; password leaves the field at once on a scripted sign-in; preferences hold no secret).
* `crates/engine/examples/aorms_smoke.rs` runs the commands against a real server: signed in as `aad`, listed projects, pulled a bound project's details and floors into a scratch drawing, signed out (2026-10-11, a local AORMS dev server).

**Not verified:** the Windows credential store against a real shared login with another AORMS desktop app.

## Not built yet

Saving a drawing to the portal; publishing the take-off for AQC (`aqcexport` file and its portal upload); downloading drawings. The `hcw-aqc-bridge` writer is the last item in [hcw-kit-port.md](hcw-kit-port.md).
