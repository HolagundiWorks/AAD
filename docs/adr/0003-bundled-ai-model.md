# ADR-0003 — Default local AI model & delivery

- **Status:** Accepted (M4; Foundry Local, 2026-07)
- **Context:** Spec §3.1 / §11 #3 — default bundled model size/quality tradeoff
  vs. a first-run download. AADT runs AI **locally, on demand**. Deterministic
  drafting/takeoff must never depend on the model being present.

## Decision

- **Runtime (inbuilt):** **[Microsoft Foundry Local](https://learn.microsoft.com/azure/ai-foundry/foundry-local/)**
  via its OpenAI-compatible Chat Completions API on localhost
  ([`FoundryLocalClient`](../../crates/aadt-ai/src/foundry.rs)). Default endpoint
  `http://127.0.0.1:5273`, overridable with `AADT_FOUNDRY_ENDPOINT`.
- **Default model:** **`phi-3.5-mini`** (catalog alias) — quantized ~2–3B class,
  CPU/NPU/GPU via Foundry Local / WinML. Suited to JSON-only classification over
  structured detection facts.
- **Delivery:** first-run download through Foundry Local’s model catalog (not
  bundled in the AADT installer). Firms may pre-provision models offline.
- **Alternate backend:** Ollama remains supported (`aadt infer --backend ollama`)
  for environments that already run `ollama serve`.
- **Lifecycle:** no background AI thread in AADT. The Foundry Local service is
  external; AADT only opens short HTTP requests. Warm/unload policy for an
  AADT-owned process still lives in `aadt_ai::lifecycle` for optional hosts.
- **Classification workflow:** Propose → Review → Freeze
  ([ADR-0007](0007-propose-review-freeze.md)) — AI writes staged attributes only;
  freeze is an explicit contractual lock.

## Consequences

- First AI use needs Foundry Local installed and a model loaded (or a reachable
  OpenAI-compatible local endpoint). Deterministic takeoff never waits on it.
- Model choice is configurable (`--model`, firm defaults); `phi-3.5-mini` is the floor.

## Alternatives considered

- **Ollama as sole runtime:** worked for M4 prototype; kept as `--backend ollama`.
- **Bundle weights in the installer:** simplest offline story, but bloats every
  install and couples model updates to app releases.
- **Cloud OpenAI:** rejected — product is local-first; no API keys in the default path.
