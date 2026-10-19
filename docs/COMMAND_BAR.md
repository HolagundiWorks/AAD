# Unified command bar

One floating input at the bottom centre of the drawing area takes native CAD commands and requests
for the assistant. There is no assistant window, no second input and no second history: everything
typed, and everything the assistant answers, goes to the same command history (`Session::log`).
Ordinary CAD works with no assistant in the build (web) or no model file.

Code: `crates/ui-egui/src/cmdline.rs` (drawing, keys), `cmdinput.rs` (classification, suggestions),
`assistant.rs` (bridge to the `Backend`), `crates/engine/src/cmd/context.rs` (references).
Tests: `crates/ui-egui/tests/command_bar.rs`, `crates/engine/tests/context_refs.rs`, unit tests in
`cmdinput.rs` and `assistant.rs`.

## Keys

| Key | Behaviour |
| --- | --- |
| Space | Without a leading `/`: runs the command line (CAD habit; empty bar repeats the previous command). With a leading `/`: an ordinary space so you can type an assistant sentence. While a command waits for input it acts as Enter; when that prompt offers a list it picks the first match. A half-typed name may still be completed from the suggestion list (egui) before it runs. |
| Enter | Submits the line: a native command, or an assistant request when the text starts with `/` (or contains a `/reference`). On an empty bar, repeats the previous command. A request typed while the assistant is busy stays in the bar. |
| Tab | Accepts the highlighted (or first) suggestion without executing it. |
| Up / Down | Move through the suggestions while a menu is open; otherwise through the history. |
| Ctrl+Space | Opens the suggestions (an empty bar starts a `/` reference). |
| Escape | Closes the suggestions; then stops a working assistant; then clears the bar and cancels the running command. |
| y / n / Enter / Escape | Answer a pending Allow/Decline from the assistant (Enter and Escape decline). The two prompt words are also clickable. |

Text from an input method (IME composition) never triggers Space or Enter actions.

## What is a request

`cmdinput::classify` / host `bar::classify` decide: text with a leading `/`, or containing a
`/reference` word, is a request for the assistant. Everything else stays on the command line
(Space/Enter run CAD). A first word that is a command or alias runs that command
(`open /tmp/x.dxf` is a command; a path is not a reference). While a command is running, the text
is its input. Without an assistant a `/` request says the assistant is not included in this build.

## References

`/drawing /view /selection /layers /rooms /floor /schedule /project` are resolved from the live document
when the request is sent (`cmd::context::resolve_text`). Results are structured, read-only, bounded
(40 items, 3000 characters, `truncated` flagged) and scoped to the active project and floor.
A reference with nothing to read (no selection, no room labels) is dimmed in the menu with the
reason, and sending it reports the reason in the history instead of sending. Context is attached to
the request as data inside `<context>` and the closing tag is neutralised.

## Assistant flow

The model only proposes calls to allow-listed tools ([ai-agent.md](ai-agent.md)); the engine
validates and executes them, a tool that changes the drawing waits for Allow in the bar, and undo
works as for any command. Offline: the model is local.

## Look

Both themes use shared tokens (`theme.rs`): near-black chrome with white text and an orange accent
(`#f2841c`) in dark; white chrome with the same orange family in light. Toolbars and panels did not
move. The bar has a fixed width range, sits over the canvas (not part of any panel) and does not move
with pan, zoom or window size (tested in Classic and Pro).

## Known gaps

- Suggestions do not rank by usage.
- Before/after screenshots and a run with the real model through the bar are not recorded here.

## History above the bar

Two lines of earlier output show above the bar by default (`ui.cmdline.lines`, 0 to 12; drag the
top edge, or F2 or the chevron for the whole log). **Clear** in the expanded history, or Window >
Clear Command History (`ui.history.clear`), forgets the log and the commands recalled with Up.
