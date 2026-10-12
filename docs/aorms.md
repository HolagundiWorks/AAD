# AORMS connection

AAD can sign in to AORMS (the office-management web portal) and pull project information into a drawing. It is optional: signed out, AAD is a complete standalone drafting program. The wire contract and the rules shared with the other AORMS desktop app (AQC) are in `docs/esti/AORMS-SUITE-SIGNIN.md` in the AORMS repository (contract `2026-10-aqc-1`).

## In the interface

* **AORMS menu** (Sign in, Sign out, Status, Projects, Pull project info, Refresh project info, Drawings) — chosen from the menu or typed with no parameters, a command opens the **AORMS dialog**; the same command with JSON parameters (`app.run`, scripts, MCP, control channel) runs the engine command and never opens a dialog.
* **Dialog:** signed out — e-mail, password (masked), a collapsible *Server* field (https, or http to this computer), *Sign in* (Enter also submits), Esc closes; errors are explained in words (wrong password, not linked, pending, rate limit with seconds, plan not connected with the reason, offline). Signed in — studio, e-mail · role · plan, the studio's project list with search, *Pull project info*, *Refresh* for the project this drawing is linked to, *Reload list*, *Sign out*. Signed in on this computer (another AORMS app's login) but AAD has no session — *Connect AAD* (never silent: it replaces another computer's AAD session).
* **Status-bar chip** left of the units button: `AORMS` (Community, not signed in) / `AORMS · <studio>` (connected) / `AORMS · !` (signed in, not connected) / `AORMS …` (working). Click opens the dialog.
* **No freezing:** every network call runs on a worker thread (`crates/ui-egui/src/aorms_ui.rs`): the job takes the client out of the session, runs, and hands it back with the result; the drawing is only touched on the UI thread, through the `aorms.apply` command, so a pull is **one undo step** and changes only project details and floors, never geometry.
* **Preferences** save the e-mail and server only. The password is cleared from the text field the moment the sign-in starts and is never stored, logged or saved; no token is ever in the preferences (the refresh token lives in the OS credential store).
* **Start-up:** the desktop app silently restores the shared sign-in once (`AAD_NO_AORMS=1` turns that off). It never takes a session over.

## Commands


Crate `aad-aorms` (layer 3, native only; the web build compiles it to "not available") and the engine commands in `crates/engine/src/cmd/aorms.rs`.

| Command | Does |
|---|---|
| `aorms.signin {email, password, baseUrl?}` | Password sign-in, then this app's session (`app: "aad"`; one active session per user per app, so AAD and AQC do not replace each other). `baseUrl` must be https, or http to this computer; default `https://aorms.in` or `$AORMS_URL`. |
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

The AORMS login is the licence: a studio on the Studio plan or higher, not expired, gets connected mode. Anything else gets a clear reason from `aorms.signin` and AAD carries on unchanged. The check is made by the server; AAD is open source, so a client-side check could be removed.

## Shared sign-in

The refresh token is kept in one OS credential shared with other AORMS desktop apps: target `AORMS/Connect`, blob `{"v":1,"refreshToken","email","updatedAt"}` (`updatedAt` Unix seconds). Each app keeps only its own session id in its own entry (`AORMS/Connect/aad`). Refresh tokens rotate, so before refreshing the client re-reads the shared credential, writes the rotated token back immediately, and on a refusal retries once if the shared token changed in the meantime (implemented and unit-tested).

The OS store (`KeyringStore`) is behind the cargo feature `keyring-store` (engine feature `aorms-keyring`); the desktop app turns it on, library users and CI do not (then the sign-in is kept in memory only and is lost when AAD closes). **Not yet verified:** the credential store itself on Windows, and two apps refreshing within seconds of each other.

## Tested

* `cargo test -p aad-aorms` (25 tests: contract handling, restore without stealing a session, rotation, busy/offline/refused refresh, hostile answers, mapping) and `cargo test -p aad-engine aorms` (12 tests: commands against a scripted server, no geometry changed, hostile parameters, signed-out behaviour).
* `cargo test -p aad-ui-egui --test aorms_ui` (15 tests: sign-in off the UI thread with a slow server, password leaves the field at once, errors in words, pull = one undo step and no geometry change, hostile ids refused, menu vs JSON routing, the dialog drawn in every state, chip states, preferences hold no secret, start-up restore silent when nothing is stored).
* `crates/engine/examples/aorms_smoke.rs` runs the commands against a real server: signed in as `aad`, listed projects, pulled a bound project's details and floors into a scratch drawing, signed out (2026-10-11, a local AORMS dev server).

**Not verified:** how the dialog and chip look on a real display (they were exercised through the egui test harness, not by eye), and the Windows credential store.

## Not built yet

Saving a drawing to the portal; publishing the take-off for AQC (`aqcexport` file and its portal upload); downloading drawings. The `hcw-aqc-bridge` writer is the last item in [hcw-kit-port.md](hcw-kit-port.md).
