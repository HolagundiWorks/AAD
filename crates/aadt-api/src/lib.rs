//! # aadt-api — the local API over a live [`Session`]
//!
//! AI-native Phase 5 ([ADR-0015](../../../docs/adr/0015-local-api-layer.md)):
//! *"everything the UI can do, scriptable"*. This crate serves a small
//! HTTP/JSON API on loopback, backed by the same [`Session`] the command line,
//! the GUI, and the CLI shell drive.
//!
//! Four properties define it, and each of them is a decision rather than a
//! default:
//!
//! - **Loopback only.** The listener binds `127.0.0.1`, and a connection whose
//!   peer is not loopback is dropped without a reply. This is a local
//!   automation seam, not a network service — nothing here is hardened for
//!   the open internet, so it is not reachable from there.
//! - **Token-gated, every route.** A bearer token is generated at startup and
//!   printed once. Without it, every route answers 401 — including health, so
//!   there is no endpoint whose gate someone can quietly widen later.
//! - **Off by default, and never load-bearing.** Nothing starts this server
//!   but an explicit `aadt serve`. Offline drafting has no dependency on it,
//!   which is the Phase 5 constraint in the phase plan.
//! - **One write path.** Reads project the session; the only mutation is
//!   `POST /v1/command`, which feeds a command line to the interpreter. An API
//!   caller therefore gets the same validation, undo record, history entry,
//!   and audit trail as a user typing at the prompt. There is deliberately no
//!   `POST /v1/entities` — a second door into the document is a second set of
//!   invariants to keep in step, and the deterministic core is the invariant.
//!
//! ## Why the server is single-threaded
//!
//! A [`Session`] is not `Send`: the command registry holds `Rc<RefCell<…>>`
//! state that the CLI shares between `SAVE` and the Object-DB commands. That
//! is not an accident to work around — a session *is* one editor on one
//! drawing, and two threads mutating it concurrently would need a document
//! lock that produces exactly the serialisation this loop already has.
//!
//! So connections are accepted and answered one at a time. The cost is that a
//! slow client delays the next one, which is why every socket carries a read
//! and write timeout and why [`http`] bounds request size before allocating.
//!
//! ## Std only
//!
//! No `tokio`, no `hyper`, no `serde` — consistent with the rest of the
//! workspace (`unsafe_code = forbid`, hand-rolled `.vdb` and PDF codecs). The
//! HTTP subset in [`http`] and the JSON in [`json`] are small because the API
//! surface is small.

pub mod http;
pub mod json;
pub mod routes;

use std::io::{BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use aadt_app::Session;
use aadt_object::{Capability, CapabilitySet, Role};

pub use routes::{handle, Handled, API_VERSION};

/// The port `aadt serve` uses unless told otherwise.
///
/// One above ShilpiDB's `shilpid` (7420) so a developer running both does not
/// have to remember which one claimed the port.
pub const DEFAULT_PORT: u16 = 7421;

/// How long a client may take to send its request, or receive its response,
/// before the connection is abandoned. The loop is single-threaded, so an
/// unbounded wait here is an unbounded wait for every other client.
pub const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// How the server is configured for a run.
#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// TCP port on `127.0.0.1`; `0` asks the OS for a free one.
    pub port: u16,
    /// Bearer token every request must present.
    pub token: String,
    /// Refuse `POST /v1/command`, serving reads only.
    pub read_only: bool,
    /// Identity attached to the token (M14.2). Shown in health / PERMS WHOAMI.
    pub principal: String,
    /// Capabilities granted to holders of this token (M14.2).
    pub caps: CapabilitySet,
}

impl ApiConfig {
    /// A configuration with a freshly generated token and admin capabilities.
    pub fn new(port: u16, read_only: bool) -> Self {
        let mut caps = Role::Admin.capabilities();
        if read_only {
            caps = Role::Viewer.capabilities();
        }
        Self {
            port,
            token: generate_token(),
            read_only,
            principal: aadt_object::current_user(),
            caps,
        }
    }

    /// Use a caller-supplied token (a script that already knows the secret).
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = token.into();
        self
    }

    /// Bind the token to a named principal.
    pub fn with_principal(mut self, id: impl Into<String>) -> Self {
        self.principal = id.into();
        self
    }

    /// Restrict (or expand) the token's capability set via a named role.
    pub fn with_role(mut self, role: Role) -> Self {
        self.caps = role.capabilities();
        if self.read_only {
            // read_only always wins over a write-capable role.
            self.caps = Role::Viewer.capabilities();
        }
        self
    }

    /// Direct capability override (for tests).
    pub fn with_caps(mut self, caps: CapabilitySet) -> Self {
        self.caps = caps;
        self
    }

    pub fn can(&self, c: Capability) -> bool {
        self.caps.has(c)
    }
}

/// The outcome of serving one connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServeOutcome {
    /// A request was answered; keep going.
    Served,
    /// A client called `POST /v1/shutdown`; stop the loop.
    Shutdown,
}

/// A bound listener plus its configuration.
pub struct ApiServer {
    listener: TcpListener,
    config: ApiConfig,
}

impl ApiServer {
    /// Bind the configured port on loopback.
    ///
    /// The bound port is written back into the configuration, so a caller that
    /// asked for port `0` can print the one the OS chose.
    pub fn bind(mut config: ApiConfig) -> std::io::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, config.port)))?;
        config.port = listener.local_addr()?.port();
        Ok(Self { listener, config })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn config(&self) -> &ApiConfig {
        &self.config
    }

    /// The line a driver prints so a user can copy a working request.
    pub fn banner(&self) -> String {
        let port = self.config.port;
        let mode = if self.config.read_only {
            "read-only"
        } else {
            "read/write"
        };
        format!(
            "API listening on http://127.0.0.1:{port} ({mode})\n\
             token: {}\n\
             try:   curl -H \"Authorization: Bearer {}\" http://127.0.0.1:{port}/v1/health",
            self.config.token, self.config.token
        )
    }

    /// Serve connections until a client calls `POST /v1/shutdown`.
    pub fn serve(&self, session: &mut Session) -> std::io::Result<()> {
        loop {
            if self.serve_once(session)? == ServeOutcome::Shutdown {
                return Ok(());
            }
        }
    }

    /// Accept and answer exactly one connection.
    ///
    /// A connection that cannot be read is closed and counted as served: one
    /// bad client must not end a session that the user still has open.
    pub fn serve_once(&self, session: &mut Session) -> std::io::Result<ServeOutcome> {
        let (stream, peer) = self.listener.accept()?;
        // Defence in depth: the listener is already loopback-bound, but a
        // remote peer arriving here would mean that assumption broke, and the
        // right response to a broken assumption is to say nothing at all.
        if !peer.ip().is_loopback() {
            return Ok(ServeOutcome::Served);
        }
        Ok(self.answer(stream, session))
    }

    fn answer(&self, stream: TcpStream, session: &mut Session) -> ServeOutcome {
        let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
        let mut write_half = match stream.try_clone() {
            Ok(s) => s,
            Err(_) => return ServeOutcome::Served,
        };
        let mut reader = BufReader::new(stream);

        let (response, shutdown) = match http::read_request(&mut reader) {
            Ok(req) => {
                let handled = routes::handle(&req, session, &self.config);
                (handled.response, handled.shutdown)
            }
            Err(http::ReadError::Closed) => return ServeOutcome::Served,
            Err(http::ReadError::Io(_)) => return ServeOutcome::Served,
            Err(http::ReadError::Malformed(m)) => (http::Response::error(400, &m), false),
            Err(http::ReadError::Unsupported { status, message }) => {
                (http::Response::error(status, &message), false)
            }
        };

        let _ = response.write_to(&mut write_half);
        let _ = write_half.flush();
        if shutdown {
            ServeOutcome::Shutdown
        } else {
            ServeOutcome::Served
        }
    }
}

/// A 32-hex-character token for one run of the server.
///
/// **What this is and is not.** It is mixed from the wall clock, the process
/// id, and an ASLR-dependent heap address through SplitMix64 — enough that a
/// token is not guessable from another process's clock reading, which is the
/// realistic threat for a loopback port that any local program can connect to.
/// It is **not** a cryptographic secret: the workspace takes no RNG dependency,
/// and anything that can read this process's memory or the terminal it printed
/// to has already lost the game. The security boundary is the loopback bind
/// plus the user's decision to start the server at all; the token stops
/// unrelated local software from stumbling into the drawing.
pub fn generate_token() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let pid = u64::from(std::process::id());
    let heap = Box::new(0u8);
    let addr = (&*heap as *const u8) as usize as u64;
    let mut state = nanos ^ pid.rotate_left(17) ^ addr.rotate_left(33);

    let mut out = String::with_capacity(32);
    for _ in 0..4 {
        state = split_mix64(&mut state);
        out.push_str(&format!("{state:016x}"));
    }
    out.truncate(32);
    out
}

/// SplitMix64 — the standard finaliser, used here purely to diffuse the
/// entropy sources above rather than to generate a stream.
fn split_mix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};

    /// Drive one real request over a real socket, so the transport is proven
    /// end to end and not only the pure routing layer.
    fn round_trip(server: &ApiServer, session: &mut Session, request: &str) -> String {
        let addr = server.local_addr().unwrap();
        let request = request.to_string();
        let client = std::thread::spawn(move || {
            let mut sock = TcpStream::connect(addr).unwrap();
            sock.write_all(request.as_bytes()).unwrap();
            sock.flush().unwrap();
            let mut response = String::new();
            sock.read_to_string(&mut response).unwrap();
            response
        });
        server.serve_once(session).unwrap();
        client.join().unwrap()
    }

    fn server(read_only: bool) -> ApiServer {
        ApiServer::bind(ApiConfig::new(0, read_only).with_token("t0ken")).unwrap()
    }

    #[test]
    fn binds_loopback_on_an_ephemeral_port() {
        let s = server(false);
        let addr = s.local_addr().unwrap();
        assert!(addr.ip().is_loopback(), "bound {addr}, expected loopback");
        assert_ne!(addr.port(), 0, "the OS-chosen port is reported back");
        assert_eq!(s.config().port, addr.port());
    }

    #[test]
    fn serves_a_real_request_over_a_real_socket() {
        let s = server(false);
        let mut session = Session::with_builtins();
        let response = round_trip(
            &s,
            &mut session,
            "GET /v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer t0ken\r\n\r\n",
        );
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
        assert!(response.contains(r#""status":"ok""#));
    }

    #[test]
    fn a_command_over_the_wire_edits_the_session() {
        let s = server(false);
        let mut session = Session::with_builtins();
        let body = r#"{"line":"LINE 0 0 10 0 A-WALL"}"#;
        let response = round_trip(
            &s,
            &mut session,
            &format!(
                "POST /v1/command HTTP/1.1\r\nAuthorization: Bearer t0ken\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        );
        assert!(response.contains(r#""changed":true"#), "{response}");
        assert_eq!(session.document().len(), 1);
    }

    #[test]
    fn an_unauthenticated_request_is_refused_over_the_wire() {
        let s = server(false);
        let mut session = Session::with_builtins();
        let response = round_trip(&s, &mut session, "GET /v1/health HTTP/1.1\r\n\r\n");
        assert!(
            response.starts_with("HTTP/1.1 401 Unauthorized"),
            "{response}"
        );
        assert!(response.contains("WWW-Authenticate"));
    }

    #[test]
    fn garbage_gets_a_400_instead_of_ending_the_session() {
        let s = server(false);
        let mut session = Session::with_builtins();
        let response = round_trip(&s, &mut session, "not http at all\r\n\r\n");
        assert!(response.starts_with("HTTP/1.1 400"), "{response}");
        // The session survives, and the next request is answered normally.
        let ok = round_trip(
            &s,
            &mut session,
            "GET /v1/health HTTP/1.1\r\nAuthorization: Bearer t0ken\r\n\r\n",
        );
        assert!(ok.starts_with("HTTP/1.1 200 OK"), "{ok}");
    }

    #[test]
    fn shutdown_stops_the_loop() {
        let s = server(false);
        let mut session = Session::with_builtins();
        let addr = s.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut sock = TcpStream::connect(addr).unwrap();
            sock.write_all(
                b"POST /v1/shutdown HTTP/1.1\r\nAuthorization: Bearer t0ken\r\nContent-Length: 0\r\n\r\n",
            )
            .unwrap();
            let mut reader = BufReader::new(sock);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            line
        });
        // `serve` returns rather than looping forever — the test would hang if
        // shutdown did not take effect.
        s.serve(&mut session).unwrap();
        assert!(client.join().unwrap().starts_with("HTTP/1.1 200 OK"));
    }

    #[test]
    fn tokens_differ_between_runs_and_are_hex() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "a fresh token per run, not a constant");
    }

    #[test]
    fn banner_carries_the_port_and_a_runnable_example() {
        let s = server(true);
        let banner = s.banner();
        assert!(banner.contains(&s.config().port.to_string()));
        assert!(banner.contains("read-only"));
        assert!(banner.contains("t0ken"));
    }
}
