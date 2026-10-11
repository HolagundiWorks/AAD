//! The AORMS sign-in dialog, chip and background jobs. The server is a scripted `Transport`, so nothing here touches a network.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cadcraft_aorms::{AormsError, Client, MemorySessionStore, MemorySharedStore, Method, Request, Response, Transport};
use cadcraft_engine::Session;
use cadcraft_ui_egui::aorms_ui::{self, ChipTone, Kind, Tone};
use cadcraft_ui_egui::{CadApp, Services, dialogs};
use serde_json::json;

const CFG: &str = r#"{"contract":"2026-10-aqc-1","supabaseUrl":"https://hub.example","supabaseAnonKey":"k"}"#;
const LOGIN: &str = r#"{"accessToken":"a1","refreshToken":"r1","expiresAt":4000000000,"userId":"u1","email":"a@x.in"}"#;
const SESSION: &str = r#"{"contract":"2026-10-aqc-1","sessionId":"s1","account":{"id":"u1","email":"a@x.in"},"studio":{"firmId":"f1","name":"Aurelia Design Collective","publicId":"S1"},"role":"OWNER","capabilities":{"write":true,"fees:manage":true,"cost:approve":false},"entitlement":{"connected":true,"plan":"PROFESSIONAL","expiresAt":null}}"#;
const PID: &str = "11111111-2222-3333-4444-555555555555";

#[derive(Clone, Default)]
struct Script {
    routes: Arc<Mutex<Vec<(String, u16, String)>>>,
    seen: Arc<Mutex<Vec<String>>>,
    delay_ms: u64,
}
impl Script {
    fn on(self, route: &str, status: u16, body: &str) -> Script {
        self.routes.lock().unwrap().push((route.into(), status, body.into()));
        self
    }
    fn calls(&self, needle: &str) -> usize {
        self.seen.lock().unwrap().iter().filter(|u| u.contains(needle)).count()
    }
}
impl Transport for Script {
    fn send(&self, req: &Request) -> Result<Response, AormsError> {
        if self.delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(self.delay_ms));
        }
        let m = match req.method {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
        };
        let path = req.url.split("://").nth(1).and_then(|r| r.split_once('/')).map_or("", |(_, p)| p);
        self.seen.lock().unwrap().push(format!("{m} {path}"));
        let key = format!("{m} {path}");
        let routes = self.routes.lock().unwrap();
        let hit = routes.iter().filter(|(k, _, _)| key.starts_with(k.as_str())).max_by_key(|(k, _, _)| k.len());
        Ok(match hit {
            Some((_, st, body)) => Response { status: *st, body: body.clone().into_bytes() },
            None => Response { status: 404, body: b"{}".to_vec() },
        })
    }
}

fn server() -> Script {
    Script::default()
        .on("GET api/aqc/v1/config", 200, CFG)
        .on("POST api/aqc/v1/auth/login", 200, LOGIN)
        .on("POST api/aqc/v1/session", 200, SESSION)
        .on("GET api/aqc/v1/projects", 200, &format!(r#"{{"projects":[{{"projectOfficeId":"{PID}","ref":"P-1","title":"Lakeview","aqc":{{"id":"q1","headSeq":3}}}},{{"projectOfficeId":"22222222-2222-3333-4444-555555555555","ref":"P-2","title":"Hill House","aqc":null}}]}}"#))
        .on(&format!("GET api/aqc/v1/seed/{PID}"), 200, r#"{"project":{"name":"Lakeview Clubhouse","client_name":"Lakeview RA","company_name":"Aurelia","location":"Hospet"},"projectOffice":{"ref":"DEMO-PRJ-04","title":"Lakeview"},"aqc":{"id":"q1","headSeq":3}}"#)
        .on("GET api/aqc/v1/projects/q1/rows", 200, r#"{"settings":{"levels":[{"id":"Lvl0","name":"Plinth","height_mm":3200},{"id":"Lvl1","name":"First floor","height_mm":3000,"slab_thickness_mm":150,"beam_depth_mm":400}]},"rows":[]}"#)
}

fn app_with(script: &Script) -> CadApp {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.session.aorms.client = Some(Client::new(
        "https://aorms.example",
        Box::new(script.clone()),
        Box::new(MemorySharedStore::default()),
        Box::new(MemorySessionStore::default()),
    ));
    app.aorms.base_url = "https://aorms.example".into(); // the server the injected client talks to
    app
}

/// Run frames until the background job has delivered (or fail after a few seconds).
fn settle(app: &mut CadApp, ctx: &egui::Context) {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        aorms_ui::poll(app, ctx);
        if !app.aorms.busy() {
            return;
        }
        assert!(Instant::now() < end, "the job never finished");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn type_credentials(app: &mut CadApp, email: &str, password: &str) {
    app.aorms.email = email.into();
    app.aorms.password = password.into();
}

#[test]
fn sign_in_runs_off_the_ui_thread_hands_the_client_back_and_clears_the_password() {
    let script = Script { delay_ms: 40, ..server() };
    let mut app = app_with(&script);
    let ctx = egui::Context::default();
    type_credentials(&mut app, "a@x.in", "pw-secret");
    let t0 = Instant::now();
    aorms_ui::start_sign_in(&mut app, &ctx);
    assert!(t0.elapsed() < Duration::from_millis(100), "starting a sign-in must not wait for the network");
    assert!(app.aorms.busy() && app.aorms.job_kind() == Some(Kind::SignIn));
    assert!(app.aorms.password.is_empty(), "the password leaves the text field the moment the job starts");
    assert!(app.session.aorms.client.is_none(), "the client is away on the worker");
    // a second click while busy does nothing
    type_credentials(&mut app, "a@x.in", "again");
    aorms_ui::start_sign_in(&mut app, &ctx);
    settle(&mut app, &ctx);
    assert_eq!(script.calls("POST api/aqc/v1/auth/login"), 1);
    assert!(app.session.aorms.client.is_some(), "the client comes back");
    let v = &app.aorms.view;
    assert!(v.signed_in && v.connected, "{v:?}");
    assert_eq!(v.studio, "Aurelia Design Collective");
    assert_eq!(app.aorms.projects.len(), 2, "the project list arrives with the sign-in");
    assert!(app.aorms.message.as_ref().is_some_and(|m| m.tone == Tone::Info));
    assert!(!format!("{:?}", app.aorms.message).contains("pw-secret"));
}

#[test]
fn a_wrong_password_is_explained_and_the_client_is_kept() {
    let script = Script::default().on("GET api/aqc/v1/config", 200, CFG).on(
        "POST api/aqc/v1/auth/login",
        401,
        r#"{"error":{"code":"invalid_credentials","message":"Invalid login credentials"}}"#,
    );
    let mut app = app_with(&script);
    let ctx = egui::Context::default();
    type_credentials(&mut app, "a@x.in", "nope");
    aorms_ui::start_sign_in(&mut app, &ctx);
    settle(&mut app, &ctx);
    let m = app.aorms.message.clone().unwrap();
    assert_eq!((m.tone, m.text.as_str()), (Tone::Error, "Invalid email or password."));
    assert!(app.aorms.password.is_empty());
    assert_eq!(app.aorms.email, "a@x.in", "the form keeps the e-mail");
    assert!(app.session.aorms.client.is_some() && !app.aorms.view.signed_in);
    assert_eq!(aorms_ui::chip(&app.aorms).tone, ChipTone::Dim);
}

#[test]
fn studio_without_connected_mode_gets_the_reason_in_words_and_drawing_still_works() {
    let script = Script::default().on("GET api/aqc/v1/config", 200, CFG).on("POST api/aqc/v1/auth/login", 200, LOGIN).on(
        "POST api/aqc/v1/session",
        403,
        r#"{"error":{"code":"not_connected","message":"no","reason":"EXPIRED"}}"#,
    );
    let mut app = app_with(&script);
    let ctx = egui::Context::default();
    type_credentials(&mut app, "a@x.in", "pw");
    aorms_ui::start_sign_in(&mut app, &ctx);
    settle(&mut app, &ctx);
    let m = app.aorms.message.clone().unwrap();
    assert_eq!(m.tone, Tone::Warn);
    assert!(m.text.contains("expired") && m.text.contains("keeps working"), "{}", m.text);
    assert!(app.run("circle", json!({ "center": [0, 0], "radius": 1 })).is_ok());
}

#[test]
fn explain_covers_the_errors_a_person_can_hit() {
    let api = |status, code: &str, reason: Option<&str>, retry| AormsError::Api {
        status,
        code: code.into(),
        message: "raw".into(),
        reason: reason.map(String::from),
        retry_after: retry,
    };
    assert!(aorms_ui::explain(&api(403, "not_linked", None, None)).contains("on the web once"));
    assert!(aorms_ui::explain(&api(403, "pending", None, None)).contains("approved"));
    assert!(aorms_ui::explain(&api(429, "rate_limited", None, Some(42))).contains("42"));
    assert!(aorms_ui::explain(&api(403, "not_connected", Some("PLAN_NOT_CONNECTED"), None)).contains("doesn't include"));
    assert!(aorms_ui::explain(&api(403, "not_connected", Some("NO_STUDIO"), None)).contains("no studio"));
    assert!(aorms_ui::explain(&api(503, "unavailable", None, None)).contains("isn't available"));
    assert_eq!(aorms_ui::explain(&api(418, "teapot", None, None)), "raw");
    assert!(aorms_ui::explain(&AormsError::Network("down".into())).contains("internet"));
    assert!(aorms_ui::explain(&AormsError::SignedOut).contains("Sign in again"));
}

fn connected(script: &Script) -> (CadApp, egui::Context) {
    let mut app = app_with(script);
    let ctx = egui::Context::default();
    type_credentials(&mut app, "a@x.in", "pw");
    aorms_ui::start_sign_in(&mut app, &ctx);
    settle(&mut app, &ctx);
    assert!(app.aorms.view.connected);
    (app, ctx)
}

#[test]
fn pulling_a_project_updates_details_and_floors_links_the_drawing_and_changes_no_geometry() {
    let script = server();
    let (mut app, ctx) = connected(&script);
    app.run("line", json!({ "points": [[0, 0], [10, 0]] })).unwrap();
    let before = app.session.doc().unwrap().entity_count();
    aorms_ui::start_pull(&mut app, &ctx, PID);
    assert!(app.aorms.busy());
    settle(&mut app, &ctx);
    let details = app.run("project", json!({})).unwrap();
    assert_eq!((details["details"]["title"].as_str(), details["details"]["owner"].as_str()), (Some("Lakeview Clubhouse"), Some("Lakeview RA")));
    let floors = app.run("floors", json!({})).unwrap();
    assert_eq!((floors["floors"][0]["name"].as_str(), floors["floors"][0]["beam"].as_f64()), (Some("First floor"), Some(400.0)));
    assert_eq!(app.session.doc().unwrap().entity_count(), before, "geometry untouched");
    assert_eq!(aorms_ui::linked(&app), Some((PID.to_string(), "DEMO-PRJ-04".to_string())));
    let m = app.aorms.message.clone().unwrap();
    assert!(m.text.contains("Lakeview Clubhouse"), "{}", m.text);
}

#[test]
fn a_pull_is_one_undo_step() {
    let script = server();
    let (mut app, ctx) = connected(&script);
    aorms_ui::start_pull(&mut app, &ctx, PID);
    settle(&mut app, &ctx);
    assert!(aorms_ui::linked(&app).is_some());
    app.run("undo", json!({})).unwrap();
    assert!(aorms_ui::linked(&app).is_none(), "one undo reverts the whole pull");
    assert!(app.run("project", json!({})).unwrap()["details"].get("title").is_none());
}

#[test]
fn hostile_project_ids_are_refused_before_any_request() {
    let script = server();
    let (mut app, ctx) = connected(&script);
    let before = script.calls("seed");
    for id in ["../../x", "a b", "", &"z".repeat(90), "id?x=1"] {
        aorms_ui::start_pull(&mut app, &ctx, id);
        assert!(!app.aorms.busy(), "{id}");
    }
    assert_eq!(script.calls("seed"), before);
}

#[test]
fn menu_and_typed_commands_open_the_dialog_but_json_calls_run_the_command() {
    let script = server();
    let mut app = app_with(&script);
    for id in ["aorms.signin", "aorms.status", "aorms.projects", "aorms.open", "aorms.drawings"] {
        app.ui.dialog = None;
        app.start(id);
        assert_eq!(app.ui.dialog.as_deref(), Some("aorms"), "{id}");
    }
    app.ui.dialog = None;
    app.cmdline("aorms.signin");
    assert_eq!(app.ui.dialog.as_deref(), Some("aorms"), "typed");
    app.ui.dialog = None;
    let st = app.run("aorms.status", json!({})).unwrap();
    assert!(app.ui.dialog.is_none(), "JSON calls never open a dialog");
    assert_eq!(st["signedIn"], false);
    assert!(app.run("aorms.signin", json!({ "email": "a@x.in", "password": "pw" })).is_ok(), "the engine command still signs in");
    assert!(app.ui.dialog.is_none());
}

#[test]
fn the_aorms_menu_exists_and_lists_the_commands() {
    let app = CadApp::new(Session::new(), Services::default());
    let tree = cadcraft_ui_egui::menus::tree(&app);
    let (_, entries) = tree.iter().find(|(m, _)| m == "AORMS").expect("an AORMS menu");
    assert!(!entries.is_empty());
}

#[test]
fn the_dialog_draws_in_every_state_without_panicking() {
    let script = server();
    let ctx = egui::Context::default();
    let draw = |app: &mut CadApp| {
        app.ui.dialog = Some("aorms".into());
        let mut out = ctx.run_ui(Default::default(), |ui| dialogs::show(app, ui.ctx()));
        out.textures_delta.clear();
    };
    // signed out
    let mut app = app_with(&script);
    draw(&mut app);
    assert_eq!(app.ui.dialog.as_deref(), Some("aorms"));
    // busy
    type_credentials(&mut app, "a@x.in", "pw");
    aorms_ui::start_sign_in(&mut app, &ctx);
    draw(&mut app);
    settle(&mut app, &ctx);
    // connected (project list loads on first show)
    draw(&mut app);
    draw(&mut app);
    // signed in without a session
    app.aorms.needs_session = true;
    app.aorms.view.connected = false;
    draw(&mut app);
    // an error message
    app.aorms.message = Some(aorms_ui::Message { tone: Tone::Error, text: "boom".into() });
    draw(&mut app);
}

#[test]
fn chip_states() {
    let mut a = aorms_ui::AormsUi::default();
    let c = aorms_ui::chip(&a);
    assert_eq!((c.text.as_str(), c.tone), ("AORMS", ChipTone::Dim));
    assert!(c.tip.contains("Community"));
    a.view = aorms_ui::View {
        signed_in: true,
        connected: true,
        studio: "Aurelia Design Collective Pvt Ltd".into(),
        email: Some("a@x.in".into()),
        ..Default::default()
    };
    let c = aorms_ui::chip(&a);
    assert_eq!(c.tone, ChipTone::Accent);
    assert!(c.text.starts_with("AORMS · Aurelia") && c.text.chars().count() <= 30, "{}", c.text);
    a.view.connected = false;
    assert_eq!(aorms_ui::chip(&a).tone, ChipTone::Warn);
}

#[test]
fn prefs_keep_the_email_and_server_but_never_a_password_or_token() {
    let script = server();
    let (mut app, _ctx) = connected(&script);
    app.aorms.base_url = "http://localhost:3128".into();
    app.session.aorms.client = None;
    let saved = app.prefs_json();
    assert!(saved.contains("aormsEmail") && saved.contains("localhost:3128"));
    for forbidden in ["password", "pw", "refresh", "token", "r1", "a1"] {
        assert!(!saved.to_lowercase().contains(&format!("\"{forbidden}")), "{forbidden}");
    }
    let mut b = CadApp::new(Session::new(), Services::default());
    b.load_prefs(&saved);
    assert_eq!((b.aorms.email.as_str(), b.aorms.base_url.as_str()), ("a@x.in", "http://localhost:3128"));
    // hostile saved values are ignored
    b.aorms.email.clear();
    b.aorms.base_url.clear();
    b.load_prefs(&json!({ "aormsEmail": "no-at-sign", "aormsBaseUrl": "http://evil.example" }).to_string());
    assert!(b.aorms.email.is_empty() && b.aorms.base_url.is_empty());
}

#[test]
fn sign_out_clears_the_session_list_and_view() {
    let script = server();
    let (mut app, ctx) = connected(&script);
    app.ui.dialog = Some("aorms".into());
    app.run("aorms.signout", json!({})).unwrap();
    aorms_ui::poll(&mut app, &ctx);
    assert!(!app.aorms.view.signed_in && !app.aorms.view.connected);
}

#[test]
fn the_client_can_move_between_threads() {
    fn assert_send<T: Send>() {}
    assert_send::<Client>();
}

#[test]
fn start_up_restore_with_nothing_stored_is_silent() {
    let script = server();
    let mut app = app_with(&script);
    app.aorms.auto_restore = true;
    let ctx = egui::Context::default();
    aorms_ui::poll(&mut app, &ctx);
    settle(&mut app, &ctx);
    assert!(app.aorms.message.is_none() && !app.aorms.view.signed_in);
    assert_eq!(script.calls("auth/login"), 0);
    // and only once
    aorms_ui::poll(&mut app, &ctx);
    assert!(!app.aorms.busy());
}
