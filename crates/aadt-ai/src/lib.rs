//! # aadt-ai — AI Orchestration Layer (flagship runtime discipline, spec §6)
//!
//! On-demand, local AI. No model runs in the background. AADT AI actions are
//! invoked explicitly, extract **structured data** (never raw geometry or images)
//! from a selection, send a structured-JSON prompt to a local Ollama subprocess at
//! `localhost:11434`, apply the JSON response as a normal undoable CAD operation,
//! then let the model unload after a short warm-idle window (~2–3 min).
//!
//! Responsibilities (spec §6.1–§6.3):
//! - Ollama subprocess lifecycle (spawn-on-demand, warm-idle, auto-unload).
//! - Structured JSON request/response contracts for wall-type inference (§5.1 step
//!   6), anomaly flagging and schedule formatting (§5.2), notes suggestion (§5.3).
//! - Graceful degradation: if Ollama is unavailable/slow, AI actions fail with a
//!   command-line message and never block deterministic drafting or takeoff.
//! - Per-invocation **audit log** of exactly what structured data was sent (§6.3).
//!
//! **Status:** skeleton. Real implementation lands in **M4 — AI Orchestration
//! Layer** (see `docs/ROADMAP.md`).

/// Local Ollama endpoint AADT talks to (spec §6.1).
pub const OLLAMA_ENDPOINT: &str = "http://localhost:11434";

/// Warm-idle window, in seconds, before the model unloads (spec §6.1 step 6).
pub const WARM_IDLE_SECS: u64 = 150;
