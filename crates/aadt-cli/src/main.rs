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

use aadt_command::{Context, Interpreter};
use aadt_detect::anomaly::{AnomalyConfig, Severity};
use aadt_detect::{detect, DetectionConfig};
use aadt_takeoff::TakeoffTable;

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
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
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

const USAGE: &str = "\
AADT — AI-Aided Architecture Design and Takeoff

Usage:
  aadt takeoff <input.dxf> [options]     One-shot deterministic takeoff
  aadt run <script.aadt>                 Execute a command script
  aadt shell                             Interactive command prompt (read from stdin)
  aadt render <input.dxf> --png <out>    Render to a PNG via the GPU canvas (needs --features gpu)

Takeoff options:
  --csv <path>             Write the takeoff CSV to <path> (default: stdout)
  --xlsx <path>            Write the takeoff as an .xlsx workbook to <path>
  --min-thickness <n>      Minimum wall face offset (default 0.05)
  --max-thickness <n>      Maximum wall face offset (default 1.0)
  --min-overlap <n>        Minimum wall run overlap (default 0.1)
  -h, --help               Show this help

Script commands: LINE, POLYLINE, CIRCLE, ARC, TEXT, INSERT, MOVE, COPY, ERASE,
LAYER, LAYERS, UNDO, REDO, LIST, OPEN, SAVE, TAKEOFF, SCHEDULE (and aliases).
Lines starting with # or ; are comments. In `shell`, :help / :complete / :quit
are meta-commands.";

fn cmd_run(args: &[String]) -> Result<(), String> {
    let path = args
        .first()
        .ok_or_else(|| format!("run needs a script file\n\n{USAGE}"))?;
    let script = std::fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))?;

    let mut interp = Interpreter::with_builtins();
    commands::register(&mut interp);
    let mut ctx = Context::new();

    let output = interp.run_script(&mut ctx, &script)?;
    for line in output {
        println!("{line}");
    }
    Ok(())
}

fn cmd_takeoff(args: &[String]) -> Result<(), String> {
    let mut input: Option<String> = None;
    let mut csv_out: Option<String> = None;
    let mut xlsx_out: Option<String> = None;
    let mut cfg = DetectionConfig::default();

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--csv" => csv_out = Some(expect_value(&mut it, "--csv")?),
            "--xlsx" => xlsx_out = Some(expect_value(&mut it, "--xlsx")?),
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
        // so `--xlsx` alone does not also dump the CSV to the terminal.
        None if xlsx_out.is_none() => {
            print!("\n{csv}");
        }
        None => {}
    }

    if let Some(path) = &xlsx_out {
        std::fs::write(path, table.to_xlsx()).map_err(|e| format!("writing {path}: {e}"))?;
        println!("Wrote takeoff workbook to {path}");
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
