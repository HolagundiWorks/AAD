//! `cadcraft-cli ask`: one request to the local assistant, run to the end.
//!
//! `cadcraft-cli ask [--file drawing.dxf | --sample] [--yes] [--save out.dxf] "audit this drawing"`
//!
//! The model proposes a tool; the engine does the work; a tool that changes the drawing asks first
//! (`--yes` answers for you). The answer goes to standard output; progress (the model's own words as
//! they are produced, and the tools that ran) goes to standard error.

use std::io::Write;
use std::sync::atomic::AtomicBool;

use cadcraft_agent::Assistant;
use cadcraft_agent::driver::{Confirmation, ask};
use cadcraft_engine::Session;
use serde_json::json;

const USAGE: &str = "usage: cadcraft-cli ask [--file drawing.dxf | --sample] [--yes] [--save out.dxf] \"request\"";

pub fn run(args: &[String]) -> Result<(), String> {
    let (mut file, mut sample, mut yes, mut save, mut text) = (None, false, false, None, Vec::new());
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--file" => file = Some(it.next().ok_or(USAGE)?.clone()),
            "--sample" => sample = true,
            "--yes" => yes = true,
            "--save" => save = Some(it.next().ok_or(USAGE)?.clone()),
            s if s.starts_with("--") => return Err(format!("ask: unknown option {s}\n{USAGE}")),
            _ => text.push(a.clone()),
        }
    }
    let request = text.join(" ");
    if request.trim().is_empty() {
        return Err(USAGE.into());
    }
    if !cadcraft_llm::available() {
        return Err("this build has no language model runtime: build with `--features assistant` (see docs/BUILD_AND_DEVELOPMENT.md)".into());
    }

    let mut session = Session::new();
    if let Some(f) = &file {
        session.execute("open", &json!({ "path": f })).map_err(|e| format!("could not open {f}: {e}"))?;
    } else if sample {
        session.open_drawing(cadcraft_engine::sample::default_sample(), "Bracket", None);
    }

    let t = std::time::Instant::now();
    let model = cadcraft_llm::Model::load_default(&cadcraft_llm::LoadOptions::default()).map_err(|e| e.to_string())?;
    eprintln!("[model {} loaded in {:.1} s]", model.path().display(), t.elapsed().as_secs_f64());

    let mut assistant = Assistant::new();
    let cancel = AtomicBool::new(false);
    let mut confirm = |c: &Confirmation| {
        eprintln!("\n[confirmation needed] {} ({:?}): {}", c.tool, c.risk, c.summary);
        if yes {
            eprintln!("[--yes: allowed]");
            return true;
        }
        eprint!("Allow? [y/N] ");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).is_ok() && matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
    };
    let mut on_piece = |p: &str| {
        eprint!("{p}");
        let _ = std::io::stderr().flush();
    };
    let t = std::time::Instant::now();
    let answer = ask(&model, &mut assistant, &mut session, &request, &mut confirm, &cancel, &mut on_piece);
    eprintln!("\n[{:.1} s]", t.elapsed().as_secs_f64());
    for e in assistant.take_events() {
        eprintln!("[tool] {} -> {}", e.tool, e.status);
    }
    if let Some(out) = save {
        session.execute("saveas", &json!({ "path": out })).map_err(|e| format!("could not save {out}: {e}"))?;
        eprintln!("[saved {out}]");
    }
    println!("{}", answer?);
    Ok(())
}
