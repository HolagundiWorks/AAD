//! Diagnostics: every problem is tied to the node (and port) it concerns.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Code {
    MissingInput,
    InvalidConnection,
    UnitMismatch,
    UnknownNodeType,
    /// A node or component schema version this program does not implement.
    OutdatedSchema,
    OutOfRange,
    InvalidFormula,
    DivisionByZero,
    InvalidConfig,
    GeometryFailure,
    StaleRevision,
    TakeoffRule,
    Cycle,
    /// An upstream node failed, so this node has no current value.
    Blocked,
    /// Two sources compete for a value (a locked parameter with a connection).
    Conflict,
    Cancelled,
    Limit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port_id: Option<String>,
    pub message: String,
    /// For a cycle: the node ids around it, the first repeated at the end.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<String>,
}

impl Diagnostic {
    pub fn error(code: Code, node: Option<&str>, port: Option<&str>, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: Severity::Error,
            node_id: node.map(str::to_string),
            port_id: port.map(str::to_string),
            message: message.into(),
            path: vec![],
        }
    }
    pub fn warning(code: Code, node: Option<&str>, port: Option<&str>, message: impl Into<String>) -> Self {
        Diagnostic { severity: Severity::Warning, ..Diagnostic::error(code, node, port, message) }
    }
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}
