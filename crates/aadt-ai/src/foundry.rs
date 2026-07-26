//! Microsoft Foundry Local — default inbuilt AI transport.
//!
//! Talks to Foundry Local's OpenAI-compatible Chat Completions API over plain
//! HTTP (`std::net`), matching the dependency-free style of [`crate::OllamaClient`].
//! Start Foundry Local (desktop app or `foundry service start`), then point AADT
//! at its endpoint via [`FOUNDRY_LOCAL_ENDPOINT`] / `AADT_FOUNDRY_ENDPOINT`.

use std::io::{Read, Write};
use std::net::TcpStream;

use crate::{json_string, string_field, AiError, ModelClient};

/// Default Foundry Local OpenAI-compatible base URL (no trailing slash).
///
/// Override with env `AADT_FOUNDRY_ENDPOINT` (e.g. `http://127.0.0.1:55588`).
pub const FOUNDRY_LOCAL_ENDPOINT: &str = "http://127.0.0.1:5273";

/// Default catalog alias — Phi-3.5-mini, CPU/GPU viable for JSON classification.
pub const DEFAULT_FOUNDRY_MODEL: &str = "phi-3.5-mini";

/// OpenAI-compatible client for a running Foundry Local service.
///
/// POSTs to `/v1/chat/completions` with `response_format: json_object` so wall-type
/// prompts get structured JSON. Connection failures map to [`AiError::Unavailable`].
#[derive(Debug, Clone)]
pub struct FoundryLocalClient {
    pub host: String,
    pub port: u16,
    /// Optional URL path prefix before `/v1/...` (usually empty).
    pub base_path: String,
    pub model: String,
}

impl FoundryLocalClient {
    /// Client for the default endpoint and model ([`DEFAULT_FOUNDRY_MODEL`]).
    pub fn new(model: impl Into<String>) -> Self {
        Self::from_endpoint(resolved_endpoint(), model)
    }

    /// Client for an explicit `http://host:port[/prefix]` base URL.
    pub fn from_endpoint(endpoint: impl AsRef<str>, model: impl Into<String>) -> Self {
        let (host, port, base_path) = parse_http_endpoint(endpoint.as_ref());
        Self {
            host,
            port,
            base_path,
            model: model.into(),
        }
    }
}

impl Default for FoundryLocalClient {
    fn default() -> Self {
        Self::new(DEFAULT_FOUNDRY_MODEL)
    }
}

/// Liveness of a Foundry Local endpoint, for a status indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundryStatus {
    /// Not probed yet this session.
    Unknown,
    /// Nothing is listening — the service is not started.
    Offline,
    /// The endpoint accepted a TCP connection, but no model is confirmed loaded.
    Running,
    /// The service is up **and** a model is loaded / listed — ready for AI calls.
    Active,
}

impl FoundryStatus {
    /// A short label for a status pill.
    pub fn label(self) -> &'static str {
        match self {
            FoundryStatus::Unknown => "checking…",
            FoundryStatus::Offline => "offline",
            FoundryStatus::Running => "running (no model)",
            FoundryStatus::Active => "active (model ready)",
        }
    }

    /// Whether the service is reachable at all (running or active).
    pub fn is_running(self) -> bool {
        matches!(self, FoundryStatus::Running | FoundryStatus::Active)
    }

    /// Whether a model is loaded and the service is ready for AI calls.
    pub fn is_active(self) -> bool {
        matches!(self, FoundryStatus::Active)
    }
}

impl FoundryLocalClient {
    /// The `host:port` endpoint this client targets, for display.
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Probe the endpoint within `timeout`: a TCP connect answers "running", and a
    /// quick `GET /v1/models` answers "active" (a model is loaded). Cheap enough
    /// to poll behind a status indicator; confirms reachability, not correctness.
    pub fn probe(&self, timeout: std::time::Duration) -> FoundryStatus {
        use std::net::ToSocketAddrs;
        let Some(addr) = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .ok()
            .and_then(|mut it| it.next())
        else {
            return FoundryStatus::Offline;
        };
        let Ok(mut stream) = TcpStream::connect_timeout(&addr, timeout) else {
            return FoundryStatus::Offline;
        };
        // Reachable — now see whether a model is loaded via the models list.
        let _ = stream.set_read_timeout(Some(timeout));
        let _ = stream.set_write_timeout(Some(timeout));
        let path = if self.base_path.is_empty() {
            "/v1/models".to_string()
        } else {
            format!("{}/v1/models", self.base_path.trim_end_matches('/'))
        };
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
            self.host, self.port
        );
        if stream.write_all(request.as_bytes()).is_err() {
            return FoundryStatus::Running;
        }
        let mut raw = String::new();
        if stream.read_to_string(&mut raw).is_err() || raw.is_empty() {
            return FoundryStatus::Running;
        }
        // A 200 response whose body lists a model id → a model is loaded.
        let ok = raw.starts_with("HTTP/1.1 200") || raw.starts_with("HTTP/1.0 200");
        let has_model = raw.contains("\"id\"") || raw.contains(&self.model);
        if ok && has_model {
            FoundryStatus::Active
        } else {
            FoundryStatus::Running
        }
    }
}

impl ModelClient for FoundryLocalClient {
    fn generate(&self, prompt: &str) -> Result<String, AiError> {
        let path = if self.base_path.is_empty() {
            "/v1/chat/completions".to_string()
        } else {
            format!(
                "{}/v1/chat/completions",
                self.base_path.trim_end_matches('/')
            )
        };

        // System + user keeps the JSON-only contract explicit for small models.
        let body = format!(
            "{{\
\"model\":{},\
\"temperature\":0,\
\"response_format\":{{\"type\":\"json_object\"}},\
\"messages\":[\
{{\"role\":\"system\",\"content\":\"You are a CAD takeoff assistant. Reply with JSON only.\"}},\
{{\"role\":\"user\",\"content\":{}}}\
]\
}}",
            json_string(&self.model),
            json_string(prompt),
        );

        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\n\
Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.host,
            self.port,
            body.len(),
        );

        let mut stream = TcpStream::connect_timeout(
            &{
                use std::net::ToSocketAddrs;
                (self.host.as_str(), self.port)
                    .to_socket_addrs()
                    .ok()
                    .and_then(|mut it| it.next())
                    .ok_or_else(|| {
                        AiError::Unavailable(format!(
                            "Foundry Local at {}:{} — cannot resolve address",
                            self.host, self.port
                        ))
                    })?
            },
            std::time::Duration::from_secs(2),
        )
        .map_err(|e| {
            AiError::Unavailable(format!(
                "Foundry Local at {}:{} — {e} (is the service running? tip: open Foundry Controls → Discover)",
                self.host, self.port
            ))
        })?;
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(60)));
        let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(10)));
        stream
            .write_all(request.as_bytes())
            .map_err(|e| AiError::Unavailable(e.to_string()))?;
        let mut raw = String::new();
        stream
            .read_to_string(&mut raw)
            .map_err(|e| AiError::Unavailable(e.to_string()))?;

        let http_body = raw
            .split_once("\r\n\r\n")
            .map(|(_, b)| b)
            .ok_or_else(|| AiError::Parse("no HTTP body from Foundry Local".to_string()))?;

        extract_chat_content(http_body)
    }
}

/// Resolve endpoint: `AADT_FOUNDRY_ENDPOINT` env (if still reachable), else live
/// CLI discovery, else [`FOUNDRY_LOCAL_ENDPOINT`].
///
/// Foundry Local often binds a **new** port after restart. A stale env URL must
/// not win forever — that freezes ASK on an unreachable TCP connect.
pub fn resolved_endpoint() -> String {
    if let Ok(env) = std::env::var("AADT_FOUNDRY_ENDPOINT") {
        let env = env.trim().to_string();
        if !env.is_empty() && endpoint_reachable(&env, std::time::Duration::from_millis(300)) {
            return env;
        }
    }
    if let Some(url) = crate::foundry_ctl::discover_endpoint() {
        std::env::set_var("AADT_FOUNDRY_ENDPOINT", &url);
        return url;
    }
    FOUNDRY_LOCAL_ENDPOINT.to_string()
}

/// Resolve model alias: non-empty `preferred`, else `AADT_FOUNDRY_MODEL`, else
/// [`DEFAULT_FOUNDRY_MODEL`].
pub fn resolved_model(preferred: &str) -> String {
    let p = preferred.trim();
    if !p.is_empty() {
        return p.to_string();
    }
    std::env::var("AADT_FOUNDRY_MODEL").unwrap_or_else(|_| DEFAULT_FOUNDRY_MODEL.to_string())
}

/// Quick TCP check — does not issue an HTTP request.
fn endpoint_reachable(endpoint: &str, timeout: std::time::Duration) -> bool {
    use std::net::ToSocketAddrs;
    let (host, port, _) = parse_http_endpoint(endpoint);
    let Ok(mut addrs) = (host.as_str(), port).to_socket_addrs() else {
        return false;
    };
    let Some(addr) = addrs.next() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, timeout).is_ok()
}

/// Parse `http://host:port/optional/path` into (host, port, path).
fn parse_http_endpoint(endpoint: &str) -> (String, u16, String) {
    let trimmed = endpoint.trim().trim_end_matches('/');
    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    let (authority, path) = match without_scheme.find('/') {
        Some(i) => (&without_scheme[..i], without_scheme[i..].to_string()),
        None => (without_scheme, String::new()),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(5273)),
        None => (authority.to_string(), 5273),
    };
    (host, port, path)
}

/// Pull assistant text from an OpenAI-style chat completion body.
fn extract_chat_content(body: &str) -> Result<String, AiError> {
    // Prefer choices[0].message.content
    if let Some(content_region) = body.find("\"content\"") {
        let after = &body[content_region..];
        if let Some(v) = string_field(after, "content") {
            if !v.is_empty() {
                return Ok(v);
            }
        }
    }
    // Some gateways nest content under message; try a second scan after "message".
    if let Some(msg) = body.find("\"message\"") {
        let after = &body[msg..];
        if let Some(v) = string_field(after, "content") {
            return Ok(v);
        }
    }
    Err(AiError::Parse(
        "Foundry Local response missing choices[].message.content".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_endpoint_with_path() {
        let (h, p, path) = parse_http_endpoint("http://127.0.0.1:55588/openai");
        assert_eq!(h, "127.0.0.1");
        assert_eq!(p, 55588);
        assert_eq!(path, "/openai");
    }

    #[test]
    fn probe_reports_offline_for_a_dead_port() {
        // Port 1 is reserved and never listening → Offline, quickly.
        let c = FoundryLocalClient::from_endpoint("http://127.0.0.1:1", "phi-3.5-mini");
        assert_eq!(
            c.probe(std::time::Duration::from_millis(200)),
            FoundryStatus::Offline
        );
        assert_eq!(c.endpoint(), "127.0.0.1:1");
    }

    #[test]
    fn extracts_openai_content() {
        let body =
            r#"{"choices":[{"message":{"role":"assistant","content":"{\"assignments\":[]}"}}]}"#;
        let c = extract_chat_content(body).unwrap();
        assert!(c.contains("assignments"));
    }
}
