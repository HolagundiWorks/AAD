//! # aadt — command-line driver (spec §2 principle 2, §6.2)
//!
//! The headless entry point for AADT. The interactive, command-line-first desktop
//! shell (`aadt-app`) grows from M1; this binary drives the same command core:
//!
//! ```text
//! aadt run <script.aadt>                          # execute a command script
//! aadt shell                                      # interactive command prompt
//! ```
//!
//! `run` executes a script through the command interpreter (`aadt-command`) with the
//! drawing built-ins plus `OPEN`/`SAVE`, mirroring the CAD command-line convention
//! users already trust. `shell` drives the same command set interactively through
//! [`aadt_app::Session`] — the GUI-agnostic core a future desktop front-end will
//! share (see `shell.rs`).

mod commands;
mod shell;

use std::process::ExitCode;

use aadt_command::Interpreter;
use aadt_detect::{detect, DetectionConfig};
use aadt_docs::{
    NotesLibrary, NotesStyle, Orientation, PaperSize, QaSeverity, Sheet, SheetReview, TitleBlock,
};

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
        "run" => cmd_run(rest),
        "shell" | "repl" => shell::run(),
        "render" => cmd_render(rest),
        "view" => cmd_view(rest),
        "sheet" => cmd_sheet(rest),
        "lisp" => cmd_lisp(rest),
        "pdf" => cmd_pdf(rest),
        "compare" => cmd_compare(rest),
        "app" => cmd_app(rest),
        "present" => cmd_present(rest),
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        "--version" | "-V" | "version" => {
            println!("aadt {}", env!("CARGO_PKG_VERSION"));
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
AAD — Accelerated Architectural Drafting

Usage:
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
  aadt app [input.dxf]                    Launch the egui desktop shell (needs --features app)

Options:
  -h, --help               Show this help
  -V, --version            Print the version

Script commands: LINE, POLYLINE, CIRCLE, ARC, TEXT, INSERT, MOVE, COPY, ERASE,
LAYER, LAYERS, UNDO, REDO, LIST, OPEN, SAVE, LISP / (…), LISPLOAD (and aliases).
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

fn expect_value(it: &mut std::slice::Iter<'_, String>, flag: &str) -> Result<String, String> {
    it.next()
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

// Used by the `render` command (feature "gpu"); unused in the default build.
#[cfg_attr(not(feature = "gpu"), allow(dead_code))]
fn parse_num(it: &mut std::slice::Iter<'_, String>, flag: &str) -> Result<f64, String> {
    let raw = expect_value(it, flag)?;
    raw.parse::<f64>()
        .map_err(|_| format!("{flag} expects a number, got {raw:?}"))
}
