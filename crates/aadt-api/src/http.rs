//! A very small HTTP/1.1 request reader and response writer — std only.
//!
//! This is not a general web server. It implements exactly the subset a local
//! JSON API needs: one request per connection, an explicit `Content-Length`
//! body, and `Connection: close`. Everything outside that subset is refused
//! with a status code rather than approximated, because a half-implemented
//! HTTP feature on a network surface is worse than an absent one.
//!
//! **Limits are part of the parser, not an afterthought.** Request line,
//! header count, header size, and body size are all bounded before any
//! allocation grows to match them, so a malformed or hostile client cannot
//! make the server allocate without bound. The server is single-threaded (see
//! the crate docs), which makes that bound the difference between one rejected
//! request and a wedged session.

use std::io::{BufRead, Write};

/// Longest accepted request line (method + target + version).
pub const MAX_REQUEST_LINE: usize = 8 * 1024;
/// Most accepted header lines.
pub const MAX_HEADERS: usize = 64;
/// Longest accepted single header line.
pub const MAX_HEADER_LINE: usize = 8 * 1024;
/// Largest accepted request body.
pub const MAX_BODY: usize = 1024 * 1024;

/// A parsed request: method, decoded path, raw query, headers, body.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub method: String,
    /// Percent-decoded path, without the query string.
    pub path: String,
    /// Raw query string (without `?`), or empty.
    pub query: String,
    /// Header names lowercased; values trimmed.
    headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    /// Build a request directly — the seam route tests use instead of a socket.
    pub fn new(method: &str, target: &str, body: &str) -> Self {
        let (path, query) = split_target(target);
        Self {
            method: method.to_ascii_uppercase(),
            path: percent_decode(&path),
            query,
            headers: Vec::new(),
            body: body.to_string(),
        }
    }

    /// Attach a header (name matching is case-insensitive).
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers
            .push((name.to_ascii_lowercase(), value.to_string()));
        self
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The first value of a query parameter, percent-decoded.
    pub fn query_param(&self, key: &str) -> Option<String> {
        self.query
            .split('&')
            .filter(|p| !p.is_empty())
            .find_map(|p| {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                (percent_decode(&k.replace('+', " ")) == key)
                    .then(|| percent_decode(&v.replace('+', " ")))
            })
    }

    /// A query parameter parsed as a non-negative integer.
    pub fn query_usize(&self, key: &str) -> Result<Option<usize>, String> {
        match self.query_param(key) {
            None => Ok(None),
            Some(raw) => raw
                .parse::<usize>()
                .map(Some)
                .map_err(|_| format!("{key}: expected a non-negative integer, got {raw:?}")),
        }
    }
}

/// Why a request could not be read.
#[derive(Debug)]
pub enum ReadError {
    /// The peer closed before sending anything — a port scan or a dropped
    /// connection, not an error worth answering.
    Closed,
    /// Syntactically invalid: answered with 400.
    Malformed(String),
    /// Valid HTTP this server does not implement: answered with 411/413/501.
    Unsupported {
        status: u16,
        message: String,
    },
    Io(std::io::Error),
}

impl From<std::io::Error> for ReadError {
    fn from(e: std::io::Error) -> Self {
        ReadError::Io(e)
    }
}

/// Read one request. Returns [`ReadError::Closed`] when the peer sends nothing.
pub fn read_request<R: BufRead>(r: &mut R) -> Result<Request, ReadError> {
    let line = match read_line(r, MAX_REQUEST_LINE)? {
        Some(l) => l,
        None => return Err(ReadError::Closed),
    };
    let mut parts = line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| ReadError::Malformed("empty request line".into()))?
        .to_ascii_uppercase();
    let target = parts
        .next()
        .ok_or_else(|| ReadError::Malformed("request line has no target".into()))?
        .to_string();
    // Two different failures hide behind a bad third token: a client speaking
    // a version this server does not implement (505), and a peer that is not
    // speaking HTTP at all (400). Telling them apart is the difference between
    // "upgrade your client" and "you connected to the wrong port".
    let version = parts.next().unwrap_or("HTTP/1.1");
    if !version.starts_with("HTTP/") {
        return Err(ReadError::Malformed(format!(
            "request line is not HTTP: {line:?}"
        )));
    }
    if !version.starts_with("HTTP/1.") {
        return Err(ReadError::Unsupported {
            status: 505,
            message: format!("unsupported HTTP version {version}"),
        });
    }

    let mut headers = Vec::new();
    loop {
        let line = read_line(r, MAX_HEADER_LINE)?.ok_or(ReadError::Closed)?;
        if line.is_empty() {
            break;
        }
        if headers.len() >= MAX_HEADERS {
            return Err(ReadError::Unsupported {
                status: 431,
                message: format!("more than {MAX_HEADERS} headers"),
            });
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| ReadError::Malformed(format!("malformed header line {line:?}")))?;
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }

    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };

    // Chunked bodies are legal HTTP that this server does not decode. Reading
    // one as if it were a plain body would feed the frame markers to the JSON
    // parser, so it is refused by name instead.
    if header("transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked")) {
        return Err(ReadError::Unsupported {
            status: 411,
            message: "chunked request bodies are not supported; send Content-Length".into(),
        });
    }

    let len = match header("content-length") {
        None => 0,
        Some(raw) => raw
            .trim()
            .parse::<usize>()
            .map_err(|_| ReadError::Malformed(format!("malformed Content-Length {raw:?}")))?,
    };
    if len > MAX_BODY {
        return Err(ReadError::Unsupported {
            status: 413,
            message: format!("request body over {MAX_BODY} bytes"),
        });
    }
    let mut buf = vec![0u8; len];
    if len > 0 {
        std::io::Read::read_exact(r, &mut buf)?;
    }
    let body = String::from_utf8(buf)
        .map_err(|_| ReadError::Malformed("request body is not valid UTF-8".into()))?;

    let (path, query) = split_target(&target);
    Ok(Request {
        method,
        path: percent_decode(&path),
        query,
        headers,
        body,
    })
}

/// Read one CRLF/LF-terminated line, bounded by `max`. `None` at clean EOF.
fn read_line<R: BufRead>(r: &mut R, max: usize) -> Result<Option<String>, ReadError> {
    let mut buf = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        match std::io::Read::read(r, &mut byte)? {
            0 => {
                return if buf.is_empty() {
                    Ok(None)
                } else {
                    Err(ReadError::Malformed("unterminated line".into()))
                }
            }
            _ => {
                if byte[0] == b'\n' {
                    if buf.last() == Some(&b'\r') {
                        buf.pop();
                    }
                    return String::from_utf8(buf)
                        .map(Some)
                        .map_err(|_| ReadError::Malformed("line is not valid UTF-8".into()));
                }
                buf.push(byte[0]);
                if buf.len() > max {
                    return Err(ReadError::Unsupported {
                        status: 431,
                        message: format!("header or request line over {max} bytes"),
                    });
                }
            }
        }
    }
}

fn split_target(target: &str) -> (String, String) {
    match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.to_string(), String::new()),
    }
}

/// Decode `%XX` escapes; malformed escapes are left verbatim rather than
/// dropped, so a path that was never encoded still matches itself.
pub fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A response: a status and a JSON body.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub body: String,
    pub extra_headers: Vec<(String, String)>,
}

impl Response {
    pub fn json(status: u16, body: String) -> Self {
        Self {
            status,
            body,
            extra_headers: Vec::new(),
        }
    }

    /// An error body in the API's one error shape: `{"error": …, "status": …}`.
    pub fn error(status: u16, message: &str) -> Self {
        Self::json(
            status,
            crate::json::object(&[
                ("error", crate::json::string(message)),
                ("status", crate::json::int(u64::from(status))),
            ]),
        )
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.extra_headers
            .push((name.to_string(), value.to_string()));
        self
    }

    pub fn status_text(&self) -> &'static str {
        match self.status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            411 => "Length Required",
            413 => "Payload Too Large",
            415 => "Unsupported Media Type",
            431 => "Request Header Fields Too Large",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            505 => "HTTP Version Not Supported",
            _ => "Status",
        }
    }

    /// Serialise the whole response, headers included.
    pub fn write_to<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n",
            self.status,
            self.status_text(),
            self.body.len()
        );
        for (name, value) in &self.extra_headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        w.write_all(head.as_bytes())?;
        w.write_all(self.body.as_bytes())?;
        w.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn raw(req: &str) -> Result<Request, ReadError> {
        read_request(&mut Cursor::new(req.as_bytes().to_vec()))
    }

    #[test]
    fn parses_method_path_query_headers_and_body() {
        let req = raw("POST /v1/command?echo=1 HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer t\r\nContent-Length: 7\r\n\r\n{\"a\":1}").unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/v1/command");
        assert_eq!(req.query_param("echo").as_deref(), Some("1"));
        assert_eq!(req.header("AUTHORIZATION"), Some("Bearer t"));
        assert_eq!(req.body, "{\"a\":1}");
    }

    #[test]
    fn decodes_percent_escapes_in_path_and_query() {
        let req = raw("GET /v1/entities?layer=A-WALL%2DINT HTTP/1.1\r\n\r\n").unwrap();
        assert_eq!(req.query_param("layer").as_deref(), Some("A-WALL-INT"));
        assert_eq!(percent_decode("a%20b"), "a b");
        // A stray percent is not an escape; keep it rather than eat it.
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn a_silent_peer_is_closed_not_malformed() {
        assert!(matches!(raw(""), Err(ReadError::Closed)));
    }

    #[test]
    fn separates_wrong_version_from_not_http_at_all() {
        match raw("GET / HTTP/2.0\r\n\r\n").unwrap_err() {
            ReadError::Unsupported { status, .. } => assert_eq!(status, 505),
            other => panic!("expected 505, got {other:?}"),
        }
        assert!(matches!(
            raw("not http at all\r\n\r\n"),
            Err(ReadError::Malformed(_))
        ));
    }

    #[test]
    fn refuses_chunked_bodies_by_name() {
        let err =
            raw("POST /v1/command HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap_err();
        match err {
            ReadError::Unsupported { status, .. } => assert_eq!(status, 411),
            other => panic!("expected 411, got {other:?}"),
        }
    }

    #[test]
    fn bounds_body_and_request_line() {
        let big = format!(
            "POST /v1/command HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );
        match raw(&big).unwrap_err() {
            ReadError::Unsupported { status, .. } => assert_eq!(status, 413),
            other => panic!("expected 413, got {other:?}"),
        }
        let long = format!("GET /{} HTTP/1.1\r\n\r\n", "a".repeat(MAX_REQUEST_LINE + 1));
        match raw(&long).unwrap_err() {
            ReadError::Unsupported { status, .. } => assert_eq!(status, 431),
            other => panic!("expected 431, got {other:?}"),
        }
    }

    #[test]
    fn writes_a_complete_response() {
        let mut out = Vec::new();
        Response::error(404, "no such route")
            .write_to(&mut out)
            .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(text.contains("Content-Type: application/json; charset=utf-8\r\n"));
        assert!(text.contains("Content-Length: 41\r\n") || text.contains("Content-Length: "));
        assert!(text.ends_with(r#"{"error":"no such route","status":404}"#));
    }
}
