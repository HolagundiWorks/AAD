//! Live walk-through of the AORMS commands against a real server (not run in CI):
//!   AORMS_URL=http://localhost:3128 AORMS_EMAIL=… AORMS_PASSWORD=… cargo run -p cadcraft-engine --example aorms_smoke
//! Signs in as app `aad`, lists projects, pulls the first bound project's details and floors into a scratch drawing, signs out.
use cadcraft_engine::Session;
use serde_json::{Value, json};

fn main() {
    let (Ok(url), Ok(email), Ok(password)) = (std::env::var("AORMS_URL"), std::env::var("AORMS_EMAIL"), std::env::var("AORMS_PASSWORD")) else {
        eprintln!("Set AORMS_URL, AORMS_EMAIL and AORMS_PASSWORD.");
        std::process::exit(2);
    };
    let mut failures = 0;
    let mut check = |name: &str, ok: bool, detail: String| {
        println!("{} {name}{}", if ok { "OK  " } else { "FAIL" }, if detail.is_empty() { String::new() } else { format!(" — {detail}") });
        if !ok {
            failures += 1;
        }
    };
    let mut s = Session::new();
    let r = s.execute("aorms.signin", &json!({ "email": email, "password": password, "baseUrl": url }));
    let v = r.as_ref().cloned().unwrap_or(Value::Null);
    check("sign in as AAD", r.is_ok() && v["connected"] == true, format!("{} · {} · {}", v["studio"], v["role"], v["plan"]));
    let p = s.execute("aorms.projects", &json!({})).unwrap_or(Value::Null);
    let list = p["projects"].as_array().cloned().unwrap_or_default();
    check("projects listed", !list.is_empty(), format!("{} projects, {} bound", list.len(), list.iter().filter(|x| x["bound"] == true).count()));
    if let Some(target) = list.iter().find(|x| x["bound"] == true).or(list.first()) {
        let id = target["projectOfficeId"].as_str().unwrap_or("");
        let r = s.execute("aorms.open", &json!({ "projectOfficeId": id }));
        let o = r.as_ref().cloned().unwrap_or(Value::Null);
        check(
            "project info pulled into the drawing",
            r.is_ok() && o["details"]["title"].is_string(),
            format!("{} · warnings {}", o["details"]["title"], o["warnings"]),
        );
        let d = s.execute("project", &json!({})).unwrap_or(Value::Null);
        check("details are in the drawing's project data", d["details"]["title"] == o["details"]["title"], String::new());
        let dr = s.execute("aorms.drawings", &json!({ "projectOfficeId": id })).unwrap_or(Value::Null);
        check("drawing list readable", dr["drawings"].is_array(), format!("{} drawings", dr["drawings"].as_array().map_or(0, Vec::len)));
        let pull = s.execute("aorms.pull", &json!({}));
        check("pull refreshes the linked project", pull.is_ok(), String::new());
    }
    let st = s.execute("aorms.status", &json!({})).unwrap_or(Value::Null);
    check("status shows the link", st["link"]["projectOfficeId"].is_string(), String::new());
    check("sign out", s.execute("aorms.signout", &json!({})).is_ok(), String::new());
    println!("{}", if failures == 0 { "All checks passed.".to_string() } else { format!("{failures} failed") });
    std::process::exit(i32::from(failures > 0));
}
