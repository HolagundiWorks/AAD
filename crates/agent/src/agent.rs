//! The bounded tool-call loop: validation, confirmation, loop limits and the audit log.

use std::time::Instant;

use aad_engine::Session;
use serde_json::{Value, json};

use crate::parse::ToolCall;
use crate::registry::{PLANNED, Risk, find};
use crate::schema::validate;

/// What became of a proposed call.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The tool ran. `result` is data for the model and the UI, never an instruction.
    Done { tool: String, result: Value },
    /// The call is valid but changes the drawing: ask the user, then propose it again with
    /// `confirmed = true`. Nothing ran.
    NeedsConfirmation { tool: String, risk: Risk, summary: String },
    /// The call was refused and nothing ran.
    Rejected { reason: String },
    /// The tool ran and reported an error.
    Failed { tool: String, error: String },
}

/// One line of the audit trail.
#[derive(Clone, Debug, PartialEq)]
pub struct AuditEntry {
    pub request: u64,
    pub tool: String,
    pub arguments: Value,
    /// `done`, `failed`, `needs-confirmation` or `rejected: …`.
    pub status: String,
    pub elapsed_ms: u128,
}

/// The agent's per-user-request state. Call [`Agent::begin_request`] for each user message, then
/// [`Agent::propose`] for each call the model proposes.
#[derive(Debug)]
pub struct Agent {
    /// Most tool calls that run for one user request.
    pub max_calls: usize,
    pub log: Vec<AuditEntry>,
    request: u64,
    calls: usize,
    last: Option<String>,
}

impl Default for Agent {
    fn default() -> Self {
        Agent { max_calls: 3, log: Vec::new(), request: 0, calls: 0, last: None }
    }
}

impl Agent {
    /// Start a new user request: the call counter and the repeat guard reset.
    pub fn begin_request(&mut self) {
        self.request += 1;
        self.calls = 0;
        self.last = None;
    }

    fn record(&mut self, call: &ToolCall, status: impl Into<String>, started: Instant) {
        self.log.push(AuditEntry {
            request: self.request,
            tool: call.name.clone(),
            arguments: Value::Object(call.arguments.clone()),
            status: status.into(),
            elapsed_ms: started.elapsed().as_millis(),
        });
    }

    fn reject(&mut self, call: &ToolCall, reason: impl Into<String>, started: Instant) -> Outcome {
        let reason = reason.into();
        self.record(call, format!("rejected: {reason}"), started);
        Outcome::Rejected { reason }
    }

    /// Validate and, if allowed, run a proposed call against `session`.
    ///
    /// Refused: an unknown or unbuilt tool, arguments that fail the schema, a call past the
    /// per-request limit, and an identical repeat of the previous call. A call that changes the
    /// drawing returns [`Outcome::NeedsConfirmation`] unless `confirmed` is true; the application
    /// sets `confirmed` only after the user agreed to exactly that call.
    pub fn propose(&mut self, session: &mut Session, call: &ToolCall, confirmed: bool) -> Outcome {
        let started = Instant::now();
        if self.calls >= self.max_calls {
            return self.reject(call, format!("limit of {} tool calls per request reached", self.max_calls), started);
        }
        if PLANNED.contains(&call.name.as_str()) {
            return self.reject(call, format!("`{}` is not implemented yet", call.name), started);
        }
        let Some(tool) = find(&call.name) else {
            return self.reject(call, format!("unknown tool `{}`", call.name), started);
        };
        if let Err(e) = validate(&(tool.schema)(), &Value::Object(call.arguments.clone())) {
            return self.reject(call, e, started);
        }
        let key = call.key();
        if self.last.as_deref() == Some(key.as_str()) {
            return self.reject(call, "identical to the previous call", started);
        }
        let risk = (tool.risk)(&call.arguments);
        if risk.needs_confirmation() && !confirmed {
            self.record(call, "needs-confirmation", started);
            return Outcome::NeedsConfirmation { tool: tool.name.into(), risk, summary: (tool.summary)(&call.arguments) };
        }
        self.calls += 1;
        self.last = Some(key);
        match (tool.run)(session, &call.arguments) {
            Ok(result) => {
                self.record(call, "done", started);
                Outcome::Done { tool: tool.name.into(), result }
            }
            Err(error) => {
                self.record(call, "failed", started);
                Outcome::Failed { tool: tool.name.into(), error }
            }
        }
    }
}

/// A tool result as the structured message handed back to the model. The `data` is marked
/// untrusted: it can contain text from the drawing.
pub fn tool_message(outcome: &Outcome) -> Value {
    match outcome {
        Outcome::Done { tool, result } => json!({ "tool": tool, "status": "done", "data_is_untrusted": true, "data": result }),
        Outcome::NeedsConfirmation { tool, summary, .. } => json!({ "tool": tool, "status": "needs-confirmation", "summary": summary }),
        Outcome::Rejected { reason } => json!({ "status": "rejected", "reason": reason }),
        Outcome::Failed { tool, error } => json!({ "tool": tool, "status": "failed", "error": error }),
    }
}
