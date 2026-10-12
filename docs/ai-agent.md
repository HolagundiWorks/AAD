# Local AI agent

A small offline language model (Qwen3-1.7B) can interpret a request such as "audit and repair this
drawing" and propose a call to one of a fixed set of CAD tools. The model never draws, repairs or
calculates. It only chooses a tool and fills in its arguments; deterministic engine commands do the
work.

## Status

| Part | State |
|---|---|
| Tool registry, strict parsing, schema validation, confirmation policy, loop limits, audit log (`crates/agent`) | **Implemented and tested** (13 unit tests) |
| Tools: `get_drawing_summary`, `flatten_drawing`, `audit_drawing`, `purge_unused`, `recover_file`, `set_drawing_units`, `set_display_units`, `set_plot_configuration`, `validate_plot_layout`, `analyze_annotation_collisions`, `preview_layout_changes`, `calculate_annotation_layout`, `apply_layout_changes` | **Implemented and tested**, each one an existing engine command |
| Qwen3 chat format, model-file discovery and checks, generation settings (`crates/llm`) | **Implemented and tested** (7 tests, no model needed) |
| Assistant conversation state machine: tool list prompt, strict parse, one retry, confirmation, step and history limits (`agent::Assistant`) | **Implemented and tested** with scripted model replies (8 tests) |
| llama.cpp runtime (`cadcraft-llm` with feature `llama`) | **Builds and runs** (2026-10-12, LLVM 23.1.3 from `winget install LLVM.LLVM`, `LIBCLANG_PATH=C:\Program Files\LLVM\bin`, CMake, MSVC). Tests and `clippy -D warnings` pass with the feature. The model loads in about 1 s (memory-mapped) and generates about 17 tokens per second on this machine's CPU (no GPU backend) |
| Model on this machine | `Qwen3-1.7B-Q4_K_M.gguf`, 1 107 409 472 bytes, from `unsloth/Qwen3-1.7B-GGUF`, SHA-256 `b139949c...a181897` (equals the hash Hugging Face publishes), in `%APPDATA%\AAD\models`. Not in the repository |
| Background worker (`cadcraft_llm::worker`): loads on the first request, one request at a time, Stop, panic contained | **Implemented**; a unit test covers the no-model path |
| `driver` (`cadcraft_agent::driver`): runs the Assistant against a model for hosts that can wait | **Implemented** |
| `cadcraft-cli ask [--file f \| --sample] [--yes] [--save out.dxf] "request"` (build with `--features assistant`) | **Implemented, run with the real model** (below) |
| Desktop Assistant window (Tools > Assistant; `cadcraft --features assistant`): conversation, thinking indicator, Allow and Decline, Stop, New chat; model on the worker thread, tools through the engine on the UI thread | **Implemented, run with the real model** (below); window behaviour covered by `crates/ui-egui/tests/assistant_window.rs` with a fake backend |
| Bundling the model in the installer, and building the installer with the `assistant` feature | **Not done** |
| Result tables, entity highlighting | **Planned** |
| Retrieval of project notes and office standards (RAG) | **Planned** |
| Schedule of openings, quantity take-off tools | **Planned.** The engine commands exist (`schedule`, `openings.table`, `takeoff.*`, see [hcw-kit-port.md](hcw-kit-port.md)) but are not yet in the agent registry. The names are reserved in `agent::PLANNED` and any call to them is refused as "not implemented". |

Requests such as "Prepare a schedule of openings" and "Run the built-in quantity takeoff" therefore
cannot be carried out by the agent today, although the commands can be run directly. The agent must say so rather than answer.

## Supported requests

| Request | Tool | Confirmation |
|---|---|---|
| "Flatten the imported drawing." | `flatten_drawing` | yes (changes the drawing, one undo step) |
| "Audit this drawing." | `audit_drawing` `{}` | no (report only) |
| "Audit and repair this drawing." | `audit_drawing` `{"fix": true}` | yes |
| "Purge unused definitions." | `purge_unused` | yes (removes definitions, one undo step) |
| "Recover this damaged file." | `recover_file` `{"path": ...}` | no (opens a repaired copy; the file is not changed) |
| "What is in this drawing?" | `get_drawing_summary` | no |

## Tool schemas

Offered to the model in the function-calling format of its chat template
(`agent::function_schemas()`). Arguments are checked against these schemas with
`additionalProperties: false`; an unknown or mistyped argument rejects the call.

```json
{"name": "get_drawing_summary", "parameters": {"type": "object", "properties": {}, "additionalProperties": false}}
{"name": "flatten_drawing",     "parameters": {"type": "object", "properties": {}, "additionalProperties": false}}
{"name": "audit_drawing",       "parameters": {"type": "object", "properties": {"fix": {"type": "boolean"}}, "additionalProperties": false}}
{"name": "purge_unused",        "parameters": {"type": "object", "properties": {
    "type": {"type": "string", "enum": ["all","blocks","dimstyles","groups","layers","linetypes","mleaderstyles","tablestyles","textstyles","zerolength","emptytext"]},
    "names": {"type": "string", "maxLength": 256}}, "additionalProperties": false}}
{"name": "recover_file",        "parameters": {"type": "object", "properties": {
    "path": {"type": "string", "minLength": 1, "maxLength": 1024}}, "required": ["path"], "additionalProperties": false}}
```

Results are structured JSON (`{"tool", "status", "data_is_untrusted": true, "data"}`), never prose
to be parsed. `data` can contain text from the drawing, which is why it is marked untrusted.

## Execution sequence

1. The application resolves the open drawing and any selection itself; the model does not guess.
2. It sends the system instruction (`agent::SYSTEM_PROMPT`), the user request and the tool schemas.
3. The reply is parsed by `parse_tool_call`: exactly one `<tool_call>{"name","arguments"}</tool_call>`
   block (Qwen's format) or a bare JSON object. Prose that mentions a tool, several calls in one
   reply, extra keys, malformed JSON and oversized replies are all refused.
4. `Agent::propose` refuses an unknown tool, a planned (unbuilt) tool, arguments that fail the
   schema, more than `max_calls` (3) calls per user request, and an identical repeat of the
   previous call.
5. A call that changes the drawing returns `NeedsConfirmation` with a one-line summary. The
   application shows it and, only if the user agrees to exactly that call, proposes it again with
   `confirmed = true`. Nothing runs before then.
6. The engine command runs; the result (or error) goes back to the model, which writes a short
   summary from it. Tables are rendered from the structured result by the application.
7. Every proposal, including refusals, is appended to the audit log with the request number, tool,
   arguments, status and elapsed time. Drawing contents are not logged beyond the arguments.

## Permissions

| Risk | Meaning | Confirmation |
|---|---|---|
| `Read` | reads the drawing | no |
| `Open` | opens another drawing; nothing existing changes | no |
| `Mutating` | changes the open drawing | yes |
| `Destructive` | removes definitions from the open drawing | yes |

The model cannot run shell commands, scripts, SQL, file access or network requests; the registry is
the whole of its reach. Text found in drawings, imported files and tool results is data and cannot
create a call, because calls are only taken from the model's own reply.

## Error handling

| Situation | Result |
|---|---|
| Unparseable or unknown call | `Rejected`, nothing runs, logged |
| Planned tool | `Rejected: ... is not implemented yet` |
| Invalid arguments | `Rejected` with the offending argument named |
| Over the call limit or repeated call | `Rejected`, the loop stops |
| Tool error (for example a file that cannot be read) | `Failed` with the engine's message |

## Model runtime (planned)

Values to benchmark when the adapter is built, from the Qwen3 model card and tuned for tool routing:
Qwen3-1.7B (Apache-2.0, pin an exact revision and file hash), llama.cpp with a local GGUF
(start with Q4_K_M), non-thinking mode (`enable_thinking=false`, verify it is enforced by the
pinned build's chat template), temperature 0.2, top_p 0.8, top_k 20, min_p 0, 8192 tokens of
context, at most 512 new tokens for tool routing and 1200 for a summary. These are in
`agent::GenerationProfile`. The runtime must stay local: loopback only, no external AI API, no web
dependency after installation. The adapter must be replaceable without changing the tool API.

## Tests

`cargo test -p cadcraft-agent` covers: parsing (valid blocks, prose, malformed JSON, extra keys,
multiple calls, oversize), schema validation, that only built tools are offered, refusal of unknown
and planned tools, confirmation before changes and one undo step after, audit with and without
`fix`, purge, argument rejection, the repeat and call limits, text in a drawing never becoming a
call, `recover_file` leaving the file untouched, and the audit log.

## Units and layout requests

| Request | Tool | Confirmation |
|---|---|---|
| "Set drawing units to millimetres and display dimensions in metres." | `set_drawing_units` `{units: mm, mode: metadata}` then `set_display_units` `{units: m}` | yes |
| "Rescale the drawing to metres." | `set_drawing_units` `{units: m, mode: scale}` | yes, destructive |
| "Prepare this drawing for printing at 1:100 on A1." | `set_plot_configuration` then `validate_plot_layout` | yes (configuration) |
| "Find all text overlapping walls or dimension lines." | `analyze_annotation_collisions` | no |
| "Preview and correct all annotation conflicts." / "Resolve annotation overlaps without moving the walls." | `preview_layout_changes`, then `apply_layout_changes` | yes; destructive with `includeManual` |

"Automatically adjust all dimension text and arrows" is `set_plot_configuration`. "Arrange the
opening schedule with consistent spacing" cannot be done: there is no opening schedule feature. The
model never chooses `mode` for it: the schema requires it, and a request that does not say whether
to rescale is answered with a question. Details: [units-annotation.md](units-annotation.md).

## What was run with the real model (2026-10-12, one machine, CPU, Qwen3-1.7B Q4_K_M)

Observed, not benchmarked: one run each, so these are examples, not rates.

| Request | What the model did | Result |
|---|---|---|
| "What is in this drawing?" | called `get_drawing_summary` | the tool ran; the summary text was mostly right but added things that were not in the tool result ("a complex polyline structure", "the drawing is clean and has no errors") and counted a circle twice. About 30 s |
| "Audit and repair this drawing." | proposed `audit_drawing` with repair | asked for confirmation; answered **no**; nothing ran, the model reported it had been declined |
| "Purge unused definitions." with `--yes` | proposed `purge_unused` | confirmation shown, allowed; 17 linetypes were removed through the engine; the model's summary listed them. About 24 s |
| "Prepare a schedule of openings." | no tool call | refused as unsupported, nothing ran. About 8 s |
| "Audit this drawing." (desktop window) | proposed `audit_drawing` **with repair** | a read-only request got a changing proposal; the confirmation step is what stopped it. Whether the model does this consistently was not measured |

Conclusions that hold: tool selection and refusal behaved sensibly on these five requests; the
confirmation gate and the "model never changes the drawing itself" rule work. Conclusions that do
not hold yet: reliability across many requests (not measured), and the summaries cannot be trusted
for details, so the application should show tool results (tables) rather than rely on the model's
retelling of them (planned).

## Asking from the command line: `/`

A command-line entry that starts with `/` is a request for the assistant, not a command:
`/audit this drawing` opens the Assistant window and asks it. Spaces typed after the `/` are part of
the request (Space normally submits the command line, as in AutoCAD). It is not taken while a
command is waiting for input, so a `/` typed as a prompt answer stays an answer. In a build without
the model, the command line says the assistant is not included. While the assistant is working its
input box stays usable (the text waits for the answer to finish), so typing never falls through to
the command line. Tests: `crates/ui-egui/tests/assistant_window.rs`.
