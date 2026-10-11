use thiserror::Error;

pub type Result<T> = std::result::Result<T, AormsError>;

#[derive(Debug, Clone, Error, PartialEq)]
pub enum AormsError {
    /// Could not reach AORMS (offline, DNS, timeout). Work is kept; try again later.
    #[error("cannot reach AORMS: {0}")]
    Network(String),
    /// AORMS answered with an error: `{ "error": { "code", "message", "reason"?, "retryAfterSeconds"? } }`.
    #[error("{message}")]
    Api { status: u16, code: String, message: String, reason: Option<String>, retry_after: Option<u64> },
    /// The answer was not what the contract says.
    #[error("unexpected answer from AORMS: {0}")]
    BadResponse(String),
    /// The server speaks another contract version.
    #[error("this AORMS server uses contract `{0}`; update AAD")]
    Contract(String),
    /// Signed out (no stored sign-in, or it was refused).
    #[error("not signed in to AORMS")]
    SignedOut,
    /// A response or request was larger than the safety cap.
    #[error("answer too large")]
    TooLarge,
    /// Built without network support (the web build).
    #[error("AORMS is not available in this build")]
    Unavailable,
}

impl AormsError {
    pub fn code(&self) -> &str {
        match self {
            AormsError::Api { code, .. } => code,
            AormsError::Network(_) => "network",
            AormsError::BadResponse(_) => "bad_response",
            AormsError::Contract(_) => "contract",
            AormsError::SignedOut => "signed_out",
            AormsError::TooLarge => "too_large",
            AormsError::Unavailable => "unavailable",
        }
    }
    pub fn status(&self) -> Option<u16> {
        match self {
            AormsError::Api { status, .. } => Some(*status),
            _ => None,
        }
    }
    /// This app's session was replaced by another sign-in of the same app (409).
    pub fn is_session_replaced(&self) -> bool {
        self.code() == "session_replaced"
    }
    /// Connected mode isn't available for the studio (`reason`: PLAN_NOT_CONNECTED | EXPIRED | NO_STUDIO).
    pub fn is_not_connected(&self) -> bool {
        self.code() == "not_connected"
    }
    /// Parse `{ "error": {...} }`; anything else becomes a generic error for the status.
    pub(crate) fn from_body(status: u16, body: &[u8]) -> AormsError {
        let v: serde_json::Value = serde_json::from_slice(body).unwrap_or(serde_json::Value::Null);
        let e = v.get("error");
        let text = |k: &str| e.and_then(|e| e.get(k)).and_then(serde_json::Value::as_str).map(str::to_string);
        let unavailable = status >= 500;
        AormsError::Api {
            status,
            code: text("code").unwrap_or_else(|| if unavailable { "unavailable" } else { "error" }.to_string()),
            message: text("message").unwrap_or_else(|| if unavailable { "AORMS isn't available right now." } else { "Request failed." }.to_string()),
            reason: text("reason"),
            retry_after: e.and_then(|e| e.get("retryAfterSeconds")).and_then(serde_json::Value::as_u64),
        }
    }
}
