//! AORMS connection commands: sign in to the shared AORMS login, list the studio's projects and pull project information into the drawing
//! (details, floors). Optional: signed out, AAD is a complete standalone program. The contract and rules are in
//! `docs/esti/AORMS-SUITE-SIGNIN.md` of the AORMS repository; the client is `aad-aorms`.
//!
//! Portal data reaches the drawing only on `aorms.open` / `aorms.pull`, and only project details and floors — never drawn geometry.
//! The password is used once for the sign-in call and never stored, logged or returned.

use aad_aorms::{AormsError, Client, RestoreOutcome};
use serde_json::{Value, json};

use super::*;
use crate::{EngineError, Result, Session};

/// The connection of this session. `None` until the first sign-in or restore.
#[derive(Default)]
pub struct AormsState {
    pub client: Option<Client>,
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("aorms.signin", "Sign in to AORMS", run_signin)
            .enabled(always)
            .noundo()
            .menu(&["AORMS", "Sign in"])
            .params("{email, password, baseUrl?: https://… (default https://aorms.in or $AORMS_URL)} → {signedIn, connected, studio, role, plan}"),
        CommandSpec::new("aorms.restore", "Restore AORMS sign-in", run_restore).enabled(always).noundo().params(
            "{baseUrl?} → {outcome: noStoredSession|restored|needsSession|sessionReplaced|notConnected|signedOut} (never takes a session over)",
        ),
        CommandSpec::new("aorms.signout", "Sign out of AORMS", run_signout)
            .enabled(always)
            .noundo()
            .menu(&["AORMS", "Sign out"])
            .params("{} → {signedIn: false} (also ends the shared sign-in used by other AORMS apps)"),
        CommandSpec::new("aorms.status", "AORMS status", run_status)
            .enabled(always)
            .noundo()
            .menu(&["AORMS", "Status"])
            .params("{} → {signedIn, connected, email?, studio?, role?, plan?, baseUrl?, link?}"),
        CommandSpec::new("aorms.projects", "AORMS projects", run_projects)
            .enabled(always)
            .noundo()
            .menu(&["AORMS", "Projects"])
            .params("{} → {projects: [{projectOfficeId, ref, title, status?, city?, bound}]}"),
        CommandSpec::new("aorms.open", "Pull project from AORMS", run_open)
            .menu(&["AORMS", "Pull project info"])
            .params("{projectOfficeId} → {details, floors?, linked, warnings} (project details and floors only; links this drawing to the project)"),
        CommandSpec::new("aorms.pull", "Refresh project info from AORMS", run_pull)
            .menu(&["AORMS", "Refresh project info"])
            .params("{} → as aorms.open, for the project this drawing is linked to"),
        CommandSpec::new("aorms.apply", "Apply AORMS project info", run_apply)
            .params("{projectOfficeId, seed, settings} → as aorms.open. The drawing half of a pull whose network half ran elsewhere (the UI fetches on a worker thread); one undo step"),
        CommandSpec::new("aorms.expect", "Check AORMS project link", run_expect)
            .enabled(always)
            .noundo()
            .params("{projectOfficeId} → {status: linked|unlinked|other, linked?} (offline: compares the project this drawing is linked to with the one a launcher expects, and says so on the command line when they differ)"),
        CommandSpec::new("aorms.drawings", "AORMS drawings", run_drawings)
            .enabled(always)
            .noundo()
            .menu(&["AORMS", "Drawings"])
            .params("{projectOfficeId?} → {drawings: [{id, ref, title, rev, fileName, sizeBytes}]} (default: the linked project)"),
    ]
}

fn fail(e: AormsError) -> EngineError {
    match &e {
        AormsError::Api { reason: Some(r), .. } if e.is_not_connected() => {
            EngineError::Other(format!("AORMS connected mode isn't available for this studio ({r}). AAD keeps working without it."))
        }
        _ => EngineError::Other(format!("AORMS: {e}")),
    }
}

fn text<'a>(cmd: &str, p: &'a Value, key: &str, max: usize) -> Result<&'a str> {
    let t =
        p.get(key).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).ok_or_else(|| bad(cmd, format!("`{key}` (text) is required")))?;
    if t.len() > max {
        return Err(bad(cmd, format!("`{key}` is too long")));
    }
    Ok(t)
}

/// Only https, or http to this computer (a local AORMS for development). No credentials in the URL.
pub fn valid_base(u: &str) -> bool {
    let rest = if let Some(r) = u.strip_prefix("https://") {
        r
    } else if let Some(r) = u.strip_prefix("http://") {
        let host = r.split(['/', ':']).next().unwrap_or("");
        if host != "localhost" && host != "127.0.0.1" {
            return false;
        }
        r
    } else {
        return false;
    };
    u.len() <= 200 && !rest.is_empty() && !rest.contains('@') && !u.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn base_url(cmd: &str, p: &Value) -> Result<String> {
    let base = match p.get("baseUrl").and_then(Value::as_str) {
        Some(b) => b.trim().to_string(),
        None => std::env::var("AORMS_URL").ok().filter(|b| !b.trim().is_empty()).unwrap_or_else(|| "https://aorms.in".into()),
    };
    if valid_base(&base) {
        Ok(base.trim_end_matches('/').to_string())
    } else {
        Err(bad(cmd, "`baseUrl` must be https:// (or http://localhost for development)"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn make_client(base: &str) -> Result<Client> {
    use aad_aorms::UreqTransport;
    #[cfg(feature = "aorms-keyring")]
    let (shared, local): (Box<dyn aad_aorms::SharedSignInStore>, Box<dyn aad_aorms::SessionStore>) =
        (Box::new(aad_aorms::KeyringStore::shared()), Box::new(aad_aorms::KeyringStore::session()));
    #[cfg(not(feature = "aorms-keyring"))]
    let (shared, local): (Box<dyn aad_aorms::SharedSignInStore>, Box<dyn aad_aorms::SessionStore>) =
        (Box::new(aad_aorms::MemorySharedStore::default()), Box::new(aad_aorms::MemorySessionStore::default()));
    Ok(Client::new(base, Box::new(UreqTransport::default()), shared, local))
}

#[cfg(target_arch = "wasm32")]
pub fn make_client(_base: &str) -> Result<Client> {
    Err(EngineError::Other("AORMS is not available in the web build".into()))
}

/// The session's client, created on first use (an injected one, e.g. in tests, is kept).
fn client<'a>(s: &'a mut Session, cmd: &str, p: &Value) -> Result<&'a mut Client> {
    if s.aorms.client.is_none() {
        let base = base_url(cmd, p)?;
        s.aorms.client = Some(make_client(&base)?);
    }
    s.aorms.client.as_mut().ok_or_else(|| EngineError::Other("AORMS client unavailable".into()))
}

/// The label this computer shows in AORMS ("AAD <version> · <computer>").
pub fn client_label() -> String {
    format!("O-Draft {} · {}", env!("CARGO_PKG_VERSION"), computer())
}

fn computer() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "computer".into()).chars().take(40).collect()
}

/// `{signedIn, connected, email?, studio?, role?, plan?, canWrite?}` for a client.
pub fn summary(c: &Client) -> Value {
    match c.session() {
        Some(si) => json!({
            "signedIn": c.is_signed_in(), "connected": c.is_connected(), "email": si.email, "studio": si.studio.name, "role": si.role, "plan": si.plan,
            "canWrite": si.capabilities.write,
        }),
        None => json!({ "signedIn": c.is_signed_in(), "connected": false, "email": c.email() }),
    }
}

fn run_signin(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "aorms.signin";
    let email = text(C, p, "email", 254)?;
    if !email.contains('@') {
        return Err(bad(C, "`email` must be an email address"));
    }
    let password = p
        .get("password")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty() && t.len() <= 200)
        .ok_or_else(|| bad(C, "`password` (text) is required"))?;
    let label = client_label();
    let c = client(s, C, p)?;
    c.sign_in(email, password, &label).map_err(fail)?;
    s.echo("Signed in to AORMS.");
    Ok(summary(client(s, C, p)?))
}

fn run_restore(s: &mut Session, p: &Value) -> Result<Value> {
    let c = client(s, "aorms.restore", p)?;
    let outcome = c.restore().map_err(fail)?;
    let name = match outcome {
        RestoreOutcome::NoStoredSession => "noStoredSession",
        RestoreOutcome::Restored => "restored",
        RestoreOutcome::NeedsSession => "needsSession",
        RestoreOutcome::SessionReplaced => "sessionReplaced",
        RestoreOutcome::NotConnected => "notConnected",
        RestoreOutcome::SignedOut => "signedOut",
    };
    Ok(json!({ "outcome": name, "status": summary(c) }))
}

fn run_signout(s: &mut Session, _p: &Value) -> Result<Value> {
    if let Some(c) = s.aorms.client.as_mut() {
        c.sign_out();
    }
    s.aorms.client = None;
    s.echo("Signed out of AORMS.");
    Ok(json!({ "signedIn": false }))
}

fn linked(s: &Session) -> Value {
    s.doc().map(|d| project::get(d, "aorms")).unwrap_or(Value::Null)
}

fn run_status(s: &mut Session, _p: &Value) -> Result<Value> {
    let link = linked(s);
    let mut out = match s.aorms.client.as_ref() {
        Some(c) => {
            let mut v = summary(c);
            v["baseUrl"] = json!(c.base_url());
            v
        }
        None => json!({ "signedIn": false, "connected": false }),
    };
    if !link.is_null() {
        out["link"] = link;
    }
    Ok(out)
}

fn need_connected<'a>(s: &'a mut Session, cmd: &str) -> Result<&'a mut Client> {
    let c =
        s.aorms.client.as_mut().filter(|c| c.is_signed_in()).ok_or_else(|| EngineError::Other("Sign in to AORMS first (aorms.signin).".into()))?;
    if !c.is_connected() {
        return Err(EngineError::Other(format!("`{cmd}` needs a current AORMS session — sign in again (aorms.signin).")));
    }
    Ok(c)
}

fn run_projects(s: &mut Session, _p: &Value) -> Result<Value> {
    let c = need_connected(s, "aorms.projects")?;
    let list = c.projects().map_err(fail)?;
    Ok(json!({ "projects": list.iter().map(|p| json!({
        "projectOfficeId": p.project_office_id, "ref": p.reference, "title": p.title, "status": p.status, "city": p.city, "bound": p.binding.is_some(),
    })).collect::<Vec<_>>() }))
}

pub fn id_param(cmd: &str, v: &str) -> Result<String> {
    if !v.is_empty() && v.len() <= 64 && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        Ok(v.to_string())
    } else {
        Err(bad(cmd, "`projectOfficeId` must be a project id"))
    }
}

/// The network half of a pull: the portal's seed and the project's shared settings. Touches no drawing, so the UI runs it on a worker
/// thread while the drawing stays responsive. The caller has checked the client is connected.
pub fn fetch_project(c: &mut Client, project_id: &str) -> std::result::Result<(Value, Value), AormsError> {
    let seed = c.seed(project_id)?;
    let aqc_id = seed.get("aqc").and_then(|a| a.get("id")).and_then(Value::as_str).map(str::to_string);
    let settings = match &aqc_id {
        Some(id) => c.project_settings(id)?,
        None => Value::Null,
    };
    Ok((seed, settings))
}

/// The drawing half of a pull: project details and floors into the drawing's project data, and the link to the project. Never touches
/// drawn geometry. Called directly it records no undo step; the `aorms.apply` command wraps it so the UI's pull is one undo step.
pub fn apply_project(s: &mut Session, project_id: &str, seed: &Value, settings: &Value) -> Result<Value> {
    let mut warnings = Vec::<String>::new();
    let details = aad_aorms::mapping::details_from_seed(seed);
    let mut out = json!({});
    if !details.is_empty() {
        out["details"] = project::run_project(s, &json!({ "details": Value::Object(details) }))?["details"].clone();
    }
    let existing = project::floors(s.doc()?);
    match aad_aorms::mapping::floors_from_settings(settings, &existing) {
        Some(floors) => match project::run_floors(s, &json!({ "floors": floors })) {
            Ok(v) => out["floors"] = v["floors"].clone(),
            Err(e) => warnings.push(format!("floors not updated: {e}")),
        },
        None if seed.get("aqc").is_some_and(|a| !a.is_null()) => warnings.push("the project has no levels yet; floors left as they are".into()),
        None => warnings.push("the project is not bound to a take-off yet; floors left as they are".into()),
    }
    let link = json!({
        "projectOfficeId": project_id,
        "ref": seed["projectOffice"]["ref"],
        "aqcId": seed["aqc"]["id"],
        "headSeq": seed["aqc"]["headSeq"],
    });
    project::set(s.doc_mut()?, "aorms", &link);
    out["linked"] = link;
    out["warnings"] = json!(warnings);
    Ok(out)
}

fn pull_into_drawing(s: &mut Session, project_id: &str) -> Result<Value> {
    let (seed, settings) = {
        let c = need_connected(s, "aorms.open")?;
        fetch_project(c, project_id).map_err(fail)?
    };
    apply_project(s, project_id, &seed, &settings)
}

fn run_open(s: &mut Session, p: &Value) -> Result<Value> {
    let id = id_param("aorms.open", text("aorms.open", p, "projectOfficeId", 64)?)?;
    pull_into_drawing(s, &id)
}

fn run_apply(s: &mut Session, p: &Value) -> Result<Value> {
    let id = id_param("aorms.apply", text("aorms.apply", p, "projectOfficeId", 64)?)?;
    let seed = p.get("seed").filter(|v| v.is_object()).ok_or_else(|| bad("aorms.apply", "`seed` (object) is required"))?;
    let settings = p.get("settings").cloned().unwrap_or(Value::Null);
    apply_project(s, &id, seed, &settings)
}

fn run_expect(s: &mut Session, p: &Value) -> Result<Value> {
    let want = id_param("aorms.expect", text("aorms.expect", p, "projectOfficeId", 64)?)?;
    let link = linked(s);
    let have = link.get("projectOfficeId").and_then(Value::as_str).map(str::to_string);
    let (status, message) = match have.as_deref() {
        Some(h) if h == want => ("linked", None),
        Some(h) => ("other", Some(format!("This drawing is linked to project {h}, not {want}."))),
        None => ("unlinked", Some(format!("This drawing is not linked to project {want}; use aorms.open to link it."))),
    };
    if let Some(m) = message {
        s.echo(m);
    }
    Ok(json!({ "status": status, "linked": have }))
}

fn run_pull(s: &mut Session, _p: &Value) -> Result<Value> {
    let link = linked(s);
    let id = link
        .get("projectOfficeId")
        .and_then(Value::as_str)
        .ok_or_else(|| EngineError::Other("This drawing isn't linked to an AORMS project; use aorms.open first.".into()))?;
    let id = id_param("aorms.pull", id)?;
    pull_into_drawing(s, &id)
}

fn run_drawings(s: &mut Session, p: &Value) -> Result<Value> {
    let id = match p.get("projectOfficeId").and_then(Value::as_str) {
        Some(v) => id_param("aorms.drawings", v.trim())?,
        None => id_param("aorms.drawings", linked(s).get("projectOfficeId").and_then(Value::as_str).unwrap_or(""))?,
    };
    let c = need_connected(s, "aorms.drawings")?;
    let seed = c.seed(&id).map_err(fail)?;
    let list: Vec<Value> = seed
        .get("drawings")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .take(500)
                .map(|d| json!({ "id": d["id"], "ref": d["ref"], "title": d["title"], "rev": d["rev"], "fileName": d["fileName"], "sizeBytes": d["sizeBytes"] }))
                .collect()
        })
        .unwrap_or_default();
    Ok(json!({ "drawings": list }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aad_aorms::{MemorySessionStore, MemorySharedStore, Method, Request, Response, Transport};
    use std::sync::{Arc, Mutex};

    /// Scripted AORMS: answers by "METHOD path-prefix" (longest wins). Records requests.
    #[derive(Clone, Default)]
    struct Script {
        routes: Arc<Mutex<Vec<(String, u16, String)>>>,
        seen: Arc<Mutex<Vec<Request>>>,
    }
    impl Script {
        fn on(self, route: &str, status: u16, body: &str) -> Script {
            if let Ok(mut r) = self.routes.lock() {
                r.push((route.into(), status, body.into()));
            }
            self
        }
    }
    impl Transport for Script {
        fn send(&self, req: &Request) -> std::result::Result<Response, AormsError> {
            if let Ok(mut s) = self.seen.lock() {
                s.push(req.clone());
            }
            let m = match req.method {
                Method::Get => "GET",
                Method::Post => "POST",
                Method::Put => "PUT",
            };
            let path = req.url.split("://").nth(1).and_then(|r| r.split_once('/')).map_or("", |(_, p)| p);
            let key = format!("{m} {path}");
            let routes = self.routes.lock().map_err(|_| AormsError::Network("lock".into()))?;
            let hit = routes.iter().filter(|(k, _, _)| key.starts_with(k.as_str())).max_by_key(|(k, _, _)| k.len());
            Ok(match hit {
                Some((_, st, body)) => Response { status: *st, body: body.clone().into_bytes() },
                None => Response { status: 404, body: b"{}".to_vec() },
            })
        }
    }

    const CFG: &str = r#"{"contract":"2026-10-aqc-1","supabaseUrl":"https://hub.example","supabaseAnonKey":"k"}"#;
    const LOGIN: &str = r#"{"accessToken":"a1","refreshToken":"r1","expiresAt":4000000000,"userId":"u1","email":"a@x.in"}"#;
    const SESSION: &str = r#"{"contract":"2026-10-aqc-1","sessionId":"s1","account":{"id":"u1","email":"a@x.in"},"studio":{"firmId":"f1","name":"Aurelia","publicId":"S1"},"role":"OWNER","capabilities":{"write":true,"fees:manage":true,"cost:approve":false},"entitlement":{"connected":true,"plan":"PROFESSIONAL","expiresAt":null}}"#;
    const PID: &str = "11111111-2222-3333-4444-555555555555";

    fn aorms() -> Script {
        Script::default()
            .on("GET api/aqc/v1/config", 200, CFG)
            .on("POST api/aqc/v1/auth/login", 200, LOGIN)
            .on("POST api/aqc/v1/session", 200, SESSION)
            .on("GET api/aqc/v1/projects", 200, r#"{"projects":[{"projectOfficeId":"11111111-2222-3333-4444-555555555555","ref":"P-1","title":"Lakeview","aqc":{"id":"q1","headSeq":3}}]}"#)
            .on(
                &format!("GET api/aqc/v1/seed/{PID}"),
                200,
                r#"{"project":{"name":"Lakeview Clubhouse","client_name":"Lakeview RA","company_name":"Aurelia Design","location":"Hospet"},"projectOffice":{"ref":"DEMO-PRJ-04","title":"Lakeview"},"drawings":[{"id":"d1","ref":"DRG-1","title":"GF plan","rev":2,"fileName":"gf.dxf","sizeBytes":1234}],"aqc":{"id":"q1","headSeq":3}}"#,
            )
            .on(
                "GET api/aqc/v1/projects/q1/rows",
                200,
                r#"{"settings":{"levels":[{"id":"Lvl0","name":"Plinth","height_mm":3200},{"id":"Lvl1","name":"First floor","height_mm":3000,"slab_thickness_mm":150,"beam_depth_mm":400}]},"rows":[],"headSeq":3,"more":false}"#,
            )
    }

    fn session_with(script: &Script) -> Session {
        let mut s = Session::new();
        s.aorms.client = Some(Client::new(
            "https://aorms.example",
            Box::new(script.clone()),
            Box::new(MemorySharedStore::default()),
            Box::new(MemorySessionStore::default()),
        ));
        s
    }
    fn signin(s: &mut Session) {
        assert!(s.execute("aorms.signin", &json!({ "email": "a@x.in", "password": "pw-secret" })).is_ok());
    }

    #[test]
    fn signin_reports_the_studio_and_never_echoes_the_password() {
        let mut s = session_with(&aorms());
        let v = s.execute("aorms.signin", &json!({ "email": "a@x.in", "password": "pw-secret" })).unwrap_or(Value::Null);
        assert_eq!(
            (v["signedIn"].as_bool(), v["connected"].as_bool(), v["studio"].as_str(), v["plan"].as_str()),
            (Some(true), Some(true), Some("Aurelia"), Some("PROFESSIONAL"))
        );
        assert!(!v.to_string().contains("pw-secret"));
        assert!(!s.log.iter().any(|l| l.contains("pw-secret")));
        let st = s.execute("aorms.status", &json!({})).unwrap_or(Value::Null);
        assert_eq!(st["email"], "a@x.in");
        assert!(!st.to_string().contains("pw-secret"));
    }

    #[test]
    fn expect_compares_the_linked_project_with_the_one_a_launcher_names() {
        let mut s = session_with(&aorms());
        let v = s.execute("aorms.expect", &json!({ "projectOfficeId": PID })).unwrap_or(Value::Null);
        assert_eq!(v["status"], "unlinked");
        assert!(s.log.iter().any(|l| l.contains("not linked")), "{:?}", s.log);
        signin(&mut s);
        s.execute("aorms.open", &json!({ "projectOfficeId": PID })).unwrap();
        let v = s.execute("aorms.expect", &json!({ "projectOfficeId": PID })).unwrap_or(Value::Null);
        assert_eq!((v["status"].as_str(), v["linked"].as_str()), (Some("linked"), Some(PID)));
        let v = s.execute("aorms.expect", &json!({ "projectOfficeId": "OTHER-1" })).unwrap_or(Value::Null);
        assert_eq!(v["status"], "other");
        assert!(s.execute("aorms.expect", &json!({ "projectOfficeId": "bad id; drop" })).is_err());
        assert!(s.execute("aorms.expect", &json!({})).is_err());
    }

    #[test]
    fn open_pulls_details_and_floors_and_links_the_drawing() {
        let mut s = session_with(&aorms());
        signin(&mut s);
        let v = s.execute("aorms.open", &json!({ "projectOfficeId": PID })).unwrap_or(Value::Null);
        assert_eq!(v["details"]["title"], "Lakeview Clubhouse");
        assert_eq!(v["details"]["owner"], "Lakeview RA");
        assert_eq!(v["details"]["pid"], "DEMO-PRJ-04");
        assert_eq!(v["floors"][0]["name"], "First floor");
        assert_eq!(v["floors"][0]["floorToFloor"], 3.0);
        assert_eq!(v["warnings"], json!([]));
        // it really landed in the drawing's project data
        let det = s.execute("project", &json!({})).unwrap_or(Value::Null);
        assert_eq!(det["details"]["architect"], "Aurelia Design");
        let fl = s.execute("floors", &json!({})).unwrap_or(Value::Null);
        assert_eq!(fl["floors"][0]["beam"], 400.0);
        let st = s.execute("aorms.status", &json!({})).unwrap_or(Value::Null);
        assert_eq!(st["link"]["projectOfficeId"], PID);
        assert_eq!(st["link"]["aqcId"], "q1");
    }

    #[test]
    fn open_changes_no_geometry() {
        let mut s = session_with(&aorms());
        signin(&mut s);
        s.execute("line", &json!({ "points": [[0, 0], [10, 0]] })).ok();
        let before = s.doc().map(|d| d.entity_count()).unwrap_or(0);
        assert!(before > 0);
        assert!(s.execute("aorms.open", &json!({ "projectOfficeId": PID })).is_ok());
        assert_eq!(s.doc().map(|d| d.entity_count()).unwrap_or(0), before);
    }

    #[test]
    fn open_is_one_undo_step_and_pull_repeats_it_for_the_linked_project() {
        let mut s = session_with(&aorms());
        signin(&mut s);
        assert!(s.execute("aorms.pull", &json!({})).is_err()); // not linked yet
        s.execute("aorms.open", &json!({ "projectOfficeId": PID })).ok();
        let again = s.execute("aorms.pull", &json!({})).unwrap_or(Value::Null);
        assert_eq!(again["linked"]["projectOfficeId"], PID);
    }

    #[test]
    fn an_unbound_project_pulls_details_and_says_why_floors_are_unchanged() {
        let script =
            aorms().on(&format!("GET api/aqc/v1/seed/{PID}"), 200, r#"{"project":{"name":"New job"},"projectOffice":{"ref":"P-9"},"aqc":null}"#);
        let mut s = session_with(&script);
        signin(&mut s);
        let v = s.execute("aorms.open", &json!({ "projectOfficeId": PID })).unwrap_or(Value::Null);
        assert_eq!(v["details"]["title"], "New job");
        assert!(v.get("floors").is_none());
        assert!(v["warnings"][0].as_str().is_some_and(|w| w.contains("not bound")));
    }

    #[test]
    fn nothing_works_signed_out_and_the_message_says_what_to_do() {
        let mut s = Session::new();
        for c in ["aorms.projects", "aorms.open"] {
            let e = s.execute(c, &json!({ "projectOfficeId": PID })).err().map(|e| e.to_string()).unwrap_or_default();
            assert!(e.contains("Sign in to AORMS first"), "{c}: {e}");
        }
        let st = s.execute("aorms.status", &json!({})).unwrap_or(Value::Null);
        assert_eq!((st["signedIn"].as_bool(), st["connected"].as_bool()), (Some(false), Some(false)));
    }

    #[test]
    fn signin_refuses_hostile_parameters_before_any_request() {
        let script = aorms();
        let mut s = session_with(&script);
        for p in [
            json!({}),
            json!({ "email": "no-at-sign", "password": "x" }),
            json!({ "email": "a@x.in" }),
            json!({ "email": "a@x.in", "password": "" }),
            json!({ "email": "a@x.in", "password": "x".repeat(300) }),
            json!({ "email": "a".repeat(300) + "@x.in", "password": "x" }),
            json!({ "email": 5, "password": ["x"] }),
        ] {
            assert!(s.execute("aorms.signin", &p).is_err(), "{p}");
        }
        assert!(script.seen.lock().map(|g| g.is_empty()).unwrap_or(false));
    }

    #[test]
    fn base_urls_must_be_https_or_local() {
        for ok in ["https://aorms.in", "https://aorms.example/", "http://localhost:3000", "http://127.0.0.1:3128"] {
            assert!(valid_base(ok), "{ok}");
        }
        for bad_url in [
            "http://evil.example",
            "ftp://aorms.in",
            "https://user:pw@aorms.in",
            "https://",
            "https://a b",
            "javascript:alert(1)",
            "",
            "http://localhost.evil.com",
        ] {
            assert!(!valid_base(bad_url), "{bad_url}");
        }
        let mut s = Session::new();
        assert!(s.execute("aorms.signin", &json!({ "email": "a@x.in", "password": "x", "baseUrl": "http://evil.example" })).is_err());
    }

    #[test]
    fn project_ids_cannot_smuggle_path_or_query() {
        let mut s = session_with(&aorms());
        signin(&mut s);
        for id in ["../../etc/passwd", "a?b=1", "a b", "", &"x".repeat(100), "id/with/slash"] {
            assert!(s.execute("aorms.open", &json!({ "projectOfficeId": id })).is_err(), "{id}");
        }
    }

    #[test]
    fn projects_and_drawings_list_what_the_portal_returns() {
        let mut s = session_with(&aorms());
        signin(&mut s);
        let p = s.execute("aorms.projects", &json!({})).unwrap_or(Value::Null);
        assert_eq!((p["projects"][0]["ref"].as_str(), p["projects"][0]["bound"].as_bool()), (Some("P-1"), Some(true)));
        let d = s.execute("aorms.drawings", &json!({ "projectOfficeId": PID })).unwrap_or(Value::Null);
        assert_eq!((d["drawings"][0]["fileName"].as_str(), d["drawings"][0]["rev"].as_i64()), (Some("gf.dxf"), Some(2)));
    }

    #[test]
    fn a_studio_without_connected_mode_gets_the_reason_and_drawing_still_works() {
        let script = aorms().on("POST api/aqc/v1/session", 403, r#"{"error":{"code":"not_connected","message":"no","reason":"PLAN_NOT_CONNECTED"}}"#);
        let mut s = session_with(&script);
        let e = s.execute("aorms.signin", &json!({ "email": "a@x.in", "password": "pw" })).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(e.contains("PLAN_NOT_CONNECTED") && e.contains("keeps working"), "{e}");
        assert!(s.execute("circle", &json!({ "center": [0, 0], "radius": 1 })).is_ok());
    }

    #[test]
    fn signout_forgets_everything_and_works_when_never_signed_in() {
        let mut s = session_with(&aorms());
        signin(&mut s);
        assert_eq!(s.execute("aorms.signout", &json!({})).unwrap_or(Value::Null)["signedIn"], false);
        assert!(s.aorms.client.is_none());
        assert!(s.execute("aorms.signout", &json!({})).is_ok());
    }
}
