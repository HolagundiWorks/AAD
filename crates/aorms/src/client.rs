//! The AORMS client: sign-in, one session for this app, token refresh that is safe with other apps sharing the sign-in, and the
//! project calls. Blocking; every call is bounded by the transport's timeouts. A failure never loses local work: callers keep it.
use serde_json::{Value, json};

use crate::error::{AormsError, Result};
use crate::model::{OnlineProject, SessionInfo};
use crate::store::{SessionStore, SharedSignIn, SharedSignInStore};
use crate::transport::{Body, Method, Request, Response, Transport};
use crate::{APP_ID, SUPPORTED_CONTRACT};

/// Refresh this long before the access token expires.
const REFRESH_SKEW_SECS: i64 = 60;

/// What `restore` found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// No stored sign-in: Community mode.
    NoStoredSession,
    /// Signed in and this app's session is current.
    Restored,
    /// Signed in, but this app has no session yet — the user starts one (never taken silently).
    NeedsSession,
    /// Another sign-in of AAD took the session over.
    SessionReplaced,
    /// The studio isn't entitled to connected mode (see the error from `start_session` for the reason).
    NotConnected,
    /// The stored sign-in was refused; the user must sign in again.
    SignedOut,
}

struct Config {
    supabase_url: String,
    anon_key: String,
}

pub struct Client {
    base: String,
    transport: Box<dyn Transport>,
    shared: Box<dyn SharedSignInStore>,
    local: Box<dyn SessionStore>,
    now: Box<dyn Fn() -> i64 + Send>,
    config: Option<Config>,
    access: Option<String>,
    expires_at: i64,
    refresh: Option<String>,
    email: Option<String>,
    session: Option<SessionInfo>,
}

fn refused(status: u16) -> bool {
    matches!(status, 400 | 401 | 403)
}

impl Client {
    pub fn new(base_url: &str, transport: Box<dyn Transport>, shared: Box<dyn SharedSignInStore>, local: Box<dyn SessionStore>) -> Client {
        Client::with_clock(base_url, transport, shared, local, Box::new(unix_now))
    }

    pub fn with_clock(
        base_url: &str,
        transport: Box<dyn Transport>,
        shared: Box<dyn SharedSignInStore>,
        local: Box<dyn SessionStore>,
        now: Box<dyn Fn() -> i64 + Send>,
    ) -> Client {
        Client {
            base: base_url.trim_end_matches('/').to_string(),
            transport,
            shared,
            local,
            now,
            config: None,
            access: None,
            expires_at: 0,
            refresh: None,
            email: None,
            session: None,
        }
    }

    pub fn session(&self) -> Option<&SessionInfo> {
        self.session.as_ref()
    }
    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }
    pub fn is_signed_in(&self) -> bool {
        self.refresh.is_some()
    }
    /// Signed in with a current session and a connected studio.
    pub fn is_connected(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.connected)
    }
    pub fn base_url(&self) -> &str {
        &self.base
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base, path.trim_start_matches('/'))
    }

    fn json_response(res: &Response) -> Result<Value> {
        if res.status >= 400 {
            return Err(AormsError::from_body(res.status, &res.body));
        }
        if res.body.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&res.body).map_err(|e| AormsError::BadResponse(e.to_string()))
    }

    // ── config, sign-in, session ─────────────────────────────────────────────────────────────────────────────

    fn load_config(&mut self) -> Result<&Config> {
        if self.config.is_none() {
            let req = Request { method: Method::Get, url: self.url("api/aqc/v1/config"), headers: vec![], body: Body::None };
            let v = Self::json_response(&self.transport.send(&req)?)?;
            let contract = v.get("contract").and_then(Value::as_str).unwrap_or("");
            if contract != SUPPORTED_CONTRACT {
                return Err(AormsError::Contract(contract.to_string()));
            }
            let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            self.config = Some(Config { supabase_url: text("supabaseUrl"), anon_key: text("supabaseAnonKey") });
        }
        self.config.as_ref().ok_or(AormsError::BadResponse("no config".into()))
    }

    /// Password sign-in, then this app's session (replacing this app's session on another computer, if any).
    pub fn sign_in(&mut self, email: &str, password: &str, client_label: &str) -> Result<SessionInfo> {
        self.load_config()?;
        let req = Request {
            method: Method::Post,
            url: self.url("api/aqc/v1/auth/login"),
            headers: vec![],
            body: Body::Json(json!({ "email": email, "password": password }).to_string().into_bytes()),
        };
        let v = Self::json_response(&self.transport.send(&req)?)?;
        let tok = |k: &str| v.get(k).and_then(Value::as_str).filter(|t| !t.is_empty()).map(str::to_string);
        let (Some(access), Some(refresh)) = (tok("accessToken"), tok("refreshToken")) else {
            return Err(AormsError::BadResponse("sign-in answer has no tokens".into()));
        };
        self.access = Some(access);
        self.refresh = Some(refresh);
        self.expires_at = v.get("expiresAt").and_then(Value::as_i64).unwrap_or_else(|| (self.now)() + 3600);
        self.email = tok("email").or_else(|| Some(email.to_string()));
        self.start_session(client_label)
    }

    /// `POST /session` — this app becomes the account's active AAD session.
    pub fn start_session(&mut self, client_label: &str) -> Result<SessionInfo> {
        self.ensure_fresh()?;
        let access = self.access.clone().ok_or(AormsError::SignedOut)?;
        let req = Request {
            method: Method::Post,
            url: self.url("api/aqc/v1/session"),
            headers: vec![("Authorization".into(), format!("Bearer {access}"))],
            body: Body::Json(json!({ "clientLabel": client_label.chars().take(120).collect::<String>(), "app": APP_ID }).to_string().into_bytes()),
        };
        let res = self.transport.send(&req)?;
        if res.status >= 400 {
            let e = AormsError::from_body(res.status, &res.body);
            if e.is_not_connected() {
                self.session = None;
            }
            return Err(e);
        }
        let v = Self::json_response(&res)?;
        let info = SessionInfo::from_json(&v).ok_or_else(|| AormsError::BadResponse("session answer incomplete".into()))?;
        self.session = Some(info.clone());
        self.persist();
        Ok(info)
    }

    /// Silent start-up from the shared sign-in. Never takes a session over.
    pub fn restore(&mut self) -> Result<RestoreOutcome> {
        let Some(shared) = self.shared.load() else { return Ok(RestoreOutcome::NoStoredSession) };
        self.refresh = Some(shared.refresh_token);
        self.email = shared.email;
        match self.do_refresh() {
            Ok(()) => {}
            Err(AormsError::SignedOut) => return Ok(RestoreOutcome::SignedOut),
            Err(e) => return Err(e), // offline / busy: still signed in, try again later
        }
        let Some(session_id) = self.local.load() else { return Ok(RestoreOutcome::NeedsSession) };
        match self.api(Method::Get, "api/aqc/v1/session", None, Some(&session_id)) {
            Ok(v) => match SessionInfo::from_json(&v) {
                Some(info) => {
                    self.session = Some(info);
                    self.persist();
                    Ok(RestoreOutcome::Restored)
                }
                None => Err(AormsError::BadResponse("session answer incomplete".into())),
            },
            Err(e) if e.is_session_replaced() => Ok(RestoreOutcome::SessionReplaced),
            Err(e) if e.is_not_connected() => Ok(RestoreOutcome::NotConnected),
            Err(AormsError::SignedOut) => Ok(RestoreOutcome::SignedOut),
            Err(e) => Err(e),
        }
    }

    /// Cheap health check; also refreshes the server's last-seen. A replaced session surfaces as `session_replaced`.
    pub fn check_session(&mut self) -> Result<SessionInfo> {
        let v = self.call_session(Method::Get, "api/aqc/v1/session", None)?;
        let info = SessionInfo::from_json(&v).ok_or_else(|| AormsError::BadResponse("session answer incomplete".into()))?;
        self.session = Some(info.clone());
        Ok(info)
    }

    /// Sign out of the shared login (the other suite apps find it empty on their next refresh and ask to sign in).
    pub fn sign_out(&mut self) {
        self.clear_local();
        self.shared.clear();
        self.local.clear();
    }

    fn clear_local(&mut self) {
        self.access = None;
        self.refresh = None;
        self.session = None;
        self.expires_at = 0;
    }

    fn persist(&self) {
        if let Some(r) = &self.refresh {
            self.shared.save(&SharedSignIn { refresh_token: r.clone(), email: self.email.clone(), updated_at: (self.now)() });
        }
        if let Some(s) = &self.session {
            self.local.save(&s.session_id);
        }
    }

    // ── token refresh (rotation-safe: the sign-in is shared with other apps) ────────────────────────────────

    fn ensure_fresh(&mut self) -> Result<()> {
        if self.access.is_some() && (self.now)() + REFRESH_SKEW_SECS < self.expires_at {
            return Ok(());
        }
        if self.refresh.is_none() {
            return Ok(());
        }
        self.do_refresh()
    }

    fn refresh_call(&self, token: &str) -> Result<Response> {
        let Some(cfg) = &self.config else { return Err(AormsError::BadResponse("no config".into())) };
        let req = Request {
            method: Method::Post,
            url: format!("{}/auth/v1/token?grant_type=refresh_token", cfg.supabase_url.trim_end_matches('/')),
            headers: vec![("apikey".into(), cfg.anon_key.clone())],
            body: Body::Json(json!({ "refresh_token": token }).to_string().into_bytes()),
        };
        self.transport.send(&req)
    }

    fn do_refresh(&mut self) -> Result<()> {
        self.load_config()?;
        let mut token = self.refresh.clone().ok_or(AormsError::SignedOut)?;
        // Another suite app may have rotated the shared token since we last looked: use the newest one.
        if let Some(latest) = self.shared.load() {
            token = latest.refresh_token;
        }
        let mut res = self.refresh_call(&token)?;
        if refused(res.status) {
            // Refused, but the shared token changed while we were refreshing (the other app won the race): retry once with it.
            if let Some(again) = self.shared.load().filter(|a| a.refresh_token != token) {
                token = again.refresh_token;
                res = self.refresh_call(&token)?;
            }
        }
        if refused(res.status) {
            if self.shared.load().is_some_and(|s| s.refresh_token == token) {
                self.shared.clear(); // the shared token is dead too
            }
            self.clear_local();
            return Err(AormsError::SignedOut);
        }
        if res.status >= 400 {
            // A busy or failing auth server (5xx, 429) never signs anyone out.
            return Err(AormsError::Api {
                status: res.status,
                code: "unavailable".into(),
                message: "AORMS sign-in isn't available right now.".into(),
                reason: None,
                retry_after: None,
            });
        }
        let v: Value = serde_json::from_slice(&res.body).map_err(|e| AormsError::BadResponse(e.to_string()))?;
        let text = |k: &str| v.get(k).and_then(Value::as_str).filter(|t| !t.is_empty()).map(str::to_string);
        let (Some(access), Some(refresh)) = (text("access_token"), text("refresh_token")) else {
            return Err(AormsError::BadResponse("refresh answer has no tokens".into()));
        };
        self.expires_at =
            v.get("expires_at").and_then(Value::as_i64).unwrap_or_else(|| (self.now)() + v.get("expires_in").and_then(Value::as_i64).unwrap_or(3600));
        self.access = Some(access);
        self.refresh = Some(refresh);
        self.persist();
        Ok(())
    }

    // ── authenticated calls ──────────────────────────────────────────────────────────────────────────────────

    fn call_session(&mut self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let id = self.session.as_ref().map(|s| s.session_id.clone()).ok_or(AormsError::SignedOut)?;
        self.api(method, path, body, Some(&id))
    }

    /// One authenticated JSON call. 401 → refresh once → retry. A `session_replaced` or `not_connected` answer ends the local session.
    fn api(&mut self, method: Method, path: &str, body: Option<Value>, session_id: Option<&str>) -> Result<Value> {
        self.ensure_fresh()?;
        for attempt in 0..2 {
            let access = self.access.clone().ok_or(AormsError::SignedOut)?;
            let mut headers = vec![("Authorization".to_string(), format!("Bearer {access}"))];
            if let Some(id) = session_id {
                headers.push(("x-aqc-session".into(), id.to_string()));
            }
            let req =
                Request { method, url: self.url(path), headers, body: body.as_ref().map_or(Body::None, |b| Body::Json(b.to_string().into_bytes())) };
            let res = self.transport.send(&req)?;
            if res.status == 401 && attempt == 0 {
                self.do_refresh()?;
                continue;
            }
            if res.status >= 400 {
                let e = AormsError::from_body(res.status, &res.body);
                if e.is_session_replaced() || e.is_not_connected() {
                    self.session = None;
                }
                if res.status == 401 {
                    self.clear_local();
                    return Err(AormsError::SignedOut);
                }
                return Err(e);
            }
            return Self::json_response(&res);
        }
        Err(AormsError::SignedOut)
    }

    /// Online projects (RLS-scoped to the user's studio) with their binding, if any.
    pub fn projects(&mut self) -> Result<Vec<OnlineProject>> {
        let v = self.call_session(Method::Get, "api/aqc/v1/projects", None)?;
        Ok(v.get("projects")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(OnlineProject::from_json).take(2000).collect())
            .unwrap_or_default())
    }

    /// Title block, parties, drawings and the binding for a project (the portal's "seed").
    pub fn seed(&mut self, project_office_id: &str) -> Result<Value> {
        self.call_session(Method::Get, &format!("api/aqc/v1/seed/{}", encode(project_office_id)), None)
    }

    /// The project's shared settings (levels, markups …): the first rows page of a bound project. `Null` when nothing is stored yet.
    pub fn project_settings(&mut self, aqc_project_id: &str) -> Result<Value> {
        let v = self.call_session(Method::Get, &format!("api/aqc/v1/projects/{}/rows?since=0", encode(aqc_project_id)), None)?;
        Ok(v.get("settings").cloned().unwrap_or(Value::Null))
    }
}

fn encode(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect()
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{MemorySessionStore, MemorySharedStore};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    type Script = Arc<Mutex<Vec<(String, VecDeque<Response>)>>>;

    /// Scripted transport: routes by "METHOD path"; the last answer for a route repeats. Records every request.
    #[derive(Clone, Default)]
    struct Fake {
        script: Script,
        seen: Arc<Mutex<Vec<Request>>>,
        fail_network: Arc<Mutex<bool>>,
    }
    impl Fake {
        fn on(self, route: &str, status: u16, body: &str) -> Fake {
            if let Ok(mut s) = self.script.lock() {
                let r = Response { status, body: body.as_bytes().to_vec() };
                match s.iter_mut().find(|(k, _)| k == route) {
                    Some((_, q)) => q.push_back(r),
                    None => s.push((route.to_string(), VecDeque::from([r]))),
                }
            }
            self
        }
        fn requests(&self) -> Vec<Request> {
            self.seen.lock().map(|g| g.clone()).unwrap_or_default()
        }
        fn count(&self, needle: &str) -> usize {
            self.requests().iter().filter(|r| r.url.contains(needle)).count()
        }
    }
    impl Transport for Fake {
        fn send(&self, req: &Request) -> Result<Response> {
            if self.fail_network.lock().map(|g| *g).unwrap_or(false) {
                return Err(AormsError::Network("down".into()));
            }
            if let Ok(mut s) = self.seen.lock() {
                s.push(req.clone());
            }
            let key = format!(
                "{} {}",
                if req.method == Method::Get {
                    "GET"
                } else if req.method == Method::Post {
                    "POST"
                } else {
                    "PUT"
                },
                req.url.split("://").nth(1).and_then(|r| r.split_once('/')).map_or("", |(_, p)| p)
            );
            let mut s = self.script.lock().map_err(|_| AormsError::Network("lock".into()))?;
            // the most specific route wins ("GET api/aqc/v1/projects/q1/rows" over "GET api/aqc/v1/projects")
            let found = s.iter_mut().filter(|(k, _)| key.starts_with(k.as_str())).max_by_key(|(k, _)| k.len());
            match found {
                Some((_, q)) => Ok(if q.len() > 1 {
                    q.pop_front().unwrap_or(Response { status: 500, body: vec![] })
                } else {
                    q.front().cloned().unwrap_or(Response { status: 500, body: vec![] })
                }),
                None => Ok(Response { status: 404, body: b"{}".to_vec() }),
            }
        }
    }

    const CFG: &str = r#"{"contract":"2026-10-aqc-1","supabaseUrl":"https://hub.example","supabaseAnonKey":"anon","limits":{}}"#;
    fn login(rt: &str) -> String {
        format!(r#"{{"accessToken":"a1","refreshToken":"{rt}","expiresAt":1003600,"userId":"u1","email":"a@x.in"}}"#)
    }
    fn session(id: &str, connected: bool) -> String {
        format!(
            r#"{{"contract":"2026-10-aqc-1","sessionId":"{id}","account":{{"id":"u1","email":"a@x.in"}},"studio":{{"firmId":"f1","name":"Aurelia","publicId":"AORMS-S-1"}},"role":"OWNER","capabilities":{{"write":true,"fees:manage":true,"cost:approve":false}},"entitlement":{{"connected":{connected},"plan":"PROFESSIONAL","expiresAt":null}}}}"#
        )
    }
    fn refreshed(at: &str, rt: &str) -> String {
        format!(r#"{{"access_token":"{at}","refresh_token":"{rt}","expires_at":1007200}}"#)
    }
    fn base() -> Fake {
        Fake::default().on("GET api/aqc/v1/config", 200, CFG).on("POST api/aqc/v1/auth/login", 200, &login("r1")).on(
            "POST api/aqc/v1/session",
            200,
            &session("s1", true),
        )
    }
    fn client(f: &Fake, shared: &MemorySharedStore, local: &MemorySessionStore, now: Arc<Mutex<i64>>) -> Client {
        Client::with_clock(
            "https://aorms.example/",
            Box::new(f.clone()),
            Box::new(shared.clone()),
            Box::new(local.clone()),
            Box::new(move || now.lock().map(|g| *g).unwrap_or(0)),
        )
    }
    fn body_of(r: &Request) -> Value {
        match &r.body {
            Body::Json(b) => serde_json::from_slice(b).unwrap_or(Value::Null),
            _ => Value::Null,
        }
    }

    #[test]
    fn sign_in_starts_an_aad_session_and_shares_the_refresh_token() {
        let f = base();
        let (sh, lo) = (MemorySharedStore::default(), MemorySessionStore::default());
        let mut c = client(&f, &sh, &lo, Arc::new(Mutex::new(1_000_000)));
        let s = c.sign_in("a@x.in", "pw", "AAD 0.5 · PC").unwrap_or_default();
        assert_eq!(s.session_id, "s1");
        assert!(c.is_connected() && c.is_signed_in());
        let post = f.requests().into_iter().find(|r| r.url.ends_with("/session")).map(|r| body_of(&r)).unwrap_or(Value::Null);
        assert_eq!(post["app"], "aad");
        assert_eq!(sh.load().map(|s| s.refresh_token), Some("r1".to_string()));
        assert_eq!(lo.load(), Some("s1".to_string()));
    }

    #[test]
    fn a_server_with_another_contract_is_refused() {
        let f = Fake::default().on("GET api/aqc/v1/config", 200, r#"{"contract":"2099-x"}"#);
        let mut c = client(&f, &MemorySharedStore::default(), &MemorySessionStore::default(), Arc::new(Mutex::new(0)));
        assert_eq!(c.sign_in("a", "b", "c").err(), Some(AormsError::Contract("2099-x".into())));
    }

    #[test]
    fn wrong_password_and_unlinked_accounts_surface_their_codes() {
        let f = Fake::default().on("GET api/aqc/v1/config", 200, CFG).on(
            "POST api/aqc/v1/auth/login",
            401,
            r#"{"error":{"code":"invalid_credentials","message":"Invalid login credentials"}}"#,
        );
        let mut c = client(&f, &MemorySharedStore::default(), &MemorySessionStore::default(), Arc::new(Mutex::new(0)));
        let e = c.sign_in("a", "b", "c").err();
        assert_eq!(e.as_ref().map(AormsError::code), Some("invalid_credentials"));
        assert!(!c.is_signed_in());
    }

    #[test]
    fn a_studio_without_connected_mode_is_not_connected_with_its_reason() {
        let g = Fake::default().on("GET api/aqc/v1/config", 200, CFG).on("POST api/aqc/v1/auth/login", 200, &login("r1")).on(
            "POST api/aqc/v1/session",
            403,
            r#"{"error":{"code":"not_connected","message":"no","reason":"EXPIRED"}}"#,
        );
        let mut c = client(&g, &MemorySharedStore::default(), &MemorySessionStore::default(), Arc::new(Mutex::new(1_000_000)));
        let e = c.sign_in("a", "b", "c").err();
        assert!(e.as_ref().is_some_and(AormsError::is_not_connected));
        assert!(matches!(e, Some(AormsError::Api { reason: Some(ref r), .. }) if r == "EXPIRED"));
        assert!(!c.is_connected());
    }

    #[test]
    fn restore_without_a_stored_sign_in_is_community_mode() {
        let mut c = client(&base(), &MemorySharedStore::default(), &MemorySessionStore::default(), Arc::new(Mutex::new(0)));
        assert_eq!(c.restore().ok(), Some(RestoreOutcome::NoStoredSession));
    }

    #[test]
    fn restore_from_another_apps_sign_in_needs_a_session_and_never_steals_one() {
        let sh = MemorySharedStore::default();
        sh.save(&SharedSignIn { refresh_token: "r-from-aqc".into(), email: Some("a@x.in".into()), updated_at: 1 });
        let f = base().on("POST auth/v1/token", 200, &refreshed("a2", "r2"));
        let lo = MemorySessionStore::default();
        let mut c = client(&f, &sh, &lo, Arc::new(Mutex::new(1_000_000)));
        assert_eq!(c.restore().ok(), Some(RestoreOutcome::NeedsSession));
        assert_eq!(f.count("/session"), 0); // no POST /session: AAD did not start (or replace) anything
        assert_eq!(body_of(f.requests().iter().find(|r| r.url.contains("auth/v1/token")).unwrap_or(&f.requests()[0]))["refresh_token"], "r-from-aqc");
        assert_eq!(sh.load().map(|s| s.refresh_token), Some("r2".to_string())); // rotated token handed back to the other app
        assert!(c.is_signed_in() && !c.is_connected());
    }

    #[test]
    fn restore_with_a_session_id_resumes_it_and_reports_a_replaced_one() {
        let sh = MemorySharedStore::default();
        sh.save(&SharedSignIn { refresh_token: "r1".into(), email: None, updated_at: 1 });
        let lo = MemorySessionStore::default();
        lo.save("s1");
        let f = base().on("POST auth/v1/token", 200, &refreshed("a2", "r2")).on("GET api/aqc/v1/session", 200, &session("s1", true));
        let mut c = client(&f, &sh, &lo, Arc::new(Mutex::new(1_000_000)));
        assert_eq!(c.restore().ok(), Some(RestoreOutcome::Restored));
        assert!(c.is_connected());

        let g = base().on("POST auth/v1/token", 200, &refreshed("a2", "r2")).on(
            "GET api/aqc/v1/session",
            409,
            r#"{"error":{"code":"session_replaced","message":"moved"}}"#,
        );
        let mut d = client(&g, &sh, &lo, Arc::new(Mutex::new(1_000_000)));
        assert_eq!(d.restore().ok(), Some(RestoreOutcome::SessionReplaced));
        assert!(!d.is_connected() && d.is_signed_in());
    }

    #[test]
    fn refresh_uses_the_token_another_app_rotated_and_hands_the_new_one_back() {
        let (sh, lo) = (MemorySharedStore::default(), MemorySessionStore::default());
        let now = Arc::new(Mutex::new(1_000_000));
        let f = base().on("POST auth/v1/token", 200, &refreshed("a2", "r3")).on("GET api/aqc/v1/projects", 200, r#"{"projects":[]}"#);
        let mut c = client(&f, &sh, &lo, now.clone());
        c.sign_in("a@x.in", "pw", "t").ok();
        sh.save(&SharedSignIn { refresh_token: "r2".into(), email: None, updated_at: 5 }); // AQC rotated r1 → r2
        if let Ok(mut n) = now.lock() {
            *n = 1_003_580; // inside the refresh skew
        }
        assert!(c.projects().is_ok());
        let sent = f.requests().iter().find(|r| r.url.contains("auth/v1/token")).map(body_of).unwrap_or(Value::Null);
        assert_eq!(sent["refresh_token"], "r2"); // never the stale r1
        assert_eq!(sh.load().map(|s| s.refresh_token), Some("r3".to_string()));
    }

    #[test]
    fn a_refused_refresh_with_nothing_newer_signs_out_and_clears_the_dead_token() {
        let (sh, lo) = (MemorySharedStore::default(), MemorySessionStore::default());
        let now = Arc::new(Mutex::new(1_000_000));
        let f = base().on("POST auth/v1/token", 400, r#"{"error":"invalid_grant"}"#);
        let mut c = client(&f, &sh, &lo, now.clone());
        c.sign_in("a@x.in", "pw", "t").ok();
        if let Ok(mut n) = now.lock() {
            *n = 1_003_580;
        }
        assert_eq!(c.projects().err(), Some(AormsError::SignedOut));
        assert_eq!(f.count("auth/v1/token"), 1); // no retry with the same token
        assert!(sh.load().is_none() && !c.is_signed_in());
    }

    #[test]
    fn a_busy_auth_server_does_not_sign_anyone_out() {
        let (sh, lo) = (MemorySharedStore::default(), MemorySessionStore::default());
        let now = Arc::new(Mutex::new(1_000_000));
        let f = base().on("POST auth/v1/token", 503, "{}");
        let mut c = client(&f, &sh, &lo, now.clone());
        c.sign_in("a@x.in", "pw", "t").ok();
        if let Ok(mut n) = now.lock() {
            *n = 1_003_580;
        }
        let e = c.projects().err();
        assert_eq!(e.as_ref().map(AormsError::code), Some("unavailable"));
        assert!(c.is_signed_in() && sh.load().is_some());
    }

    #[test]
    fn being_offline_is_a_network_error_not_a_sign_out() {
        let (sh, lo) = (MemorySharedStore::default(), MemorySessionStore::default());
        let f = base();
        let mut c = client(&f, &sh, &lo, Arc::new(Mutex::new(1_000_000)));
        c.sign_in("a@x.in", "pw", "t").ok();
        if let Ok(mut g) = f.fail_network.lock() {
            *g = true;
        }
        assert!(matches!(c.projects(), Err(AormsError::Network(_))));
        assert!(c.is_signed_in());
    }

    #[test]
    fn session_replaced_ends_the_local_session_and_keeps_the_sign_in() {
        let f = base().on("GET api/aqc/v1/projects", 409, r#"{"error":{"code":"session_replaced","message":"moved"}}"#);
        let mut c = client(&f, &MemorySharedStore::default(), &MemorySessionStore::default(), Arc::new(Mutex::new(1_000_000)));
        c.sign_in("a@x.in", "pw", "t").ok();
        assert!(c.projects().err().is_some_and(|e| e.is_session_replaced()));
        assert!(!c.is_connected() && c.is_signed_in());
    }

    #[test]
    fn sign_out_clears_the_shared_login_and_this_apps_session() {
        let (sh, lo) = (MemorySharedStore::default(), MemorySessionStore::default());
        let mut c = client(&base(), &sh, &lo, Arc::new(Mutex::new(1_000_000)));
        c.sign_in("a@x.in", "pw", "t").ok();
        c.sign_out();
        assert!(sh.load().is_none() && lo.load().is_none() && !c.is_signed_in());
    }

    #[test]
    fn projects_seed_and_settings_read_the_contract_shapes() {
        let f = base()
            .on("GET api/aqc/v1/projects", 200, r#"{"projects":[{"projectOfficeId":"p1","ref":"P-1","title":"Lakeview","status":"ACTIVE","city":"Hospet","aqc":{"id":"q1","headSeq":12,"formatVersion":17,"editedByOther":true}},{"projectOfficeId":"p2","ref":"P-2","title":"Hill","aqc":null},{"title":"no id"}]}"#)
            .on("GET api/aqc/v1/seed/p1", 200, r#"{"project":{"name":"Lakeview"}}"#)
            .on("GET api/aqc/v1/projects/q1/rows", 200, r#"{"settings":{"levels":[]},"rows":[],"headSeq":12,"more":false}"#);
        let mut c = client(&f, &MemorySharedStore::default(), &MemorySessionStore::default(), Arc::new(Mutex::new(1_000_000)));
        c.sign_in("a@x.in", "pw", "t").ok();
        let ps = c.projects().unwrap_or_default();
        assert_eq!(ps.len(), 2); // the entry without an id is skipped
        assert!(ps[0].binding.as_ref().is_some_and(|b| b.edited_by_other && b.head_seq == 12));
        assert!(ps[1].binding.is_none());
        assert_eq!(c.seed("p1").ok().and_then(|v| v["project"]["name"].as_str().map(str::to_string)), Some("Lakeview".into()));
        assert!(c.project_settings("q1").ok().is_some_and(|v| v.get("levels").is_some()));
        let h = f.requests();
        assert!(
            h.iter()
                .filter(|r| r.url.contains("/projects") || r.url.contains("/seed/"))
                .all(|r| r.headers.iter().any(|(k, v)| k == "x-aqc-session" && v == "s1"))
        );
    }

    #[test]
    fn ids_are_sanitised_before_they_reach_a_url() {
        assert_eq!(encode("p1/../../etc?x=1"), "p1etcx1");
        assert_eq!(encode("0f8e-AA_1"), "0f8e-AA_1");
    }

    #[test]
    fn hostile_error_bodies_become_generic_errors() {
        let e = AormsError::from_body(500, b"<html>boom</html>");
        assert_eq!((e.status(), e.code()), (Some(500), "unavailable"));
        let e = AormsError::from_body(418, b"\xff\xfe");
        assert_eq!(e.code(), "error");
    }
}
