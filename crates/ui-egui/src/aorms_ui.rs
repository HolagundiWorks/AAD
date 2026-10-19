//! The AORMS connection in the interface: the sign-in dialog, the status-bar chip and the background jobs behind them.
//!
//! Everything here is optional — signed out, O-Draft is a complete standalone drafting program. The network calls block (up to 60 s), so
//! they never run on the UI thread: a job takes the [`Client`] out of the session, runs on a worker thread and hands the client back
//! with its result; [`poll`] collects it each frame. The drawing is only ever touched on the UI thread, through the `aorms.apply`
//! command (one undo step), and only for project details and floors — never drawn geometry. The password lives in the dialog's text
//! field until the job starts, is cleared immediately after, and is never stored, logged or saved in the preferences.

use std::sync::mpsc::{self, Receiver, TryRecvError};

use aad_aorms::{AormsError, Client, OnlineProject, RestoreOutcome, SessionInfo};
use aad_engine::cmd::aorms as engine;
use egui::{Color32, Key, RichText};
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Info,
    Warn,
    Error,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Message {
    pub tone: Tone,
    pub text: String,
}

/// What the interface knows about the connection while the client is away on a worker thread.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct View {
    pub signed_in: bool,
    pub connected: bool,
    pub studio: String,
    pub role: String,
    pub plan: Option<String>,
    pub email: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    SignIn,
    Restore,
    Connect,
    Projects,
    Pull,
}

enum Outcome {
    Connected { projects: Option<Vec<OnlineProject>> },
    Restored { outcome: RestoreOutcome, projects: Option<Vec<OnlineProject>> },
    Projects(Vec<OnlineProject>),
    Fetched { id: String, seed: Value, settings: Value },
    Failed(AormsError),
}

struct Done {
    client: Client,
    outcome: Outcome,
}

pub struct Job {
    pub kind: Kind,
    rx: Receiver<Done>,
}

/// A request made where no `egui::Context` is at hand (menus, typed commands); run by [`poll`].
#[derive(Clone, PartialEq, Debug)]
enum Queued {
    Pull(String),
}

#[derive(Default)]
pub struct AormsUi {
    pub email: String,
    pub password: String,
    pub base_url: String,
    pub filter: String,
    pub selected: Option<String>,
    pub projects: Vec<OnlineProject>,
    pub message: Option<Message>,
    pub view: View,
    pub job: Option<Job>,
    /// Signed in on this computer (another AORMS app's login) but O-Draft has no session of its own yet, or it was replaced.
    pub needs_session: bool,
    /// The host turns this on to restore the shared sign-in once at start-up.
    pub auto_restore: bool,
    restore_started: bool,
    queued: Vec<Queued>,
    projects_loaded: bool,
}

impl AormsUi {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    pub fn job_kind(&self) -> Option<Kind> {
        self.job.as_ref().map(|j| j.kind)
    }
    /// The server this app talks to: the saved one, else `$AORMS_URL`, else aorms.in.
    pub fn effective_base(&self) -> String {
        let typed = self.base_url.trim();
        if !typed.is_empty() {
            return typed.trim_end_matches('/').to_string();
        }
        std::env::var("AORMS_URL")
            .ok()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| "https://aorms.in".into())
            .trim_end_matches('/')
            .to_string()
    }
    /// Saved preferences: the e-mail and server only. Never a password or token.
    pub fn prefs(&self) -> Value {
        let mut v = json!({});
        if !self.email.is_empty() {
            v["aormsEmail"] = json!(self.email);
        }
        if !self.base_url.trim().is_empty() {
            v["aormsBaseUrl"] = json!(self.base_url.trim());
        }
        v
    }
    pub fn load_prefs(&mut self, v: &Value) {
        if let Some(e) = v.get("aormsEmail").and_then(Value::as_str).map(str::trim)
            && e.len() <= 254
            && e.contains('@')
        {
            self.email = e.to_string();
        }
        if let Some(b) = v.get("aormsBaseUrl").and_then(Value::as_str).map(str::trim)
            && engine::valid_base(b)
        {
            self.base_url = b.to_string();
        }
    }
}

// ── status chip ──────────────────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChipTone {
    Dim,
    Accent,
    Warn,
}

pub struct Chip {
    pub text: String,
    pub tone: ChipTone,
    pub tip: String,
}

/// The status-bar chip: Community when signed out, the studio when connected, a prompt when signed in without a session.
pub fn chip(a: &AormsUi) -> Chip {
    if a.busy() {
        return Chip { text: "AORMS …".into(), tone: ChipTone::Dim, tip: crate::tl!("Talking to AORMS…").to_string() };
    }
    if a.view.connected {
        let studio: String = a.view.studio.chars().take(20).collect();
        let who = a.view.email.clone().unwrap_or_default();
        return Chip {
            text: if studio.is_empty() { "AORMS".into() } else { format!("AORMS · {studio}") },
            tone: ChipTone::Accent,
            tip: crate::tf!("Connected to {studio} as {email}. Click for projects.", studio = a.view.studio, email = who),
        };
    }
    if a.view.signed_in || a.needs_session {
        return Chip {
            text: "AORMS · !".into(),
            tone: ChipTone::Warn,
            tip: crate::tl!("Signed in to AORMS, but O-Draft isn't connected. Click to connect.").to_string(),
        };
    }
    Chip { text: "AORMS".into(), tone: ChipTone::Dim, tip: crate::tl!("Community mode: not signed in to AORMS. Click to sign in.").to_string() }
}

// ── errors, in words ─────────────────────────────────────────────────────────────────────────────────────────

pub fn explain(e: &AormsError) -> String {
    match e {
        AormsError::Network(_) => crate::tl!("Can't reach AORMS. Check your internet connection and try again.").to_string(),
        AormsError::Contract(_) => crate::tl!("This AORMS server needs a newer version of O-Draft.").to_string(),
        AormsError::SignedOut => crate::tl!("Your AORMS sign-in has ended. Sign in again.").to_string(),
        AormsError::TooLarge => crate::tl!("AORMS sent more data than O-Draft accepts.").to_string(),
        AormsError::Unavailable => crate::tl!("AORMS is not available in the web version of O-Draft.").to_string(),
        AormsError::BadResponse(_) => crate::tl!("AORMS answered in a way O-Draft doesn't understand.").to_string(),
        AormsError::Api { code, reason, retry_after, status, message } => match code.as_str() {
            "invalid_credentials" => crate::tl!("Invalid email or password.").to_string(),
            "not_linked" => crate::tl!("Sign in to AORMS on the web once with this account, then try again.").to_string(),
            "pending" => crate::tl!("Your studio hasn't approved your access yet.").to_string(),
            "rate_limited" => crate::tf!("Too many attempts. Try again in {seconds} seconds.", seconds = retry_after.unwrap_or(60)),
            "session_replaced" => crate::tl!("You signed in to O-Draft on another computer. Connect here to continue.").to_string(),
            "not_connected" => match reason.as_deref() {
                Some("EXPIRED") => crate::tl!("Your studio's AORMS plan has expired. O-Draft keeps working without it.").to_string(),
                Some("NO_STUDIO") => crate::tl!("This account has no studio in AORMS. O-Draft keeps working without it.").to_string(),
                _ => crate::tl!("Your studio's AORMS plan doesn't include connected mode. O-Draft keeps working without it.").to_string(),
            },
            "role_not_allowed" => crate::tl!("Portal accounts can't use O-Draft. Sign in with a staff account.").to_string(),
            _ if *status >= 500 => crate::tl!("AORMS isn't available right now. Try again shortly.").to_string(),
            _ => message.clone(),
        },
    }
}

// ── jobs ─────────────────────────────────────────────────────────────────────────────────────────────────────

type Work = Box<dyn FnOnce(&mut Client) -> Outcome + Send>;

fn spawn(app: &mut CadApp, ctx: &egui::Context, kind: Kind, work: Work) {
    if app.aorms.busy() {
        return;
    }
    let client = match app.session.aorms.client.take() {
        Some(c) => c,
        None => match engine::make_client(&app.aorms.effective_base()) {
            Ok(c) => c,
            Err(e) => {
                app.aorms.message = Some(Message { tone: Tone::Error, text: e.to_string() });
                return;
            }
        },
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (tx, rx) = mpsc::channel();
        let ctx2 = ctx.clone();
        let started = std::thread::Builder::new().name("aorms".into()).spawn(move || {
            let mut client = client;
            let outcome = work(&mut client);
            let _ = tx.send(Done { client, outcome });
            ctx2.request_repaint();
        });
        match started {
            Ok(_) => {
                app.aorms.job = Some(Job { kind, rx });
                app.aorms.message = None;
            }
            Err(e) => app.aorms.message = Some(Message { tone: Tone::Error, text: e.to_string() }),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (ctx, kind, work, client);
        app.aorms.message = Some(Message { tone: Tone::Error, text: explain(&AormsError::Unavailable) });
    }
}

fn projects_of(c: &mut Client) -> Option<Vec<OnlineProject>> {
    c.projects().ok()
}

pub fn start_sign_in(app: &mut CadApp, ctx: &egui::Context) {
    let email = app.aorms.email.trim().to_string();
    let password = std::mem::take(&mut app.aorms.password); // out of the text field before anything else happens
    let base = app.aorms.effective_base();
    if !engine::valid_base(&base) {
        app.aorms.message = Some(Message {
            tone: Tone::Error,
            text: crate::tl!("The server address must start with https:// (or http://localhost for development).").to_string(),
        });
        return;
    }
    // A fresh client for the chosen server, unless a (test or previous) one for it is already in the session.
    if app.session.aorms.client.as_ref().is_some_and(|c| c.base_url() != base) {
        app.session.aorms.client = None;
    }
    if app.session.aorms.client.is_none() {
        match engine::make_client(&base) {
            Ok(c) => app.session.aorms.client = Some(c),
            Err(e) => {
                app.aorms.message = Some(Message { tone: Tone::Error, text: e.to_string() });
                return;
            }
        }
    }
    let label = engine::client_label();
    spawn(
        app,
        ctx,
        Kind::SignIn,
        Box::new(move |c| match c.sign_in(&email, &password, &label) {
            Ok(_) => Outcome::Connected { projects: projects_of(c) },
            Err(e) => Outcome::Failed(e),
        }),
    );
}

pub fn start_restore(app: &mut CadApp, ctx: &egui::Context) {
    spawn(
        app,
        ctx,
        Kind::Restore,
        Box::new(|c| match c.restore() {
            Ok(outcome) => {
                let projects = if outcome == RestoreOutcome::Restored { projects_of(c) } else { None };
                Outcome::Restored { outcome, projects }
            }
            Err(e) => Outcome::Failed(e),
        }),
    );
}

/// "Connect here": starts this app's session (never silent — it replaces another computer's O-Draft session).
pub fn start_connect(app: &mut CadApp, ctx: &egui::Context) {
    let label = engine::client_label();
    spawn(
        app,
        ctx,
        Kind::Connect,
        Box::new(move |c| match c.start_session(&label) {
            Ok(_) => Outcome::Connected { projects: projects_of(c) },
            Err(e) => Outcome::Failed(e),
        }),
    );
}

pub fn start_projects(app: &mut CadApp, ctx: &egui::Context) {
    spawn(
        app,
        ctx,
        Kind::Projects,
        Box::new(|c| match c.projects() {
            Ok(list) => Outcome::Projects(list),
            Err(e) => Outcome::Failed(e),
        }),
    );
}

pub fn start_pull(app: &mut CadApp, ctx: &egui::Context, project_id: &str) {
    let Ok(id) = engine::id_param("aorms.open", project_id) else {
        app.aorms.message = Some(Message { tone: Tone::Error, text: crate::tl!("That isn't a project id.").to_string() });
        return;
    };
    spawn(
        app,
        ctx,
        Kind::Pull,
        Box::new(move |c| match engine::fetch_project(c, &id) {
            Ok((seed, settings)) => Outcome::Fetched { id, seed, settings },
            Err(e) => Outcome::Failed(e),
        }),
    );
}

fn refresh_view(app: &mut CadApp) {
    if app.aorms.busy() {
        return;
    }
    app.aorms.view = match app.session.aorms.client.as_ref() {
        Some(c) => {
            let s = c.session();
            View {
                signed_in: c.is_signed_in(),
                connected: c.is_connected(),
                studio: s.map(|s: &SessionInfo| s.studio.name.clone()).unwrap_or_default(),
                role: s.map(|s| s.role.clone()).unwrap_or_default(),
                plan: s.and_then(|s| s.plan.clone()),
                email: c.email().map(str::to_string),
            }
        }
        None => View::default(),
    };
}

fn say(app: &mut CadApp, tone: Tone, text: String) {
    app.aorms.message = Some(Message { tone, text });
}

fn finish(app: &mut CadApp, kind: Kind, outcome: Outcome) {
    match outcome {
        Outcome::Connected { projects } => {
            app.aorms.needs_session = false;
            app.aorms.password.clear();
            if let Some(p) = projects {
                app.aorms.projects = p;
                app.aorms.projects_loaded = true;
            }
            say(app, Tone::Info, crate::tl!("Connected to AORMS.").to_string());
        }
        Outcome::Restored { outcome, projects } => match outcome {
            RestoreOutcome::Restored => {
                app.aorms.needs_session = false;
                if let Some(p) = projects {
                    app.aorms.projects = p;
                    app.aorms.projects_loaded = true;
                }
            }
            RestoreOutcome::NeedsSession => app.aorms.needs_session = true,
            RestoreOutcome::SessionReplaced => {
                app.aorms.needs_session = true;
                say(
                    app,
                    Tone::Warn,
                    explain(&AormsError::Api {
                        status: 409,
                        code: "session_replaced".into(),
                        message: String::new(),
                        reason: None,
                        retry_after: None,
                    }),
                );
            }
            RestoreOutcome::NotConnected => say(
                app,
                Tone::Warn,
                crate::tl!("Your studio's AORMS plan doesn't include connected mode. O-Draft keeps working without it.").to_string(),
            ),
            RestoreOutcome::NoStoredSession | RestoreOutcome::SignedOut => {}
        },
        Outcome::Projects(list) => {
            app.aorms.projects = list;
            app.aorms.projects_loaded = true;
        }
        Outcome::Fetched { id, seed, settings } => match app.run("aorms.apply", json!({ "projectOfficeId": id, "seed": seed, "settings": settings }))
        {
            Ok(v) => {
                let title = v["details"]["title"].as_str().unwrap_or("").to_string();
                let warnings: Vec<String> =
                    v["warnings"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
                let head = crate::tf!("Project info pulled into this drawing: {title}", title = title);
                if warnings.is_empty() {
                    say(app, Tone::Info, head);
                } else {
                    say(app, Tone::Warn, format!("{head} — {}", warnings.join("; ")));
                }
            }
            Err(e) => say(app, Tone::Error, e),
        },
        Outcome::Failed(e) => {
            if e.is_session_replaced() {
                app.aorms.needs_session = true;
            }
            // A password typed for a failed sign-in is already gone; the form keeps the e-mail.
            let _ = kind;
            say(app, if e.is_not_connected() { Tone::Warn } else { Tone::Error }, explain(&e));
        }
    }
}

/// Once per frame: start the one-time restore, run queued requests, collect a finished job and keep the cached view current.
pub fn poll(app: &mut CadApp, ctx: &egui::Context) {
    if app.aorms.auto_restore && !app.aorms.restore_started {
        app.aorms.restore_started = true;
        start_restore(app, ctx);
    }
    if !app.aorms.busy() && !app.aorms.queued.is_empty() {
        let queued = std::mem::take(&mut app.aorms.queued);
        for q in queued {
            match q {
                Queued::Pull(id) => start_pull(app, ctx, &id),
            }
        }
    }
    let received = match app.aorms.job.as_ref() {
        Some(j) => match j.rx.try_recv() {
            Ok(d) => Some(Some(d)),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(None),
        },
        None => None,
    };
    if let Some(done) = received {
        let kind = app.aorms.job.take().map(|j| j.kind).unwrap_or(Kind::Restore);
        match done {
            Some(d) => {
                app.session.aorms.client = Some(d.client);
                finish(app, kind, d.outcome);
            }
            // The worker died without answering: the client is gone with it. Say so; the user can sign in again.
            None => say(app, Tone::Error, crate::tl!("The AORMS request stopped unexpectedly. Sign in again.").to_string()),
        }
    }
    refresh_view(app);
}

// ── routing: menu / typed commands without parameters open the dialog ───────────────────────────────────────

pub fn route(app: &mut CadApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if !params.is_null() {
        return None; // JSON callers run the engine command itself
    }
    match id {
        "aorms.signin" | "aorms.status" | "aorms.projects" | "aorms.open" | "aorms.drawings" => {}
        "aorms.pull" => {
            if let Some(link) = linked(app)
                && app.aorms.view.connected
            {
                app.aorms.queued.push(Queued::Pull(link.0));
            }
        }
        _ => return None,
    }
    app.ui.dialog = Some("aorms".into());
    Some(Ok(Value::Null))
}

/// The AORMS project this drawing is linked to: (project id, reference).
pub fn linked(app: &CadApp) -> Option<(String, String)> {
    let text = app.session.doc().ok()?.project.get("aorms")?.clone();
    let v: Value = serde_json::from_str(&text).ok()?;
    let id = v.get("projectOfficeId")?.as_str()?.to_string();
    let reference = v.get("ref").and_then(Value::as_str).unwrap_or("").to_string();
    Some((id, reference))
}

// ── the dialog ───────────────────────────────────────────────────────────────────────────────────────────────

enum Act {
    SignIn,
    Connect,
    SignOut,
    Reload,
    Pull(String),
}

fn tone_color(t: &Tokens, tone: Tone) -> Color32 {
    match tone {
        Tone::Info => t.text_dim,
        Tone::Warn => t.warn,
        Tone::Error => Color32::from_rgb(0xDA, 0x1E, 0x28),
    }
}

pub fn dialog(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    let t = Tokens::get();
    let busy = app.aorms.busy();
    let kind = app.aorms.job_kind();
    let view = app.aorms.view.clone();
    let link = linked(app);
    let needs_session = app.aorms.needs_session;
    let mut act: Option<Act> = None;
    // The project list is loaded when the dialog is first shown signed in.
    if view.connected && !app.aorms.projects_loaded && !busy {
        act = Some(Act::Reload);
    }

    egui::Window::new(crate::tl!("AORMS"))
        .id(egui::Id::new("AORMS connection"))
        .open(open)
        .resizable(false)
        .collapsible(false)
        .default_width(400.0)
        .show(ctx, |ui| {
            let a = &mut app.aorms;
            if busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(match kind {
                        Some(Kind::SignIn) => crate::tl!("Signing in…"),
                        Some(Kind::Pull) => crate::tl!("Pulling project info…"),
                        Some(Kind::Connect) => crate::tl!("Connecting…"),
                        _ => crate::tl!("Talking to AORMS…"),
                    });
                });
                ui.add_space(4.0);
            }
            if view.connected {
                ui.heading(&view.studio);
                let who = [view.email.clone(), Some(view.role.clone()).filter(|r| !r.is_empty()), view.plan.clone()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · ");
                ui.label(RichText::new(who).color(t.text_dim));
                ui.add_space(6.0);
                if let Some((_, reference)) = &link {
                    ui.horizontal(|ui| {
                        ui.label(crate::tf!("This drawing is linked to {project}.", project = reference));
                        if ui.add_enabled(!busy, egui::Button::new(crate::tl!("Refresh"))).clicked()
                            && let Some((id, _)) = &link
                        {
                            act = Some(Act::Pull(id.clone()));
                        }
                    });
                    ui.add_space(4.0);
                }
                ui.separator();
                ui.label(RichText::new(crate::tl!("Project")).strong());
                ui.add(egui::TextEdit::singleline(&mut a.filter).hint_text(crate::tl!("Search projects")).desired_width(f32::INFINITY));
                let needle = a.filter.trim().to_lowercase();
                egui::ScrollArea::vertical().max_height(200.0).auto_shrink([false, true]).show(ui, |ui| {
                    let mut shown = 0;
                    for p in &a.projects {
                        let label = format!("{}  {}", p.reference, p.title);
                        if !needle.is_empty() && !label.to_lowercase().contains(&needle) {
                            continue;
                        }
                        shown += 1;
                        let selected = a.selected.as_deref() == Some(p.project_office_id.as_str());
                        if ui.selectable_label(selected, label).clicked() {
                            a.selected = Some(p.project_office_id.clone());
                        }
                    }
                    if shown == 0 {
                        ui.label(
                            RichText::new(if a.projects.is_empty() { crate::tl!("No projects yet.") } else { crate::tl!("No project matches.") })
                                .color(t.text_faint),
                        );
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(crate::tl!(
                        "Pulling copies the project's title block details and floors into this drawing. Your drawn geometry is never changed."
                    ))
                    .small()
                    .color(t.text_faint),
                );
                ui.horizontal(|ui| {
                    let can = !busy && a.selected.is_some();
                    if ui.add_enabled(can, egui::Button::new(crate::tl!("Pull project info"))).clicked()
                        && let Some(id) = a.selected.clone()
                    {
                        act = Some(Act::Pull(id));
                    }
                    if ui.add_enabled(!busy, egui::Button::new(crate::tl!("Reload list"))).clicked() {
                        act = Some(Act::Reload);
                    }
                    if ui.add_enabled(!busy, egui::Button::new(crate::tl!("Sign out"))).clicked() {
                        act = Some(Act::SignOut);
                    }
                });
            } else if view.signed_in || needs_session {
                ui.label(crate::tl!("You're signed in to AORMS on this computer, but O-Draft isn't connected yet."));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new(crate::tl!("Connect O-Draft"))).clicked() {
                        act = Some(Act::Connect);
                    }
                    if ui.add_enabled(!busy, egui::Button::new(crate::tl!("Sign out"))).clicked() {
                        act = Some(Act::SignOut);
                    }
                });
            } else {
                ui.label(crate::tl!(
                    "Sign in with your AORMS account to pull project information into your drawings. O-Draft keeps working without it."
                ));
                ui.add_space(6.0);
                let enter = egui::Grid::new("aorms_signin_grid")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(crate::tl!("Email"));
                        ui.add_enabled(!busy, egui::TextEdit::singleline(&mut a.email).desired_width(260.0));
                        ui.end_row();
                        ui.label(crate::tl!("Password"));
                        let pw = ui.add_enabled(!busy, egui::TextEdit::singleline(&mut a.password).password(true).desired_width(260.0));
                        ui.end_row();
                        pw.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
                    })
                    .inner;
                let default_base = a.effective_base();
                ui.collapsing(crate::tl!("Server"), |ui| {
                    ui.add_enabled(!busy, egui::TextEdit::singleline(&mut a.base_url).hint_text(default_base).desired_width(f32::INFINITY));
                    if !a.base_url.trim().is_empty() && !engine::valid_base(a.base_url.trim()) {
                        ui.label(
                            RichText::new(crate::tl!("The address must start with https:// (or http://localhost for development)."))
                                .small()
                                .color(tone_color(&t, Tone::Error)),
                        );
                    }
                });
                ui.add_space(6.0);
                let ready = !busy && a.email.contains('@') && !a.password.is_empty();
                if ui.add_enabled(ready, egui::Button::new(crate::tl!("Sign in"))).clicked() || (enter && ready) {
                    act = Some(Act::SignIn);
                }
            }
            if let Some(m) = &a.message {
                ui.add_space(6.0);
                ui.label(RichText::new(&m.text).color(tone_color(&t, m.tone)));
            }
        });

    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        *open = false;
    }
    match act {
        Some(Act::SignIn) => start_sign_in(app, ctx),
        Some(Act::Connect) => start_connect(app, ctx),
        Some(Act::Reload) => start_projects(app, ctx),
        Some(Act::Pull(id)) => start_pull(app, ctx, &id),
        Some(Act::SignOut) => {
            let _ = app.run("aorms.signout", json!({}));
            app.aorms.projects.clear();
            app.aorms.projects_loaded = false;
            app.aorms.selected = None;
            app.aorms.needs_session = false;
            app.aorms.message = Some(Message { tone: Tone::Info, text: crate::tl!("Signed out of AORMS.").to_string() });
            refresh_view(app);
        }
        None => {}
    }
    // Keep the spinner moving while a job runs.
    if app.aorms.busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}
