# ADR-0003 — Default local AI model & delivery

- **Status:** Proposed (revisit at M4)
- **Context:** Spec §3.1 / §11 #3 — default bundled model size/quality tradeoff
  vs. a first-run download. AADT runs AI **locally, on demand** via an Ollama
  subprocess (spec §6.1), and must be CPU-viable on architect workstations
  without GPUs.

## Decision

- **Runtime:** Ollama as an on-demand subprocess (spec §3.1). No background
  service; spawn on first AADT AI action, keep warm ~2–3 min, then unload.
- **Default model:** a quantized ~2–3B model — **Qwen2.5-3B** or **Phi-3.5-mini**
  (~2 GB, CPU-viable). The AI layer's job is classification/narrative/formatting
  over deterministic data, which small models handle when prompted for
  **JSON-only** output.
- **Delivery:** **first-run download** rather than bundling the weights in the
  installer. Install size is not a constraint (spec §2, principle 6), but a
  first-run fetch keeps the base installer lean, lets the model be updated
  independently, and lets firms pre-provision or swap models. Offline
  installs get an optional bundled-model variant.

## Consequences

- First AI use requires a one-time download (or a pre-provisioned model);
  deterministic drafting/takeoff never depend on it (spec §8, graceful
  degradation).
- Model choice is configurable per firm; the default is a floor, not a ceiling.

## Alternatives considered

- **Bundle weights in the installer:** simplest offline story, but bloats every
  install and couples model updates to app releases.
