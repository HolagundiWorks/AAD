//! The HTTP seam: a blocking transport trait so the client is tested without sockets and the web build compiles without a network stack.
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    None,
    Json(Vec<u8>),
    Bytes { content_type: String, data: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Largest answer accepted (a 25 MB drawing plus headroom).
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_RESPONSE_BYTES: usize = 40 * 1024 * 1024;

pub trait Transport: Send {
    fn send(&self, req: &Request) -> Result<Response>;
}

/// Native transport (rustls). Timeouts bound every call; non-2xx answers are returned, not raised, so the client can read the error body.
#[cfg(not(target_arch = "wasm32"))]
pub struct UreqTransport {
    agent: ureq::Agent,
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for UreqTransport {
    fn default() -> Self {
        UreqTransport {
            agent: ureq::AgentBuilder::new()
                .timeout_connect(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(60))
                .user_agent("AAD (AORMS connect)")
                .build(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Transport for UreqTransport {
    fn send(&self, req: &Request) -> Result<Response> {
        use crate::error::AormsError;
        use std::io::Read;
        let mut r = match req.method {
            Method::Get => self.agent.get(&req.url),
            Method::Post => self.agent.post(&req.url),
            Method::Put => self.agent.put(&req.url),
        };
        for (k, v) in &req.headers {
            r = r.set(k, v);
        }
        let result = match &req.body {
            Body::None => r.call(),
            Body::Json(b) => r.set("Content-Type", "application/json").send_bytes(b),
            Body::Bytes { content_type, data } => r.set("Content-Type", content_type).send_bytes(data),
        };
        let res = match result {
            Ok(res) => res,
            Err(ureq::Error::Status(_, res)) => res,
            Err(ureq::Error::Transport(t)) => return Err(AormsError::Network(t.to_string())),
        };
        let status = res.status();
        let mut body = Vec::new();
        res.into_reader().take(MAX_RESPONSE_BYTES as u64 + 1).read_to_end(&mut body).map_err(|e| AormsError::Network(e.to_string()))?;
        if body.len() > MAX_RESPONSE_BYTES {
            return Err(AormsError::TooLarge);
        }
        Ok(Response { status, body })
    }
}
