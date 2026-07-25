//! # aadt — command-line driver (spec §2 principle 2, §6.2)
//!
//! The headless entry point for AADT. The interactive, command-line-first desktop
//! shell (`aadt-app`) grows from M1; until then this binary drives the pipeline:
//!
//! ```text
//! aadt takeoff <input.dxf> [--csv <out.csv>] [--xlsx <out.xlsx>]
//!                                                 # one-shot deterministic takeoff
//!              [--min-thickness <n>] [--max-thickness <n>] [--min-overlap <n>]
//! aadt run <script.aadt>                          # execute a command script
//! aadt shell                                      # interactive command prompt
//! ```
//!
//! `run` executes a script through the command interpreter (`aadt-command`) with the
//! drawing built-ins plus `OPEN`/`SAVE`/`TAKEOFF`, mirroring the CAD command-line
//! convention users already trust (spec §4.2, §4.3, §6.2). `shell` drives the same
//! command set interactively through [`aadt_app::Session`] — the GUI-agnostic core a
//! future desktop front-end will share (see `shell.rs`).

mod commands;
mod shell;

use std::process::ExitCode;

use aadt_command::Interpreter;
use aadt_detect::anomaly::{AnomalyConfig, Severity};
use aadt_detect::{detect, DetectionConfig};
use aadt_docs::{
    NotesLibrary, NotesStyle, Orientation, PaperSize, QaSeverity, Sheet, SheetReview, TitleBlock,
};
use aadt_takeoff::{TakeoffTable, TAKEOFF_SCHEMA_VERSION};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("aadt: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let Some((command, rest)) = args.split_first() else {
        return Err(format!("no command given\n\n{USAGE}"));
    };
    match command.as_str() {
        "takeoff" => cmd_takeoff(rest),
        "run" => cmd_run(rest),
        "shell" | "repl" => shell::run(),
        "render" => cmd_render(rest),
        "view" => cmd_view(rest),
        "sheet" => cmd_sheet(rest),
        "lisp" => cmd_lisp(rest),
        "pdf" => cmd_pdf(rest),
        "compare" => cmd_compare(rest),
        "collab" => cmd_collab(rest),
        "app" => cmd_app(rest),
        "infer" => cmd_infer(rest),
        "present" => cmd_present(rest),
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        "--version" | "-V" | "version" => {
            println!(
                "aadt {} (takeoff schema {})",
                env!("CARGO_PKG_VERSION"),
                TAKEOFF_SCHEMA_VERSION
            );
            Ok(())
        }
        other => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    }
}

/// Render a DXF to a PNG via the GPU canvas. Built only with `--features gpu`;
/// without it the command explains how to enable it rather than failing opaquely.
#[cfg(feature = "gpu")]
fn cmd_render(args: &[String]) -> Result<(), String> {
    use aadt_render::gpu::WgpuRenderer;
    use aadt_render::{build_drawlist, Camera, Renderer};

    let mut input: Option<String> = None;
    let mut png_out: Option<String> = None;
    let (mut width, mut height) = (1600u32, 1200u32);

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--png" => png_out = Some(expect_value(&mut it, "--png")?),
            "--width" => width = parse_num(&mut it, "--width")? as u32,
            "--height" => height = parse_num(&mut it, "--height")? as u32,
            other if other.starts_with('-') => return Err(format!("unknown option {other:?}")),
            other => input = Some(other.to_string()),
        }
    }

    let input = input.ok_or_else(|| format!("render needs an input DXF\n\n{USAGE}"))?;
    let out = png_out.ok_or("render needs --png <path>")?;

    let doc = aadt_dxf::import_path(&input).map_err(|e| e.to_string())?;
    println!("Loaded {input} ({} entities)", doc.len());

    let mut renderer = WgpuRenderer::new(width, height).map_err(|e| e.to_string())?;
    let mut camera = Camera::new((width as f64, height as f64));
    camera.fit_document(&doc, 0.08);

    let list = build_drawlist(&doc);
    renderer.render(&camera, &list);
    renderer.save_png(&out).map_err(|e| e.to_string())?;
    println!(
        "Rendered {} primitives to {out} ({width}x{height})",
        list.len()
    );
    Ok(())
}

#[cfg(not(feature = "gpu"))]
fn cmd_render(_args: &[String]) -> Result<(), String> {
    Err("the `render` command needs the GPU backend; rebuild with `--features gpu`".to_string())
}

/// Open the interactive canvas window on a DXF. Built only with `--features gui`.
#[cfg(feature = "gui")]
fn cmd_view(args: &[String]) -> Result<(), String> {
    let input = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .ok_or_else(|| format!("view needs an input DXF\n\n{USAGE}"))?;
    let doc = aadt_dxf::import_path(input).map_err(|e| e.to_string())?;
    println!(
        "Opening {input} ({} entities) — drag to pan, scroll to zoom, F to fit, Esc to quit",
        doc.len()
    );
    aadt_render::view::run(doc).map_err(|e| e.to_string())
}

#[cfg(not(feature = "gui"))]
fn cmd_view(_args: &[String]) -> Result<(), String> {
    Err(
        "the `view` command needs the interactive window; rebuild with `--features gui`"
            .to_string(),
    )
}

/// Compose a sheet: place a border, a title block, and the notes the drawing's
/// detected elements call for, then run documentation QA and save (spec §5.3).
fn cmd_sheet(args: &[String]) -> Result<(), String> {
    let mut input: Option<String> = None;
    let mut out: Option<String> = None;
    let mut size = "A1".to_string();
    let mut tb = TitleBlock::default();

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => out = Some(expect_value(&mut it, "--out")?),
            "--size" => size = expect_value(&mut it, "--size")?,
            "--project" => tb.project = expect_value(&mut it, "--project")?,
            "--title" => tb.sheet_title = expect_value(&mut it, "--title")?,
            "--number" => {
                let n = expect_value(&mut it, "--number")?;
                tb.drawing_number = n.clone();
                tb.sheet_number = n;
            }
            "--scale" => tb.scale = expect_value(&mut it, "--scale")?,
            "--date" => tb.date = expect_value(&mut it, "--date")?,
            "--by" => tb.drawn_by = expect_value(&mut it, "--by")?,
            other if other.starts_with('-') => return Err(format!("unknown option {other:?}")),
            other => input = Some(other.to_string()),
        }
    }

    let input = input.ok_or_else(|| format!("sheet needs an input DXF\n\n{USAGE}"))?;
    let out = out.ok_or("sheet needs --out <path>")?;
    let paper =
        PaperSize::from_name(&size).ok_or_else(|| format!("unknown paper size {size:?}"))?;

    let model = aadt_dxf::import_path(&input).map_err(|e| e.to_string())?;
    println!("Loaded {input} ({} entities)", model.len());

    // Suggest notes from the drawing itself.
    let det = detect(&model, &DetectionConfig::default());
    let props = model.properties();
    let floors = model.floors();
    let notes = NotesLibrary::standard().suggest_for_drawing(&det, props, &floors);

    // Compose onto a fresh sheet (paper space), rather than mixing paper
    // millimetres with the model's own units in one document.
    // The drawing, fit into the area left of (landscape) or above (portrait) the title block.
    let sheet = Sheet::new(paper, aadt_docs::orientation_from_extents(&model), 10.0);
    let mut doc = aadt_model::Document::new();
    sheet.place(&mut doc);

    let area = aadt_docs::drawing_area_for_sheet(&sheet);
    let composed = aadt_docs::place_drawing(&mut doc, &model, &area);
    // Record the fit scale in the title block if the user didn't set one
    // (assume a model drawn in metres → 1000 mm per unit).
    if tb.scale.trim().is_empty() {
        tb.scale = composed.scale_label(1000.0);
    }
    println!("Fit scale: {}", composed.scale_label(1000.0));

    tb.place_on_sheet(&mut doc, &sheet);

    let (min, max) = sheet.inner();
    let notes_style = NotesStyle {
        origin: aadt_geometry::Point2::new(min.x + 5.0, max.y - 5.0),
        ..Default::default()
    };
    aadt_docs::place_notes(&mut doc, &notes, &notes_style);

    // Documentation QA.
    let review = SheetReview {
        require_notes: true,
        note_count: notes.len(),
        has_border: true,
    };
    let findings = review.review(&tb);
    if findings.is_empty() {
        println!("QA: complete — ready to issue.");
    } else {
        println!("QA: {} finding(s):", findings.len());
        for f in &findings {
            let sev = match f.severity {
                QaSeverity::Blocking => "BLOCKING",
                QaSeverity::Advisory => "advisory",
            };
            println!("  [{sev}] {}", f.message);
        }
    }

    aadt_dxf::export_path(&doc, &out).map_err(|e| e.to_string())?;
    println!(
        "Wrote sheet to {out} ({} entities, {} notes placed)",
        doc.len(),
        notes.len()
    );
    Ok(())
}

/// AADT-PRESENT: lay several drawings out as views on one sheet (spec §5.3).
fn cmd_present(args: &[String]) -> Result<(), String> {
    let mut inputs: Vec<String> = Vec::new();
    let mut out: Option<String> = None;
    let mut size = "A1".to_string();

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => out = Some(expect_value(&mut it, "--out")?),
            "--size" => size = expect_value(&mut it, "--size")?,
            other if other.starts_with('-') => return Err(format!("unknown option {other:?}")),
            other => inputs.push(other.to_string()),
        }
    }
    if inputs.is_empty() {
        return Err(format!("present needs at least one input DXF\n\n{USAGE}"));
    }
    let out = out.ok_or("present needs --out <path>")?;
    let paper =
        PaperSize::from_name(&size).ok_or_else(|| format!("unknown paper size {size:?}"))?;

    // Load each drawing, keeping its label (the file stem).
    let mut docs: Vec<(String, aadt_model::Document)> = Vec::new();
    for path in &inputs {
        let d = aadt_dxf::import_path(path).map_err(|e| e.to_string())?;
        let label = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_uppercase())
            .unwrap_or_default();
        docs.push((label, d));
    }

    let sheet = Sheet::new(paper, Orientation::Landscape, 10.0);
    let mut doc = aadt_model::Document::new();
    sheet.place(&mut doc);

    // A near-square grid: cols = ceil(sqrt(n)).
    let n = docs.len();
    let cols = (n as f64).sqrt().ceil() as usize;
    let rows = n.div_ceil(cols);
    let areas = aadt_docs::grid_areas(&sheet, cols, rows, 15.0);

    let views: Vec<aadt_docs::View> = docs
        .iter()
        .zip(&areas)
        .map(|((label, d), area)| aadt_docs::View {
            doc: d,
            area: *area,
            label: label.clone(),
        })
        .collect();
    let scales = aadt_docs::present(&mut doc, &views);

    aadt_dxf::export_path(&doc, &out).map_err(|e| e.to_string())?;
    println!("Composed {n} view(s) in a {cols}x{rows} grid onto {out}:");
    for ((label, _), scale) in docs.iter().zip(&scales) {
        let ratio = if *scale > 0.0 {
            format!("1:{}", (1000.0 / scale).round() as i64)
        } else {
            "empty".to_string()
        };
        println!("  {label}: {ratio}");
    }
    Ok(())
}

/// Run an embedded LISP script. With `--dxf`, the drawing is loaded and the
/// `aadt-*` query functions are available to the script (read-only).
fn cmd_lisp(args: &[String]) -> Result<(), String> {
    let mut script: Option<String> = None;
    let mut dxf: Option<String> = None;
    let mut save: Option<String> = None;

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--dxf" => dxf = Some(expect_value(&mut it, "--dxf")?),
            "--save" => save = Some(expect_value(&mut it, "--save")?),
            other if other.starts_with('-') => return Err(format!("unknown option {other:?}")),
            other => script = Some(other.to_string()),
        }
    }

    let script = script.ok_or_else(|| format!("lisp needs a script file\n\n{USAGE}"))?;
    let src = std::fs::read_to_string(&script).map_err(|e| format!("reading {script}: {e}"))?;

    // The document is shared with the script so `aadt-*` functions can query and
    // draw into it; `--dxf` seeds it from a file, otherwise it starts empty.
    let doc = std::rc::Rc::new(std::cell::RefCell::new(match &dxf {
        Some(path) => {
            let d = aadt_dxf::import_path(path).map_err(|e| e.to_string())?;
            println!("Loaded {path} ({} entities) — aadt-* available", d.len());
            d
        }
        None => aadt_model::Document::new(),
    }));

    let mut lisp = aadt_lisp::Interpreter::new();
    aadt_lisp::host::install_document(lisp.env(), doc.clone());

    let result = lisp.eval_str(&src).map_err(|e| e.to_string())?;
    let out = lisp.output();
    if !out.is_empty() {
        print!("{out}");
        if !out.ends_with('\n') {
            println!();
        }
    }
    println!("=> {result}");

    if let Some(path) = &save {
        aadt_dxf::export_path(&doc.borrow(), path).map_err(|e| e.to_string())?;
        println!("Wrote {path} ({} entities)", doc.borrow().len());
    }
    Ok(())
}

/// Infer wall types with the local model and apply them to the drawing as one
/// undoable pass (spec §5.1 step 6 / §6.1). Degrades cleanly when no model is
/// running — the deterministic pipeline never depends on the AI being available.
fn cmd_infer(args: &[String]) -> Result<(), String> {
    let mut input: Option<String> = None;
    let mut save: Option<String> = None;
    let mut model = String::new(); // empty → backend default
    let mut backend = aadt_ai::AiBackend::FoundryLocal;

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--save" => save = Some(expect_value(&mut it, "--save")?),
            "--model" => model = expect_value(&mut it, "--model")?,
            "--backend" => {
                let v = expect_value(&mut it, "--backend")?;
                backend = aadt_ai::AiBackend::parse(&v).ok_or_else(|| {
                    format!("unknown --backend {v:?}; use foundry-local or ollama")
                })?;
            }
            other if other.starts_with('-') => return Err(format!("unknown option {other:?}")),
            other => input = Some(other.to_string()),
        }
    }
    let input = input.ok_or_else(|| format!("infer needs an input DXF\n\n{USAGE}"))?;

    let mut doc = aadt_dxf::import_path(&input).map_err(|e| e.to_string())?;
    let det = detect(&doc, &DetectionConfig::default());
    let model_label = if model.is_empty() {
        match backend {
            aadt_ai::AiBackend::FoundryLocal => aadt_ai::DEFAULT_FOUNDRY_MODEL,
            aadt_ai::AiBackend::Ollama => "llama3",
        }
    } else {
        model.as_str()
    };
    println!(
        "Loaded {input}: {} wall(s) to classify via {} / {model_label}",
        det.walls.len(),
        backend.as_str()
    );
    if backend == aadt_ai::AiBackend::FoundryLocal {
        println!(
            "Foundry Local endpoint: {} (set AADT_FOUNDRY_ENDPOINT to override)",
            aadt_ai::resolved_endpoint()
        );
    }

    let client = aadt_ai::make_client(backend, &model);
    let mut audit = aadt_ai::AuditLog::new();
    match aadt_ai::infer_wall_types_open(client.as_ref(), &mut audit, &det, &doc) {
        Ok(assignments) => {
            let n = aadt_ai::propose_wall_types(
                &mut doc,
                &det,
                &assignments,
                aadt_ai::DEFAULT_AUTO_ACCEPT_THRESHOLD,
            );
            let counts = aadt_ai::status_counts(&doc);
            let review = aadt_ai::list_for_review(&doc, aadt_ai::DEFAULT_AUTO_ACCEPT_THRESHOLD);
            println!(
                "Proposed {n} wall type(s) as attributes (auto-accept ≥ {:.0}%).",
                aadt_ai::DEFAULT_AUTO_ACCEPT_THRESHOLD * 100.0
            );
            println!(
                "Status: proposed={} accepted={} frozen={} rejected={}; {} need review.",
                counts[0].1,
                counts[1].1,
                counts[2].1,
                counts[3].1,
                review.len()
            );
            if let Some(path) = &save {
                aadt_dxf::export_path(&doc, path).map_err(|e| e.to_string())?;
                println!("Wrote {path}");
            }
        }
        // AI is an accelerant, never a dependency (spec §6.1 step 7): any failure
        // — model down, or a response we can't use — leaves the drawing untouched
        // and is reported, not fatal.
        Err(e) => {
            println!("AI inference skipped: {e}.");
            println!("The drawing is unchanged — the deterministic pipeline does not need AI.");
            if backend == aadt_ai::AiBackend::FoundryLocal {
                println!(
                    "Tip: start Foundry Local, load {model_label}, and expose the OpenAI-compatible API."
                );
            }
        }
    }
    println!("(audit: {} model call(s) logged)", audit.entries().len());
    Ok(())
}

/// Plot a DXF to a vector PDF (spec §4.5).
fn cmd_pdf(args: &[String]) -> Result<(), String> {
    let mut input: Option<String> = None;
    let mut out: Option<String> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => out = Some(expect_value(&mut it, "--out")?),
            other if other.starts_with('-') => return Err(format!("unknown option {other:?}")),
            other => input = Some(other.to_string()),
        }
    }
    let input = input.ok_or_else(|| format!("pdf needs an input DXF\n\n{USAGE}"))?;
    let out = out.ok_or("pdf needs --out <path>")?;

    let doc = aadt_dxf::import_path(&input).map_err(|e| e.to_string())?;
    let bytes = aadt_pdf::plot(&doc, aadt_pdf::PageSize::A3_LANDSCAPE);
    std::fs::write(&out, &bytes).map_err(|e| format!("writing {out}: {e}"))?;
    println!(
        "Plotted {input} ({} entities) to {out} ({} bytes)",
        doc.len(),
        bytes.len()
    );
    Ok(())
}

/// Compare two DXF revisions and narrate the change set (spec §6.4).
fn cmd_compare(args: &[String]) -> Result<(), String> {
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    let [before, after] = files.as_slice() else {
        return Err(format!(
            "compare needs two DXF files: <before> <after>\n\n{USAGE}"
        ));
    };

    let b = aadt_dxf::import_path(before).map_err(|e| e.to_string())?;
    let a = aadt_dxf::import_path(after).map_err(|e| e.to_string())?;
    println!("Comparing {before} → {after}");
    print!("{}", aadt_pdf::compare(&b, &a).narrate());
    Ok(())
}

/// Demonstrate the collaboration model: two offline editors mark up a drawing,
/// then sync, and both converge to the same markup set (spec §11). By default no
/// network is involved — `merge` is the sync primitive. With `--net`, the same
/// scenario runs over a real loopback TCP relay ([`aadt_collab::sync`]).
fn cmd_collab(args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a == "--net") {
        return cmd_collab_net();
    }
    use aadt_collab::Replica;
    use aadt_geometry::Point2;
    use aadt_pdf::{Markup, MarkupKind};

    let cloud = |a: &str, n: &str| Markup::new(MarkupKind::Cloud, Point2::new(0.0, 0.0), a, n);

    let mut alice = Replica::new("alice");
    let mut bob = Replica::new("bob");

    // Both edit while offline.
    let a1 = alice.add_markup(cloud("alice", "clarify north arrow"));
    alice.add_markup(cloud("alice", "dimension the stair"));
    bob.add_markup(cloud("bob", "confirm door swing"));

    println!("Before sync:");
    println!(
        "  alice sees {} markup(s), bob sees {}",
        alice.markups().len(),
        bob.markups().len()
    );

    // Sync: exchange operations both ways.
    let (asnap, bsnap) = (alice.clone(), bob.clone());
    alice.merge(&bsnap);
    bob.merge(&asnap);

    // Bob, now seeing alice's markup, resolves one of hers.
    bob.remove(a1);
    alice.merge(&bob);

    println!("\nAfter sync (and bob resolving one of alice's notes):");
    for (who, r) in [("alice", &alice), ("bob", &bob)] {
        let notes: Vec<&str> = r.markups().iter().map(|(_, m)| m.note.as_str()).collect();
        println!("  {who}: {:?}", notes);
    }
    let converged = alice.markups().len() == bob.markups().len();
    println!("\nParticipants: {:?}", alice.participants());
    println!("Converged: {converged}");
    Ok(())
}

/// The same convergence demo, but driven over a real TCP relay on loopback: a
/// relay server plus two clients, each edit shipped as an operation and merged
/// on arrival. Proves the transport, not just the CRDT.
fn cmd_collab_net() -> Result<(), String> {
    use std::time::{Duration, Instant};

    use aadt_collab::{Client, RelayServer};
    use aadt_geometry::Point2;
    use aadt_pdf::{Markup, MarkupKind};

    let cloud = |a: &str, n: &str| Markup::new(MarkupKind::Cloud, Point2::new(0.0, 0.0), a, n);

    let server = RelayServer::start("127.0.0.1:0").map_err(|e| e.to_string())?;
    println!("Relay listening on {}", server.addr());

    let mut alice = Client::connect(server.addr(), "alice").map_err(|e| e.to_string())?;
    let mut bob = Client::connect(server.addr(), "bob").map_err(|e| e.to_string())?;

    let a1 = alice
        .add_markup(cloud("alice", "clarify north arrow"))
        .map_err(|e| e.to_string())?;
    bob.add_markup(cloud("bob", "confirm door swing"))
        .map_err(|e| e.to_string())?;

    // Wait (bounded) for both ops to fan out to both clients.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !(alice.markups().len() == 2 && bob.markups().len() == 2) {
        std::thread::sleep(Duration::from_millis(5));
    }

    let show = |who: &str, c: &Client| {
        let notes: Vec<String> = c.markups().iter().map(|(_, m)| m.note.clone()).collect();
        println!("  {who}: {notes:?}");
    };
    println!("\nAfter both edits fan out over TCP:");
    show("alice", &alice);
    show("bob", &bob);

    // Bob resolves one of alice's notes; the remove propagates back.
    bob.remove(a1).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && alice.markups().len() != 1 {
        std::thread::sleep(Duration::from_millis(5));
    }

    println!("\nAfter bob resolves one of alice's notes:");
    show("alice", &alice);
    show("bob", &bob);

    let a = alice.snapshot();
    let b = bob.snapshot();
    let converged = a.markups().len() == b.markups().len();
    println!("\nParticipants: {:?}", a.participants());
    println!("Converged over the network: {converged}");
    Ok(())
}

/// Launch the egui desktop shell (ADR-0001). Built only with `--features app`.
#[cfg(feature = "app")]
fn cmd_app(args: &[String]) -> Result<(), String> {
    let mut doc = match args.iter().find(|a| !a.starts_with('-')) {
        Some(path) => {
            let p = std::path::Path::new(path);
            match p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .as_deref()
            {
                Some("aadt") => aadt_store::load_path(p).map_err(|e| e.to_string())?,
                _ => aadt_dxf::import_path(p).map_err(|e| e.to_string())?,
            }
        }
        None => aadt_model::Document::new(),
    };
    aadt_standards::apply_standard_layers(&mut doc);
    let mut interp = Interpreter::with_builtins();
    commands::register(&mut interp);
    let mut session = aadt_app::Session::from_interpreter(interp);
    session.set_document(doc);
    aadt_app::gui::run(session).map_err(|e| e.to_string())
}

#[cfg(not(feature = "app"))]
fn cmd_app(_args: &[String]) -> Result<(), String> {
    Err("the `app` command needs the desktop shell; rebuild with `--features app`".to_string())
}

const USAGE: &str = "\
AADT — AI-Aided Architecture Design and Takeoff

Usage:
  aadt takeoff <input.dxf> [options]     One-shot deterministic takeoff
  aadt run <script.aadt>                 Execute a command script
  aadt shell                             Interactive command prompt (read from stdin)
  aadt render <input.dxf> --png <out>    Render to a PNG via the GPU canvas (needs --features gpu)
  aadt view <input.dxf>                  Open the interactive canvas window (needs --features gui)
  aadt sheet <input.dxf> --out <out.dxf> Compose a sheet: border, title block, notes, QA
      [--size A1] [--project X] [--title Y] [--number A-101] [--scale 1:100] [--date D] [--by NN]
  aadt present <a.dxf> <b.dxf> ... --out <out.dxf>
                                          Lay several drawings out as views on one sheet [--size A1]
  aadt lisp <script.lsp> [--dxf <in.dxf>] [--save <out.dxf>]
                                          Run a LISP script; aadt-* can query and draw into the drawing
  aadt pdf <input.dxf> --out <out.pdf>    Plot a DXF to a vector PDF
  aadt compare <before.dxf> <after.dxf>   Diff two revisions and narrate the changes
  aadt infer <input.dxf> [--save <out>] [--model <name>] [--backend foundry-local|ollama]
                                  Infer wall types (default: Foundry Local / phi-3.5-mini)
  aadt collab [--net]                     Demonstrate convergent multi-user markup sync
                                          (--net runs it over a real loopback TCP relay)
  aadt app [input.dxf]                    Launch the egui desktop shell (needs --features app)

Takeoff options:
  --csv <path>             Write the takeoff CSV to <path> (default: stdout)
  --xlsx <path>            Write the takeoff as an .xlsx workbook to <path>
  --json <path>            Write the takeoff as machine-readable JSON to <path>
  --min-thickness <n>      Minimum wall face offset (default 0.05)
  --max-thickness <n>      Maximum wall face offset (default 1.0)
  --min-overlap <n>        Minimum wall run overlap (default 0.1)
  -h, --help               Show this help
  -V, --version            Print the engine and takeoff-schema version

Script commands: LINE, POLYLINE, CIRCLE, ARC, TEXT, INSERT, MOVE, COPY, ERASE,
LAYER, LAYERS, UNDO, REDO, LIST, OPEN, SAVE, TAKEOFF, SCHEDULE, LISP / (…), LISPLOAD (and aliases).
Lines starting with # or ; are comments. In `shell`, :help / :complete / :quit
are meta-commands.";

fn cmd_run(args: &[String]) -> Result<(), String> {
    let path = args
        .first()
        .ok_or_else(|| format!("run needs a script file\n\n{USAGE}"))?;
    let script = std::fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))?;

    let mut interp = Interpreter::with_builtins();
    commands::register(&mut interp);
    let mut session = aadt_app::Session::from_interpreter(interp);

    for (n, line) in script.lines().enumerate() {
        match session.feed(line) {
            aadt_app::Feedback::Ok(msg) => println!("{msg}"),
            aadt_app::Feedback::Err(e) => {
                return Err(format!("line {}: {e}", n + 1));
            }
            aadt_app::Feedback::Silent => {}
        }
    }
    Ok(())
}

fn cmd_takeoff(args: &[String]) -> Result<(), String> {
    let mut input: Option<String> = None;
    let mut csv_out: Option<String> = None;
    let mut xlsx_out: Option<String> = None;
    let mut json_out: Option<String> = None;
    let mut cfg = DetectionConfig::default();

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--csv" => csv_out = Some(expect_value(&mut it, "--csv")?),
            "--xlsx" => xlsx_out = Some(expect_value(&mut it, "--xlsx")?),
            "--json" => json_out = Some(expect_value(&mut it, "--json")?),
            "--min-thickness" => cfg.min_thickness = parse_num(&mut it, "--min-thickness")?,
            "--max-thickness" => cfg.max_thickness = parse_num(&mut it, "--max-thickness")?,
            "--min-overlap" => cfg.min_overlap = parse_num(&mut it, "--min-overlap")?,
            other if other.starts_with('-') => return Err(format!("unknown option {other:?}")),
            other => {
                if input.replace(other.to_string()).is_some() {
                    return Err("more than one input file given".to_string());
                }
            }
        }
    }

    let input = input.ok_or_else(|| format!("takeoff needs an input DXF\n\n{USAGE}"))?;

    let doc = aadt_dxf::import_path(&input).map_err(|e| e.to_string())?;
    println!("Loaded {} ({} entities)", input, doc.len());

    let detection = detect(&doc, &cfg);
    println!(
        "Analyzed: {} walls, {} openings, {} rooms, {} structural, {} graph nodes",
        detection.walls.len(),
        detection.openings.len(),
        detection.rooms.len(),
        detection.structural.len(),
        detection.graph.nodes.len()
    );

    let table = TakeoffTable::from_detection(&detection);
    let csv = table.to_csv();

    match &csv_out {
        Some(path) => {
            std::fs::write(path, &csv).map_err(|e| format!("writing {path}: {e}"))?;
            println!("Wrote takeoff to {path}");
        }
        // Only fall back to stdout when no file output at all was requested,
        // so `--xlsx`/`--json` alone do not also dump the CSV to the terminal.
        None if xlsx_out.is_none() && json_out.is_none() => {
            print!("\n{csv}");
        }
        None => {}
    }

    if let Some(path) = &xlsx_out {
        std::fs::write(path, table.to_xlsx()).map_err(|e| format!("writing {path}: {e}"))?;
        println!("Wrote takeoff workbook to {path}");
    }

    if let Some(path) = &json_out {
        std::fs::write(path, table.to_json()).map_err(|e| format!("writing {path}: {e}"))?;
        println!("Wrote takeoff JSON to {path}");
    }

    println!(
        "Totals: wall length {:.3}, net wall area {:.3}, room area {:.3}",
        table.total_wall_length(),
        table.total_wall_net_area(),
        table.total_room_area()
    );

    // Advisory only — printed after the numbers so the quantities stay the
    // headline, but printed at all because a silently wrong takeoff is this
    // product's worst failure.
    let flags = detection.anomalies(&AnomalyConfig::default());
    if !flags.is_empty() {
        println!("\n{} anomal{} found:", flags.len(), plural(flags.len()));
        for f in &flags {
            let mark = match f.severity {
                Severity::Warning => "warning",
                Severity::Info => "info",
            };
            println!("  [{mark}] {}: {}", f.kind.as_token(), f.message);
        }
    }
    Ok(())
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        "y"
    } else {
        "ies"
    }
}

fn expect_value(it: &mut std::slice::Iter<'_, String>, flag: &str) -> Result<String, String> {
    it.next()
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

fn parse_num(it: &mut std::slice::Iter<'_, String>, flag: &str) -> Result<f64, String> {
    let raw = expect_value(it, flag)?;
    raw.parse::<f64>()
        .map_err(|_| format!("{flag} expects a number, got {raw:?}"))
}
