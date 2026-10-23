# Dual-channel command UI (WinUI)

WinUI exposes two input channels that share one CAD executor (`Session` / `cmdline`) and one
assistant path (`aad-host` `assistant.ask`). Do not add a second command parser or AI-only geometry.

| Channel | Where | Responsibility |
| --- | --- | --- |
| **Fixed AI prompt** | Floating bar over the drawing (`CommandBar` / `AiInput`) | Natural-language asks, slash-transferred text, Allow/Decline, session history preview |
| **Dynamic CAD prompt** | Cursor-anchored chrome (`CommandChrome` CadInput + prompt + suggestions + Length/Angle) | Commands, arguments, coordinates, distances, angles, selection; autocomplete from the engine registry |

Canonical interaction contract for the WinUI shell. Optional egui still uses a unified cmdline
(`cmdinput`); parity is a follow-up.

Code: `apps/aad-winui` (`MainWindow`, `CommandChrome`, `Native`); host `crates/aad-host` (`bar.rs`,
`ffi.rs` suggest / Space-literal / slash-transfer / ask, `dyn_chrome`).
Tests: host `bar` unit tests; egui `crates/ui-egui/tests/command_bar.rs` (unified bar).

## Slash transfer (CAD → AI)

Intercept `/` only when the CAD prompt is **idle** and eligible:

- No running command.
- Not inside unbalanced quotes.
- Not a pure text prompt.

Then:

1. Strip one leading `/` and the spaces that follow it (`slash_transfer_payload`).
2. Clear the CAD draft.
3. Move focus to `AiInput` with that payload (user may edit before Enter).
4. Do **not** run the text through CAD `cmdline`.

Example: `/ create a room 3000 x 4000 mm with a door` → AI field receives
`create a room 3000 x 4000 mm with a door`.

Do **not** intercept slash while a command is waiting for an argument, while quotes/filenames are
open, while a modal owns focus, or while already editing the AI prompt.

FFI: `aad_host_slash_transfer` (eligible?), `aad_host_slash_payload` (strip `/`), `aad_host_ask` (natural ask, no CAD classify).

## Keyboard contract

### CAD command-entry (idle or active command, focus on CadInput)

| Key | Behaviour |
| --- | --- |
| Typing | Case-insensitive autocomplete from the registry (aliases, first-match ranking). |
| Enter / Space | Complete to the first suggestion when a partial name matches (e.g. `e` → ERASE), then run. Empty idle: first Space arms, second repeats last CAD line; empty Enter repeats. While a command waits, Space is Enter. Open quote / text prompt: Space is literal. |
| Tab | Dyn separator when a point frame applies; else accept first suggestion without running; empty cycles object snaps. |
| Up / Down | Recall typed CAD lines (`_typed`); not AI asks. |
| Escape | Dismiss suggestions; then cancel command / stop assistant as today. |
| `/` (idle) | Slash transfer to AI (above). |

WinUI suppresses TextBox `CharacterReceived` for Space so activate cannot leave a space in the draft.

### AI prompt (focus on AiInput)

| Key | Behaviour |
| --- | --- |
| Enter | `aad_host_ask` with the field text (not CAD). |
| Space | Literal space. |
| `/` | Literal (no special meaning after transfer). |
| Escape | Clear the field if non-empty; otherwise cancel / stop as for CAD. |
| CAD repeat | Never (empty Enter/Space do not repeat CAD). |

## Autocomplete

Host `bar::suggestions_json`: commands (id / alias / label) or `/reference` providers. Ranking:
exact alias or id first, then shortest prefix. Suggestions never execute by appearing; Enter/Space
or click accepts. Ambiguity follows the host ranking (first row), not a second parser in C#.

## History

- Collapsed AI bar shows **exactly two** recent session-log lines; chevron expands the full log.
- Clear history empties `Session::log` and CAD Up/Down recall only; drawing unchanged.
- CAD Up/Down uses the typed-command list filled when a CAD line is submitted — AI asks are not added.
- Assistant echoes still appear in the session log on the AI bar.

## Dynamic CAD chrome

Host `dyn` JSON (when the pointer is known, or at canvas centre until the first move):

- `on`: park the CAD field beside the crosshair (independent of Dynamic Input mode).
- `prompt`: engine prompt text (`Specify next point…`).
- `dims`: Length/Angle (or draft box) when Dynamic Input is on and a dyn frame applies; otherwise false.
- `aLabel` / `a` / `bLabel` / `b` / `active`: dim fields when `dims` is true.

WinUI renders this state; it does not hard-code per-command stages. See also [UI_UX.md](UI_UX.md).

Cursor CAD chrome is ~50% size at 50% opacity and shows **only** the typed command/text (no
prompt label, unit hint, or Length/Angle readouts). Typed distances still accept SketchUp-style
units (`mm`, `cm`, `m`, `km`, `in`, `ft`, `yd`, `5'6"`) in the engine.

## AI bar placement

Fixed bottom-centre of the drawing canvas. Solid fill (no acrylic / glass); not draggable.

## Assistant safety

The model only proposes allow-listed tools ([ai-agent.md](ai-agent.md)). Destructive or
drawing-changing tools wait for Allow/Decline. Undo is the normal command undo stack. Without
`--features assistant` / a model, Ask reports that the assistant is not included.

## Known gaps

- Suggestions do not rank by usage.
- egui remains a unified bar until a follow-up.
- Full Narrator / DPI automation matrix for the dual chrome is not automated here.
