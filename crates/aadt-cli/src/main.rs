//! # aadt — command-line driver (spec §2 principle 2, §6.2)
//!
//! The headless entry point for AADT's M0 vertical slice. The interactive,
//! command-line-first desktop shell (`aadt-app`) grows from M1; until then this
//! binary is how the deterministic pipeline is driven and tested.
//!
//! ```text
//! aadt takeoff <input.dxf> [--csv <out.csv>]
//!              [--min-thickness <n>] [--max-thickness <n>] [--min-overlap <n>]
//! ```
//!
//! With `--csv`, the takeoff is written to that file; otherwise it prints to stdout.
//! A human-readable summary always goes to stdout, mirroring the CAD command-line
//! convention users already trust (spec §4.3, §6.2).

use std::process::ExitCode;

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
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        other => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    }
}

const USAGE: &str = "\
AADT — AI-Aided Architecture Design and Takeoff

Usage:
  aadt takeoff <input.dxf> [options]

Options:
  --csv <path>             Write the takeoff CSV to <path> (default: stdout)
  --min-thickness <n>      Minimum wall face offset (default 0.05)
  --max-thickness <n>      Maximum wall face offset (default 1.0)
  --min-overlap <n>        Minimum wall run overlap (default 0.1)
  -h, --help               Show this help";

fn cmd_takeoff(args: &[String]) -> Result<(), String> {
    let mut input: Option<String> = None;
    let mut csv_out: Option<String> = None;
    let mut cfg = DetectionConfig::default();

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--csv" => csv_out = Some(expect_value(&mut it, "--csv")?),
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
        "Analyzed: {} walls, {} rooms, {} graph nodes",
        detection.walls.len(),
        detection.rooms.len(),
        detection.graph.nodes.len()
    );

    let table = TakeoffTable::from_detection(&detection);
    let csv = table.to_csv();

    match &csv_out {
        Some(path) => {
            std::fs::write(path, &csv).map_err(|e| format!("writing {path}: {e}"))?;
            println!("Wrote takeoff to {path}");
        }
        None => {
            print!("\n{csv}");
        }
    }

    println!(
        "Totals: wall length {:.3}, wall footprint {:.3}, room area {:.3}",
        table.total_wall_length(),
        table.total_wall_footprint(),
        table.total_room_area()
    );
    Ok(())
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
