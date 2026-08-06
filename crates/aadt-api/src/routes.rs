//! Request → response, as a pure function over a [`Session`].
//!
//! Every route in this module is reachable without a socket: [`handle`] takes a
//! parsed [`Request`] and a session and returns a [`Response`]. The transport
//! (accept, timeouts, loopback checks) lives in the crate root, so the routing,
//! the authorisation gate, and the read-only gate are all unit-testable without
//! binding a port.
//!
//! **Reads project the session; the one write is a command line.** There is no
//! `POST /v1/entities`. Geometry changes only through `POST /v1/command`, which
//! feeds the same interpreter as the command line and the GUI — so an API
//! caller gets the same validation, the same undo record, and the same history
//! entry, and no second path into the document exists to keep in step.

use std::collections::BTreeMap;

use aadt_app::{Feedback, Session};
use aadt_geometry::{Bbox, Point2};
use aadt_model::{Document, Entity, Geometry};

use crate::http::{Request, Response};
use crate::json;
use crate::ApiConfig;

/// The API version prefix every route carries.
pub const API_VERSION: &str = "v1";

/// A handled request, plus whether the server was asked to stop.
#[derive(Debug, Clone)]
pub struct Handled {
    pub response: Response,
    pub shutdown: bool,
}

impl Handled {
    fn ok(response: Response) -> Self {
        Self {
            response,
            shutdown: false,
        }
    }
}

/// Route one request against a session.
pub fn handle(req: &Request, session: &mut Session, cfg: &ApiConfig) -> Handled {
    if let Some(denied) = authorize(req, cfg) {
        return Handled::ok(denied);
    }
    if let Some(denied) = authorize_route(req, cfg) {
        return Handled::ok(denied);
    }
    let path = req.path.trim_end_matches('/');
    let path = if path.is_empty() { "/" } else { path };

    match (req.method.as_str(), path) {
        ("GET", "/") | ("GET", "/v1") => Handled::ok(index()),
        ("GET", "/health") | ("GET", "/v1/health") => Handled::ok(health(session, cfg)),
        ("GET", "/v1/document") => Handled::ok(document(session)),
        ("GET", "/v1/entities") => Handled::ok(entities(req, session)),
        ("GET", "/v1/selection") => Handled::ok(selection(session)),
        ("GET", "/v1/layers") => Handled::ok(layers(session)),
        ("GET", "/v1/history") => Handled::ok(history(session)),
        ("GET", "/v1/transcript") => Handled::ok(transcript(session)),
        ("GET", "/v1/commands") => Handled::ok(commands(req, session)),
        ("POST", "/v1/command") => Handled::ok(command(req, session, cfg)),
        ("POST", "/v1/shutdown") => Handled {
            response: Response::json(200, json::object(&[("status", json::string("stopping"))])),
            shutdown: true,
        },
        (_, p) if p.starts_with("/v1/entities/") => Handled::ok(entity_by_id(req, session, p)),
        // A known path reached with the wrong verb is a different mistake from
        // a path that does not exist; saying so saves a caller a guess.
        (_, p) if is_known_path(p) => Handled::ok(Response::error(
            405,
            &format!("{} is not allowed on {p}", req.method),
        )),
        (_, p) => Handled::ok(Response::error(
            404,
            &format!("no such route {p} (GET /{API_VERSION} lists them)"),
        )),
    }
}

fn is_known_path(path: &str) -> bool {
    matches!(
        path,
        "/" | "/health"
            | "/v1"
            | "/v1/health"
            | "/v1/document"
            | "/v1/entities"
            | "/v1/selection"
            | "/v1/layers"
            | "/v1/history"
            | "/v1/transcript"
            | "/v1/commands"
            | "/v1/command"
            | "/v1/shutdown"
    )
}

/// Bearer-token gate. Applies to **every** route, health included: an
/// unauthenticated liveness probe would be one endpoint whose behaviour
/// depends on nobody having wired it into something else later.
fn authorize(req: &Request, cfg: &ApiConfig) -> Option<Response> {
    let presented = req
        .header("authorization")
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| req.header("x-aadt-token"))
        .unwrap_or("")
        .trim();
    if constant_time_eq(presented, &cfg.token) {
        None
    } else {
        Some(
            Response::error(401, "missing or invalid API token")
                .with_header("WWW-Authenticate", "Bearer realm=\"aadt\""),
        )
    }
}

/// After the token gate: does this route need a capability the principal lacks?
fn authorize_route(req: &Request, cfg: &ApiConfig) -> Option<Response> {
    use aadt_object::Capability;
    let path = req.path.trim_end_matches('/');
    let path = if path.is_empty() { "/" } else { path };
    let need = match (req.method.as_str(), path) {
        ("POST", "/v1/command") => Capability::Command,
        ("POST", "/v1/shutdown") => Capability::Admin,
        _ => Capability::Read,
    };
    if cfg.can(need) {
        None
    } else {
        Some(Response::error(
            403,
            &format!(
                "principal {:?} lacks {:?} (role caps: {})",
                cfg.principal,
                format!("{need:?}").to_ascii_lowercase(),
                cfg.caps.names().join(",")
            ),
        ))
    }
}

/// Compare without an early exit on the first differing byte.
///
/// The token is short-lived and loopback-only, so this is defence in depth
/// rather than the thing standing between an attacker and the drawing — but a
/// timing-leaky comparison in an auth path is a bad habit to leave in the tree.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = (a.len() ^ b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= x ^ y;
    }
    diff == 0
}

fn index() -> Response {
    let routes = [
        "GET /v1/health",
        "GET /v1/document",
        "GET /v1/entities?limit&offset&layer",
        "GET /v1/entities/{id}",
        "GET /v1/layers",
        "GET /v1/selection",
        "GET /v1/history",
        "GET /v1/transcript",
        "GET /v1/commands?prefix",
        "POST /v1/command {\"line\": \"...\"}",
        "POST /v1/shutdown",
    ];
    Response::json(
        200,
        json::object(&[
            ("api", json::string("aadt")),
            ("version", json::string(env!("CARGO_PKG_VERSION"))),
            ("routes", json::string_array(routes)),
        ]),
    )
}

fn health(session: &Session, cfg: &ApiConfig) -> Response {
    Response::json(
        200,
        json::object(&[
            ("status", json::string("ok")),
            ("version", json::string(env!("CARGO_PKG_VERSION"))),
            ("read_only", json::bool_(cfg.read_only)),
            ("principal", json::string(&cfg.principal)),
            ("capabilities", json::string_array(cfg.caps.names())),
            ("entities", json::int(session.document().len() as u64)),
            ("revision", json::int(session.document().revision())),
        ]),
    )
}

fn document(session: &Session) -> Response {
    let doc = session.document();
    let props = doc.properties();
    Response::json(
        200,
        json::object(&[
            ("entities", json::int(doc.len() as u64)),
            ("revision", json::int(doc.revision())),
            ("layers", json::int(doc.layers().count() as u64)),
            ("extents", extents_json(doc)),
            (
                "properties",
                json::object(&[
                    ("title", json::string(&props.title)),
                    ("project", json::string(&props.project)),
                    ("client", json::string(&props.client)),
                    ("architect", json::string(&props.architect)),
                    ("scale", json::string(&props.scale)),
                    ("sheet_number", json::string(&props.sheet_number)),
                ]),
            ),
        ]),
    )
}

/// Drawing extents, or `null` for an empty drawing — the honest answer, and
/// one a client can branch on without inspecting four zeroes.
fn extents_json(doc: &Document) -> String {
    match Bbox::of_points(doc.points()) {
        None => "null".to_string(),
        Some(b) => json::object(&[
            ("min", point_json(b.min)),
            ("max", point_json(b.max)),
            ("width", json::number(b.width())),
            ("height", json::number(b.height())),
        ]),
    }
}

fn point_json(p: Point2) -> String {
    json::object(&[("x", json::number(p.x)), ("y", json::number(p.y))])
}

/// Default page size for `/v1/entities`.
const DEFAULT_LIMIT: usize = 200;
/// Largest page a caller may request.
const MAX_LIMIT: usize = 5_000;

fn entities(req: &Request, session: &Session) -> Response {
    let limit = match req.query_usize("limit") {
        Ok(v) => v.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT),
        Err(e) => return Response::error(400, &e),
    };
    let offset = match req.query_usize("offset") {
        Ok(v) => v.unwrap_or(0),
        Err(e) => return Response::error(400, &e),
    };
    let layer = req.query_param("layer");

    let doc = session.document();
    // `entities()` is already ascending by id, so paging is stable across
    // calls as long as the revision has not changed — which the response
    // reports so a caller can tell when its pages straddle an edit.
    let all = doc.entities();
    let filtered: Vec<&Entity> = match &layer {
        Some(name) => all
            .into_iter()
            .filter(|e| e.layer.eq_ignore_ascii_case(name))
            .collect(),
        None => all,
    };
    let total = filtered.len();
    let page: Vec<String> = filtered
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(entity_json)
        .collect();

    Response::json(
        200,
        json::object(&[
            ("total", json::int(total as u64)),
            ("offset", json::int(offset as u64)),
            ("limit", json::int(limit as u64)),
            ("count", json::int(page.len() as u64)),
            ("revision", json::int(doc.revision())),
            ("entities", json::array(page)),
        ]),
    )
}

fn entity_by_id(req: &Request, session: &Session, path: &str) -> Response {
    if req.method != "GET" {
        return Response::error(405, &format!("{} is not allowed on {path}", req.method));
    }
    let raw = path.trim_start_matches("/v1/entities/");
    let Ok(id) = raw.parse::<u64>() else {
        return Response::error(400, &format!("entity id must be an integer, got {raw:?}"));
    };
    match session
        .document()
        .entities()
        .into_iter()
        .find(|e| e.id.0 == id)
    {
        Some(e) => Response::json(200, entity_json(e)),
        None => Response::error(404, &format!("no entity {id} in this drawing")),
    }
}

fn entity_json(e: &Entity) -> String {
    json::object(&[
        ("id", json::int(e.id.0)),
        ("layer", json::string(&e.layer)),
        ("space", json::int(e.space.0)),
        ("kind", json::string(kind_of(&e.geometry))),
        ("geometry", geometry_json(&e.geometry)),
        ("attributes", attributes_json(e)),
    ])
}

fn kind_of(g: &Geometry) -> &'static str {
    match g {
        Geometry::Line(_) => "line",
        Geometry::Polyline(_) => "polyline",
        Geometry::Circle(_) => "circle",
        Geometry::Arc(_) => "arc",
        Geometry::Text(_) => "text",
        Geometry::Insert(_) => "insert",
        Geometry::Image(_) => "image",
    }
}

fn geometry_json(g: &Geometry) -> String {
    match g {
        Geometry::Line(s) => json::object(&[("a", point_json(s.a)), ("b", point_json(s.b))]),
        Geometry::Polyline(p) => json::object(&[
            (
                "vertices",
                json::array(p.vertices.iter().copied().map(point_json)),
            ),
            ("closed", json::bool_(p.closed)),
        ]),
        Geometry::Circle(c) => json::object(&[
            ("center", point_json(c.center)),
            ("radius", json::number(c.radius)),
        ]),
        Geometry::Arc(a) => json::object(&[
            ("center", point_json(a.center)),
            ("radius", json::number(a.radius)),
            ("start_angle", json::number(a.start_angle)),
            ("end_angle", json::number(a.end_angle)),
        ]),
        Geometry::Text(t) => json::object(&[
            ("value", json::string(&t.value)),
            ("position", point_json(t.position)),
            ("height", json::number(t.height)),
            ("rotation", json::number(t.rotation)),
        ]),
        Geometry::Insert(i) => json::object(&[
            ("name", json::string(&i.name)),
            ("position", point_json(i.position)),
            ("rotation", json::number(i.rotation)),
            ("scale_x", json::number(i.scale_x)),
            ("scale_y", json::number(i.scale_y)),
        ]),
        Geometry::Image(img) => json::object(&[
            ("path", json::string(&img.path)),
            ("position", point_json(img.position)),
            ("rotation", json::number(img.rotation)),
            ("width", json::number(img.width)),
            ("height", json::number(img.height)),
            ("opacity", json::number(img.opacity as f64)),
        ]),
    }
}

/// Attributes sorted by key: a `HashMap` iterates in an arbitrary order, and
/// an API whose response bytes change between identical calls cannot be
/// diffed, cached, or golden-tested.
fn attributes_json(e: &Entity) -> String {
    let sorted: BTreeMap<&str, &str> = e
        .attributes
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let fields: Vec<(&str, String)> = sorted
        .into_iter()
        .map(|(k, v)| (k, json::string(v)))
        .collect();
    json::object(&fields)
}

fn layers(session: &Session) -> Response {
    let mut names: Vec<_> = session.document().layers().collect();
    names.sort_by(|a, b| a.name.cmp(&b.name));
    let items = names.into_iter().map(|l| {
        json::object(&[
            ("name", json::string(&l.name)),
            ("color", json::int(u64::from(l.color))),
            ("lineweight_mm", json::number(f64::from(l.lineweight_mm))),
            ("visible", json::bool_(l.visible)),
            ("frozen", json::bool_(l.frozen)),
            ("locked", json::bool_(l.locked)),
        ])
    });
    let items: Vec<String> = items.collect();
    Response::json(
        200,
        json::object(&[
            ("count", json::int(items.len() as u64)),
            ("layers", json::array(items)),
        ]),
    )
}

fn selection(session: &Session) -> Response {
    let ids: Vec<String> = session
        .selection()
        .iter()
        .map(|id| json::int(id.0))
        .collect();
    Response::json(
        200,
        json::object(&[
            ("count", json::int(ids.len() as u64)),
            ("ids", json::array(ids)),
        ]),
    )
}

fn history(session: &Session) -> Response {
    let lines = session.history();
    Response::json(
        200,
        json::object(&[
            ("count", json::int(lines.len() as u64)),
            (
                "lines",
                json::string_array(lines.iter().map(String::as_str)),
            ),
        ]),
    )
}

fn transcript(session: &Session) -> Response {
    let entries: Vec<String> = session
        .transcript()
        .iter()
        .map(|fb| {
            json::object(&[
                ("status", json::string(feedback_status(fb))),
                (
                    "message",
                    match fb.message() {
                        Some(m) => json::string(m),
                        None => "null".to_string(),
                    },
                ),
            ])
        })
        .collect();
    Response::json(
        200,
        json::object(&[
            ("count", json::int(entries.len() as u64)),
            ("entries", json::array(entries)),
        ]),
    )
}

fn feedback_status(fb: &Feedback) -> &'static str {
    match fb {
        Feedback::Ok(_) => "ok",
        Feedback::Silent => "silent",
        Feedback::Err(_) => "error",
    }
}

fn commands(req: &Request, session: &Session) -> Response {
    let prefix = req.query_param("prefix").unwrap_or_default();
    let names = session.complete(&prefix);
    Response::json(
        200,
        json::object(&[
            ("count", json::int(names.len() as u64)),
            (
                "commands",
                json::string_array(names.iter().map(String::as_str)),
            ),
        ]),
    )
}

fn command(req: &Request, session: &mut Session, cfg: &ApiConfig) -> Response {
    if cfg.read_only {
        return Response::error(
            403,
            "this API is running read-only; restart without --read-only to run commands",
        );
    }
    let body = match json::parse_object(&req.body) {
        Ok(b) => b,
        Err(e) => return Response::error(400, &format!("malformed JSON body: {e}")),
    };
    let Some(line) = body.str_field("line") else {
        return Response::error(
            400,
            "body must be a JSON object with a string \"line\" field",
        );
    };
    // A single line, like the command prompt. Accepting an embedded newline
    // would run two commands behind one request, and the response has one
    // status to report them with.
    if line.contains('\n') || line.contains('\r') {
        return Response::error(
            400,
            "\"line\" must be a single command line; send one request per command",
        );
    }

    // M14.2: fine-grained capability on the command verb (OBJECTS RUN, CONNECTOR, …).
    let need = aadt_object::command_capability(line);
    if !cfg.can(need) {
        return Response::error(
            403,
            &format!(
                "principal {:?} cannot run {line:?} (needs {:?})",
                cfg.principal, need
            ),
        );
    }

    let before = session.document().revision();
    let feedback = session.feed(line);
    let doc = session.document();
    let status = feedback_status(&feedback);
    let body = json::object(&[
        ("status", json::string(status)),
        (
            "message",
            match feedback.message() {
                Some(m) => json::string(m),
                None => "null".to_string(),
            },
        ),
        ("line", json::string(line)),
        ("revision", json::int(doc.revision())),
        ("changed", json::bool_(doc.revision() != before)),
        ("entities", json::int(doc.len() as u64)),
        (
            "selection",
            json::array(session.selection().iter().map(|id| json::int(id.0))),
        ),
        ("principal", json::string(&cfg.principal)),
    ]);
    // A rejected command is a successfully handled request that reports a
    // failed command: 200 with `"status":"error"`. Mapping it to 400 would
    // conflate "your HTTP was wrong" with "your CAD command was wrong", and a
    // client needs to tell those apart.
    Response::json(200, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "test-token";

    fn cfg(read_only: bool) -> ApiConfig {
        ApiConfig::new(0, read_only).with_token(TOKEN)
    }

    fn get(path: &str) -> Request {
        Request::new("GET", path, "").with_header("authorization", &format!("Bearer {TOKEN}"))
    }

    fn post(path: &str, body: &str) -> Request {
        Request::new("POST", path, body).with_header("authorization", &format!("Bearer {TOKEN}"))
    }

    fn run(req: &Request, session: &mut Session) -> Response {
        handle(req, session, &cfg(false)).response
    }

    #[test]
    fn viewer_role_cannot_run_commands() {
        let mut s = Session::with_builtins();
        let cfg = ApiConfig::new(0, false)
            .with_token(TOKEN)
            .with_role(aadt_object::Role::Viewer);
        let res = handle(
            &post("/v1/command", r#"{"line":"LINE 0 0 1 0"}"#),
            &mut s,
            &cfg,
        )
        .response;
        assert_eq!(res.status, 403);
        assert_eq!(s.document().len(), 0);
        let health = handle(&get("/v1/health"), &mut s, &cfg).response;
        assert_eq!(health.status, 200);
        assert!(health.body.contains(r#""capabilities""#));
    }

    #[test]
    fn every_route_needs_the_token() {
        let mut s = Session::with_builtins();
        for path in ["/v1/health", "/v1/document", "/v1/entities", "/v1/commands"] {
            let res = handle(&Request::new("GET", path, ""), &mut s, &cfg(false)).response;
            assert_eq!(res.status, 401, "{path} answered unauthenticated");
        }
        let wrong = Request::new("GET", "/v1/health", "").with_header("authorization", "Bearer no");
        assert_eq!(
            handle(&wrong, &mut s, &cfg(false)).response.status,
            401,
            "a wrong token is refused like a missing one"
        );
    }

    #[test]
    fn health_reports_liveness_and_mode() {
        let mut s = Session::with_builtins();
        let res = run(&get("/v1/health"), &mut s);
        assert_eq!(res.status, 200);
        assert!(res.body.contains(r#""status":"ok""#));
        assert!(res.body.contains(r#""read_only":false"#));

        let ro = handle(&get("/v1/health"), &mut s, &cfg(true)).response;
        assert!(ro.body.contains(r#""read_only":true"#));
    }

    #[test]
    fn command_runs_through_the_same_interpreter_as_the_prompt() {
        let mut s = Session::with_builtins();
        let res = run(
            &post("/v1/command", r#"{"line":"LINE 0 0 10 0 A-WALL"}"#),
            &mut s,
        );
        assert_eq!(res.status, 200);
        assert!(res.body.contains(r#""status":"ok""#), "{}", res.body);
        assert!(res.body.contains(r#""changed":true"#));
        assert!(res.body.contains(r#""entities":1"#));
        assert_eq!(s.document().len(), 1);
        // The API call lands in history exactly like a typed line, so the
        // transcript is a complete record of what touched the drawing.
        assert_eq!(s.history(), &["LINE 0 0 10 0 A-WALL".to_string()]);
    }

    #[test]
    fn a_rejected_command_is_a_200_carrying_an_error_status() {
        let mut s = Session::with_builtins();
        let res = run(&post("/v1/command", r#"{"line":"NOSUCH 1 2"}"#), &mut s);
        assert_eq!(res.status, 200, "the HTTP request itself was fine");
        assert!(res.body.contains(r#""status":"error""#));
        assert!(res.body.contains(r#""changed":false"#));
        assert_eq!(s.document().len(), 0);
    }

    #[test]
    fn read_only_refuses_commands_but_still_serves_reads() {
        let mut s = Session::with_builtins();
        s.feed("LINE 0 0 10 0 A-WALL");
        let ro = cfg(true);
        let res = handle(&post("/v1/command", r#"{"line":"ERASE ALL"}"#), &mut s, &ro).response;
        assert_eq!(res.status, 403);
        assert_eq!(s.document().len(), 1, "nothing ran");
        let read = handle(&get("/v1/entities"), &mut s, &ro).response;
        assert_eq!(read.status, 200);
        assert!(read.body.contains(r#""total":1"#));
    }

    #[test]
    fn command_body_must_carry_one_single_line() {
        let mut s = Session::with_builtins();
        assert_eq!(run(&post("/v1/command", "not json"), &mut s).status, 400);
        assert_eq!(run(&post("/v1/command", r#"{"x":1}"#), &mut s).status, 400);
        let two = run(
            &post("/v1/command", r#"{"line":"LINE 0 0 1 1\nERASE ALL"}"#),
            &mut s,
        );
        assert_eq!(two.status, 400, "two commands behind one status code");
        assert_eq!(s.document().len(), 0);
    }

    #[test]
    fn entities_page_deterministically_and_filter_by_layer() {
        let mut s = Session::with_builtins();
        s.feed("LINE 0 0 10 0 A-WALL");
        s.feed("LINE 0 5 10 5 A-WALL");
        s.feed("CIRCLE 5 5 2 A-COL");

        let all = run(&get("/v1/entities"), &mut s);
        assert!(all.body.contains(r#""total":3"#));

        let walls = run(&get("/v1/entities?layer=A-WALL"), &mut s);
        assert!(walls.body.contains(r#""total":2"#), "{}", walls.body);

        let page = run(&get("/v1/entities?limit=1&offset=1"), &mut s);
        assert!(page.body.contains(r#""count":1"#));
        let again = run(&get("/v1/entities?limit=1&offset=1"), &mut s);
        assert_eq!(page.body, again.body, "identical calls, identical bytes");

        assert_eq!(run(&get("/v1/entities?limit=x"), &mut s).status, 400);
    }

    #[test]
    fn entity_lookup_by_id_reports_geometry_and_404s_honestly() {
        let mut s = Session::with_builtins();
        s.feed("CIRCLE 5 5 2 A-COL");
        let id = s.document().entities()[0].id.0;
        let res = run(&get(&format!("/v1/entities/{id}")), &mut s);
        assert_eq!(res.status, 200);
        assert!(res.body.contains(r#""kind":"circle""#), "{}", res.body);
        assert!(res.body.contains(r#""radius":2"#));
        assert_eq!(run(&get("/v1/entities/99999"), &mut s).status, 404);
        assert_eq!(run(&get("/v1/entities/abc"), &mut s).status, 400);
    }

    #[test]
    fn document_reports_extents_and_null_when_empty() {
        let mut s = Session::with_builtins();
        let empty = run(&get("/v1/document"), &mut s);
        assert!(empty.body.contains(r#""extents":null"#), "{}", empty.body);
        s.feed("LINE 0 0 10 4 A-WALL");
        let filled = run(&get("/v1/document"), &mut s);
        assert!(filled.body.contains(r#""width":10"#), "{}", filled.body);
        assert!(filled.body.contains(r#""height":4"#));
    }

    #[test]
    fn selection_history_transcript_and_commands_project_the_session() {
        let mut s = Session::with_builtins();
        s.feed("LINE 0 0 10 0 A-WALL");
        s.feed("SELECT ALL");
        assert!(run(&get("/v1/selection"), &mut s)
            .body
            .contains(r#""count":1"#));
        assert!(run(&get("/v1/history"), &mut s).body.contains("SELECT ALL"));
        assert!(run(&get("/v1/transcript"), &mut s)
            .body
            .contains(r#""status":"ok""#));
        let cmds = run(&get("/v1/commands?prefix=LI"), &mut s);
        assert!(cmds.body.contains("LINE"), "{}", cmds.body);
        assert!(run(&get("/v1/layers"), &mut s).body.contains("A-WALL"));
    }

    #[test]
    fn unknown_routes_404_and_known_routes_405_on_the_wrong_verb() {
        let mut s = Session::with_builtins();
        assert_eq!(run(&get("/v1/nope"), &mut s).status, 404);
        assert_eq!(run(&post("/v1/document", ""), &mut s).status, 405);
        assert_eq!(run(&get("/v1/command"), &mut s).status, 405);
        // A trailing slash is the same route, not a missing one.
        assert_eq!(run(&get("/v1/health/"), &mut s).status, 200);
    }

    #[test]
    fn shutdown_is_the_only_route_that_stops_the_loop() {
        let mut s = Session::with_builtins();
        assert!(!handle(&get("/v1/health"), &mut s, &cfg(false)).shutdown);
        let stop = handle(&post("/v1/shutdown", ""), &mut s, &cfg(false));
        assert!(stop.shutdown);
        assert_eq!(stop.response.status, 200);
    }

    #[test]
    fn index_lists_the_routes_it_serves() {
        let mut s = Session::with_builtins();
        let body = run(&get("/v1"), &mut s).body;
        for route in ["/v1/document", "/v1/entities", "/v1/command"] {
            assert!(body.contains(route), "index omits {route}: {body}");
        }
    }

    #[test]
    fn constant_time_eq_still_compares_correctly() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
        assert!(!constant_time_eq("", "a"));
        assert!(constant_time_eq("", ""));
    }
}
