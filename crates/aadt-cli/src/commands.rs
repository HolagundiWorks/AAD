//! CLI-registered commands that bridge the substrate command interpreter to the
//! file-I/O and AADT-engine layers.
//!
//! These live in the CLI (not `aadt-command`) so the substrate command core stays
//! free of engine dependencies: `OPEN`/`SAVE` use `aadt-dxf`, and `TAKEOFF` uses
//! `aadt-detect` + `aadt-takeoff`. They are registered onto an interpreter that
//! already carries the generic drawing built-ins.

use aadt_command::{Command, Context, Interpreter};
use aadt_detect::{detect, DetectionConfig};
use aadt_geometry::Point2;
use aadt_standards::{apply_layer_set, apply_standard_layers, generate_section, LayerSet};
use aadt_takeoff::delta::takeoff_delta_summary;
use aadt_takeoff::schedule::ScheduleStyle;
use aadt_takeoff::sink::{sink_from_spec, TakeoffPushMeta};
use aadt_takeoff::TakeoffTable;
use aadt_takeoff::TAKEOFF_SCHEMA_VERSION;
use std::path::Path;

/// Register the file-I/O and engine commands onto an interpreter.
pub fn register(interp: &mut Interpreter) {
    let registry = interp.registry_mut();
    registry.register(Box::new(OpenCmd));
    registry.register(Box::new(SaveCmd));
    registry.register(Box::new(TakeoffCmd));
    registry.register(Box::new(ScheduleCmd));
    registry.register(Box::new(HistoryCmd));
    registry.register(Box::new(LayerSetCmd));
    registry.register(Box::new(StandardsCmd));
    registry.register(Box::new(SectionCmd));
    registry.register(Box::new(BlockLibCmd));
    registry.register(Box::new(HatchLibCmd));
    registry.register(Box::new(RecognizeCmd));
    registry.register(Box::new(ReviewCmd));
    registry.register(Box::new(AcceptClassCmd));
    registry.register(Box::new(RejectClassCmd));
    registry.register(Box::new(FreezeClassCmd));
    registry.register(Box::new(AskCmd));
}

fn is_aadt_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("aadt"))
        .unwrap_or(false)
}

fn resolve_drawing_path(ctx: &Context, args_path: Option<&str>) -> Result<String, String> {
    if let Some(p) = args_path {
        return Ok(p.to_string());
    }
    ctx.drawing_path
        .clone()
        .ok_or_else(|| "no drawing path — OPEN/SAVE a .aadt file first".into())
}

fn table_to_run_data(
    table: &TakeoffTable,
    doc_revision: u64,
    published: bool,
) -> aadt_store::TakeoffRunData {
    aadt_store::TakeoffRunData {
        doc_revision,
        checkpoint_id: None,
        schema_ver: TAKEOFF_SCHEMA_VERSION.to_string(),
        totals_json: format!(
            "{{\"wall_length\":{:.6},\"wall_net_area\":{:.6},\"room_area\":{:.6}}}",
            table.total_wall_length(),
            table.total_wall_net_area(),
            table.total_room_area()
        ),
        published,
        walls: table
            .walls
            .iter()
            .map(|w| aadt_store::TakeoffWallRow {
                wall_id: w.id as i64,
                layer: w.layer.clone(),
                length: w.length,
                thickness: w.thickness,
                height: w.height,
                gross_area: w.gross_area,
                volume: w.volume,
                cost_code: w.cost_code.clone(),
                wall_type: w.wall_type.clone(),
                class_status: w.class_status.clone(),
            })
            .collect(),
        rooms: table
            .rooms
            .iter()
            .map(|r| aadt_store::TakeoffRoomRow {
                room_id: r.id as i64,
                layer: r.layer.clone(),
                area: r.area,
                perimeter: r.perimeter,
                cost_code: r.cost_code.clone(),
            })
            .collect(),
        openings: table
            .openings
            .iter()
            .map(|o| aadt_store::TakeoffOpeningRow {
                number: o.number as i64,
                kind: o.kind.clone(),
                width: o.width,
                height: o.height,
                area: o.area,
                cost_code: o.cost_code.clone(),
            })
            .collect(),
        structural: table
            .structural
            .iter()
            .map(|s| aadt_store::TakeoffStructuralRow {
                number: s.number as i64,
                kind: s.kind.clone(),
                cost_code: s.cost_code.clone(),
            })
            .collect(),
    }
}

fn persist_takeoff_run(
    path: &str,
    table: &TakeoffTable,
    doc_revision: u64,
    published: bool,
) -> Result<(i64, Option<String>), String> {
    if !is_aadt_path(path) {
        return Err("takeoff persistence requires a .aadt drawing path".into());
    }
    let prev_id = aadt_store::latest_takeoff_run_id(path).map_err(|e| e.to_string())?;
    let data = table_to_run_data(table, doc_revision, published);
    let run_id = aadt_store::append_takeoff_run(path, &data).map_err(|e| e.to_string())?;
    let mut delta = None;
    if let Some(from) = prev_id {
        if let Ok(Some(prev)) = aadt_store::load_takeoff_run(path, from) {
            // Rebuild a minimal previous table for delta from stored totals only if needed —
            // compare via current vs previous run using store rows → temporary tables.
            let prev_table = run_data_to_table(&prev);
            let summary = takeoff_delta_summary(&prev_table, table);
            let _ = aadt_store::append_takeoff_delta(path, from, run_id, &summary);
            delta = Some(summary);
        }
    }
    Ok((run_id, delta))
}

fn run_data_to_table(data: &aadt_store::TakeoffRunData) -> TakeoffTable {
    TakeoffTable {
        walls: data
            .walls
            .iter()
            .map(|w| aadt_takeoff::WallTakeoff {
                id: w.wall_id as usize,
                layer: w.layer.clone(),
                length: w.length,
                thickness: w.thickness,
                height: w.height,
                gross_area: w.gross_area,
                volume: w.volume,
                cost_code: w.cost_code.clone(),
                wall_type: w.wall_type.clone(),
                class_status: w.class_status.clone(),
            })
            .collect(),
        openings: data
            .openings
            .iter()
            .map(|o| aadt_takeoff::OpeningTakeoff {
                number: o.number as u32,
                kind: o.kind.clone(),
                width: o.width,
                height: o.height,
                area: o.area,
                cost_code: o.cost_code.clone(),
            })
            .collect(),
        rooms: data
            .rooms
            .iter()
            .map(|r| aadt_takeoff::RoomTakeoff {
                id: r.room_id as usize,
                layer: r.layer.clone(),
                area: r.area,
                perimeter: r.perimeter,
                cost_code: r.cost_code.clone(),
            })
            .collect(),
        structural: data
            .structural
            .iter()
            .map(|s| aadt_takeoff::StructuralTakeoff {
                number: s.number as u32,
                kind: s.kind.clone(),
                cost_code: s.cost_code.clone(),
            })
            .collect(),
    }
}

fn compute_takeoff(ctx: &aadt_command::Context) -> TakeoffTable {
    let detection = detect(&ctx.doc, &DetectionConfig::default());
    let mut table = TakeoffTable::from_detection(&detection);
    table.attach_wall_classifications(&ctx.doc, &detection);
    table
}

fn takeoff_live_enabled(ctx: &Context) -> bool {
    let v = ctx.doc.properties().takeoff_live.trim();
    v.is_empty() || v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("on")
}

/// Join command args into a single filesystem path (paths often contain spaces).
fn path_from_args(args: &[String], cmd: &str) -> Result<String, String> {
    let raw = args.join(" ");
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("{cmd}: missing path"));
    }
    let path = if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    Ok(path.to_string())
}

fn open_document(path: &str) -> Result<(aadt_model::Document, Vec<String>), String> {
    let p = Path::new(path);
    match p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("aadt") => aadt_store::load_path(p)
            .map(|doc| (doc, Vec::new()))
            .map_err(|e| e.to_string()),
        Some("dwg") => aadt_dxf::import_path_report(p)
            .map(|r| (r.document, r.warnings))
            .map_err(|e| e.to_string()),
        _ => aadt_dxf::import_path_report(p)
            .map(|r| (r.document, r.warnings))
            .map_err(|e| e.to_string()),
    }
}

fn save_document(doc: &aadt_model::Document, path: &str) -> Result<(), String> {
    let p = Path::new(path);
    match p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("aadt") => aadt_store::save_path(doc, p).map_err(|e| e.to_string()),
        Some("dwg") => aadt_dxf::export_dwg_path(doc, p).map_err(|e| e.to_string()),
        _ => aadt_dxf::export_path(doc, p).map_err(|e| e.to_string()),
    }
}

struct OpenCmd;
impl Command for OpenCmd {
    fn name(&self) -> &str {
        "OPEN"
    }
    fn help(&self) -> &str {
        "OPEN path.dxf|path.dwg|path.aadt — replace the document with an imported drawing"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let path = path_from_args(args, "OPEN")?;
        let (mut doc, warnings) = open_document(&path)?;
        // Preserve CAD layers under ORG-DWG_LAYER-* before seeding AIA standards.
        doc.retag_imported_layers();
        ctx.doc = doc;
        ctx.drawing_path = Some(path.clone());
        let seeded = apply_standard_layers(&mut ctx.doc);
        let _ = apply_layer_set(&mut ctx.doc, "IMPORT");
        ctx.doc.assign_imported_by_layer();
        let mut msg = format!(
            "opened {path} ({} entities; seeded {seeded} AIA layers; floors=Ground; import layers ORG-DWG_LAYER-* / IMPORT)",
            ctx.doc.len()
        );
        if !warnings.is_empty() {
            msg.push_str("; warning: ");
            msg.push_str(&warnings.join("; "));
        }
        Ok(msg)
    }
}

struct SaveCmd;
impl Command for SaveCmd {
    fn name(&self) -> &str {
        "SAVE"
    }
    fn help(&self) -> &str {
        "SAVE path.dxf|path.dwg|path.aadt — write the document (DXF, DWG subset, or native .aadt)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let path = path_from_args(args, "SAVE")?;
        save_document(&ctx.doc, &path)?;
        ctx.drawing_path = Some(path.clone());
        let mut msg = format!("saved {} entities to {path}", ctx.doc.len());
        if is_aadt_path(&path) && takeoff_live_enabled(ctx) {
            let table = compute_takeoff(ctx);
            match persist_takeoff_run(&path, &table, ctx.doc.revision(), false) {
                Ok((run_id, delta)) => {
                    msg.push_str(&format!("; takeoff run {run_id}"));
                    if let Some(d) = delta {
                        msg.push_str(&format!("; Δ {d}"));
                    }
                }
                Err(e) => msg.push_str(&format!("; takeoff persist skipped ({e})")),
            }
        }
        Ok(msg)
    }
}

struct LayerSetCmd;
impl Command for LayerSetCmd {
    fn name(&self) -> &str {
        "LAYERSET"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["LSET", "AADT-LAYERSET"]
    }
    fn help(&self) -> &str {
        "LAYERSET [name] — apply CONSTRUCTION/SERVICES/ELECTRICAL/PRESENTATION (list if omitted)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        match args.first() {
            None => {
                let names: Vec<_> = LayerSet::names().collect();
                Ok(format!("layer sets: {}", names.join(", ")))
            }
            Some(name) => {
                let msg = ctx.doc.transact("LAYERSET", |d| apply_layer_set(d, name))?;
                Ok(msg)
            }
        }
    }
}

struct StandardsCmd;
impl Command for StandardsCmd {
    fn name(&self) -> &str {
        "STANDARDS"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["AIA", "AADT-STANDARDS"]
    }
    fn help(&self) -> &str {
        "STANDARDS — seed missing AIA/NCS layers into the drawing"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let n = ctx.doc.transact("STANDARDS", apply_standard_layers);
        Ok(format!("seeded {n} AIA layers"))
    }
}

struct SectionCmd;
impl Command for SectionCmd {
    fn name(&self) -> &str {
        "SECTION"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["AADT-SECTION"]
    }
    fn help(&self) -> &str {
        "SECTION x1 y1 x2 y2 [height] — generate a 2D section from the plan cut line"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let x1: f64 = args
            .first()
            .ok_or("SECTION: missing x1")?
            .parse()
            .map_err(|_| "SECTION: bad x1")?;
        let y1: f64 = args
            .get(1)
            .ok_or("SECTION: missing y1")?
            .parse()
            .map_err(|_| "SECTION: bad y1")?;
        let x2: f64 = args
            .get(2)
            .ok_or("SECTION: missing x2")?
            .parse()
            .map_err(|_| "SECTION: bad x2")?;
        let y2: f64 = args
            .get(3)
            .ok_or("SECTION: missing y2")?
            .parse()
            .map_err(|_| "SECTION: bad y2")?;
        let height: f64 = args
            .get(4)
            .map(|s| s.parse().map_err(|_| format!("SECTION: bad height {s}")))
            .transpose()?
            .unwrap_or(3.0);
        let n = ctx.doc.transact("SECTION", |d| {
            generate_section(d, Point2::new(x1, y1), Point2::new(x2, y2), height)
        })?;
        Ok(format!("SECTION added {n} entities on A-SECT*"))
    }
}

struct BlockLibCmd;
impl Command for BlockLibCmd {
    fn name(&self) -> &str {
        "BLOCKLIB"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["BLIB"]
    }
    fn help(&self) -> &str {
        "BLOCKLIB [list|import path [name]|load-all] — DXF/SVG library (opensource inches→m)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let root = aadt_library::default_library_root();
        match args.first().map(|s| s.to_ascii_lowercase()).as_deref() {
            None | Some("list") => {
                let blocks = aadt_library::list_blocks(&root).map_err(|e| e.to_string())?;
                if blocks.is_empty() {
                    return Ok(format!("no blocks in {}", root.join("blocks").display()));
                }
                let lines: Vec<_> = blocks
                    .iter()
                    .map(|b| format!("  {} ({})", b.name, b.path.display()))
                    .collect();
                Ok(format!("{} block(s):\n{}", blocks.len(), lines.join("\n")))
            }
            Some("import") => {
                let path = args.get(1).ok_or("BLOCKLIB import: missing path")?;
                let name = args.get(2).map(String::as_str);
                let imported = ctx.doc.transact("BLOCKLIB", |d| {
                    aadt_library::import_block(d, Path::new(path), name).map_err(|e| e.to_string())
                })?;
                Ok(format!("imported block {imported:?}"))
            }
            Some("load-all") | Some("loadall") | Some("import-all") => {
                let names = ctx.doc.transact("BLOCKLIB", |d| {
                    aadt_library::import_all_blocks(d, &root).map_err(|e| e.to_string())
                })?;
                Ok(format!(
                    "loaded {} library block(s) (opensource scaled inches→metres)",
                    names.len()
                ))
            }
            Some(other) => Err(format!(
                "BLOCKLIB: unknown subcommand {other:?}; try list, import, or load-all"
            )),
        }
    }
}

struct HatchLibCmd;
impl Command for HatchLibCmd {
    fn name(&self) -> &str {
        "HATCHLIB"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["HLIB"]
    }
    fn help(&self) -> &str {
        "HATCHLIB — list hatch patterns from the library"
    }
    fn run(&self, _ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let root = aadt_library::default_library_root();
        let pats = aadt_library::list_hatch_patterns(&root).map_err(|e| e.to_string())?;
        let lines: Vec<_> = pats
            .iter()
            .map(|p| format!("  {} — {}", p.name, p.description))
            .collect();
        Ok(format!("{} pattern(s):\n{}", pats.len(), lines.join("\n")))
    }
}

struct ScheduleCmd;
impl Command for ScheduleCmd {
    fn name(&self) -> &str {
        "SCHEDULE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["AADT-SCHEDULE"]
    }
    fn help(&self) -> &str {
        "SCHEDULE [x y]|REFRESH — place takeoff table, or refresh linked schedules"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if args
            .first()
            .map(|a| a.eq_ignore_ascii_case("REFRESH"))
            .unwrap_or(false)
        {
            let table = compute_takeoff(ctx);
            let run = ctx
                .drawing_path
                .as_deref()
                .filter(|p| is_aadt_path(p))
                .and_then(|p| aadt_store::latest_takeoff_run_id(p).ok().flatten())
                .map(|id| id.to_string())
                .unwrap_or_default();
            let n = table.refresh_schedules(&mut ctx.doc, &run)?;
            return Ok(format!("refreshed linked schedule ({n} entities)"));
        }
        let origin = match args {
            [] => Point2::new(0.0, 0.0),
            [x, y] => Point2::new(
                x.parse::<f64>()
                    .map_err(|_| format!("SCHEDULE: bad x {x}"))?,
                y.parse::<f64>()
                    .map_err(|_| format!("SCHEDULE: bad y {y}"))?,
            ),
            _ => return Err("SCHEDULE: expected no arguments, 'x y', or REFRESH".to_string()),
        };

        let table = compute_takeoff(ctx);
        let run = ctx
            .drawing_path
            .as_deref()
            .filter(|p| is_aadt_path(p))
            .and_then(|p| aadt_store::latest_takeoff_run_id(p).ok().flatten())
            .map(|id| id.to_string())
            .unwrap_or_default();
        let style = ScheduleStyle {
            origin,
            ..Default::default()
        };
        let ids = table.insert_schedule_with_run(&mut ctx.doc, &style, &run);
        Ok(format!(
            "placed schedule at {},{} ({} entities; UNDO removes it)",
            origin.x,
            origin.y,
            ids.len()
        ))
    }
}

struct TakeoffCmd;
impl Command for TakeoffCmd {
    fn name(&self) -> &str {
        "TAKEOFF"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["AADT-TAKEOFF"]
    }
    fn help(&self) -> &str {
        "TAKEOFF [out.csv]|PUBLISH [sink] — detect/compute quantities; PUBLISH pins + sink"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if args
            .first()
            .map(|a| a.eq_ignore_ascii_case("PUBLISH"))
            .unwrap_or(false)
        {
            return takeoff_publish(ctx, args.get(1).map(|s| s.as_str()));
        }
        let table = compute_takeoff(ctx);
        if let Some(path) = args.first() {
            if !path.eq_ignore_ascii_case("PUBLISH") {
                std::fs::write(path, table.to_csv()).map_err(|e| format!("writing {path}: {e}"))?;
            }
        }
        let mut msg = format!(
            "{} walls, {} rooms; wall length {:.3}, room area {:.3}",
            table.walls.len(),
            table.rooms.len(),
            table.total_wall_length(),
            table.total_room_area(),
        );
        if let Some(path) = ctx.drawing_path.clone() {
            if is_aadt_path(&path) && takeoff_live_enabled(ctx) {
                match persist_takeoff_run(&path, &table, ctx.doc.revision(), false) {
                    Ok((run_id, delta)) => {
                        msg.push_str(&format!("; run {run_id}"));
                        if let Some(d) = delta {
                            msg.push_str(&format!("; Δ {d}"));
                        }
                    }
                    Err(e) => msg.push_str(&format!("; persist skipped ({e})")),
                }
            }
        }
        Ok(msg)
    }
}

fn takeoff_publish(ctx: &mut Context, sink_spec: Option<&str>) -> Result<String, String> {
    let path = resolve_drawing_path(ctx, None)?;
    if !is_aadt_path(&path) {
        return Err("TAKEOFF PUBLISH requires a .aadt drawing".into());
    }
    let table = compute_takeoff(ctx);
    let (run_id, delta) = persist_takeoff_run(&path, &table, ctx.doc.revision(), true)?;
    aadt_store::mark_takeoff_published(&path, run_id).map_err(|e| e.to_string())?;
    let mut msg = format!("published takeoff run {run_id}");
    if let Some(d) = delta {
        msg.push_str(&format!("; Δ {d}"));
    }
    let spec = sink_spec
        .map(|s| s.to_string())
        .or_else(|| std::env::var("AADT_TAKEOFF_SINK").ok())
        .unwrap_or_else(|| "none".into());
    let mut sink = sink_from_spec(&spec)?;
    let props = ctx.doc.properties();
    let drawing_id = if props.drawing_number.is_empty() {
        path.clone()
    } else {
        props.drawing_number.clone()
    };
    let meta = TakeoffPushMeta {
        project_id: props.project_number.clone(),
        drawing_id,
        run_id: run_id.to_string(),
        doc_revision: ctx.doc.revision(),
        title_block_revision: props.revision.clone(),
        checkpoint_label: String::new(),
    };
    sink.push(&table, &meta)?;
    if spec != "none" {
        msg.push_str(&format!("; sink {spec}"));
    }
    Ok(msg)
}

struct HistoryCmd;
impl Command for HistoryCmd {
    fn name(&self) -> &str {
        "HISTORY"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["HIST", "VERSIONS"]
    }
    fn help(&self) -> &str {
        "HISTORY [LIST]|SAVE [label]|RESTORE id|PURGE — drawing time machine (.aadt)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let sub = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_else(|| "LIST".into());
        match sub.as_str() {
            "LIST" | "" => {
                let path = resolve_drawing_path(ctx, args.get(1).map(|s| s.as_str()))?;
                if !is_aadt_path(&path) {
                    return Err("HISTORY: .aadt file required".into());
                }
                let list = aadt_store::list_checkpoints(&path).map_err(|e| e.to_string())?;
                if list.is_empty() {
                    return Ok("no checkpoints yet (SAVE the drawing)".into());
                }
                let lines: Vec<_> = list
                    .iter()
                    .take(30)
                    .map(|c| {
                        format!(
                            "  #{} [{}] {} — {} (rev {}, {})",
                            c.id, c.kind, c.label, c.change_summary, c.doc_revision, c.created_at
                        )
                    })
                    .collect();
                Ok(format!(
                    "{} checkpoint(s):\n{}",
                    list.len(),
                    lines.join("\n")
                ))
            }
            "SAVE" => {
                let path = resolve_drawing_path(ctx, None)?;
                if !is_aadt_path(&path) {
                    return Err("HISTORY SAVE: .aadt file required".into());
                }
                let label = args
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| "Named version".into());
                let note = args.get(2).cloned().unwrap_or_default();
                let id = aadt_store::save_named_checkpoint(&path, &ctx.doc, &label, &note, "")
                    .map_err(|e| e.to_string())?;
                Ok(format!("saved named checkpoint #{id} ({label})"))
            }
            "RESTORE" => {
                let id: i64 = args
                    .get(1)
                    .ok_or("HISTORY RESTORE: missing checkpoint id")?
                    .parse()
                    .map_err(|_| "HISTORY RESTORE: id must be an integer")?;
                let path = resolve_drawing_path(ctx, None)?;
                if !is_aadt_path(&path) {
                    return Err("HISTORY RESTORE: .aadt file required".into());
                }
                let restored =
                    aadt_store::restore_checkpoint(&path, id).map_err(|e| e.to_string())?;
                let n = restored.len();
                ctx.doc = restored;
                // Write restore point back into the file.
                let _ = aadt_store::save_path_with_checkpoint(
                    &ctx.doc,
                    &path,
                    Some((&format!("Restored from #{id}"), "", "")),
                );
                Ok(format!(
                    "restored checkpoint #{id} ({n} entities); saved restore point"
                ))
            }
            "PURGE" => {
                let path = resolve_drawing_path(ctx, None)?;
                let n = aadt_store::purge_auto_checkpoints(&path).map_err(|e| e.to_string())?;
                Ok(format!("purged {n} auto checkpoint(s)"))
            }
            other => Err(format!(
                "HISTORY: unknown subcommand {other:?} (LIST|SAVE|RESTORE|PURGE)"
            )),
        }
    }
}

fn parse_entity_targets(
    ctx: &Context,
    args: &[String],
) -> Result<Vec<aadt_model::EntityId>, String> {
    if args.is_empty() {
        return Ok(ctx.selection.clone());
    }
    let token = args[0].as_str();
    match token.to_ascii_uppercase().as_str() {
        "SELECTION" => Ok(ctx.selection.clone()),
        "ALL" => Ok(ctx.doc.entities().into_iter().map(|e| e.id).collect()),
        _ => {
            let mut ids = Vec::new();
            for a in args {
                let n: u64 = a.parse().map_err(|_| format!("bad entity id {a:?}"))?;
                ids.push(aadt_model::EntityId(n));
            }
            Ok(ids)
        }
    }
}

struct RecognizeCmd;
impl Command for RecognizeCmd {
    fn name(&self) -> &str {
        "RECOGNIZE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["INFER", "AI-RECOGNIZE"]
    }
    fn help(&self) -> &str {
        "RECOGNIZE — AI wall-type propose (skips frozen; does not freeze)"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let det = detect(&ctx.doc, &DetectionConfig::default());
        let client = aadt_ai::make_client(aadt_ai::AiBackend::FoundryLocal, "");
        let mut audit = aadt_ai::AuditLog::new();
        let assignments = match aadt_ai::infer_wall_types_open(
            client.as_ref(),
            &mut audit,
            &det,
            &ctx.doc,
        ) {
            Ok(a) => a,
            Err(e) => {
                return Ok(format!(
                    "AI recognition skipped: {e}. Drawing unchanged (deterministic path does not need AI)."
                ));
            }
        };
        let n = aadt_ai::propose_wall_types(
            &mut ctx.doc,
            &det,
            &assignments,
            aadt_ai::DEFAULT_AUTO_ACCEPT_THRESHOLD,
        );
        let counts = aadt_ai::status_counts(&ctx.doc);
        let review = aadt_ai::list_for_review(&ctx.doc, aadt_ai::DEFAULT_AUTO_ACCEPT_THRESHOLD);
        Ok(format!(
            "proposed {n} wall(s); status proposed={} accepted={} frozen={} rejected={}; {} need review",
            counts[0].1,
            counts[1].1,
            counts[2].1,
            counts[3].1,
            review.len()
        ))
    }
}

struct ReviewCmd;
impl Command for ReviewCmd {
    fn name(&self) -> &str {
        "REVIEW"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["AI-REVIEW"]
    }
    fn help(&self) -> &str {
        "REVIEW — list proposed / low-confidence classifications"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let items = aadt_ai::list_for_review(&ctx.doc, aadt_ai::DEFAULT_AUTO_ACCEPT_THRESHOLD);
        if items.is_empty() {
            return Ok("review: nothing pending".into());
        }
        let mut lines = vec![format!("review: {} object(s)", items.len())];
        for it in items {
            lines.push(format!(
                "  id={} type={} status={} conf={:.2} src={} rev={}",
                it.entity_id.0,
                it.wall_type,
                it.status.as_token(),
                it.confidence,
                it.source,
                it.revision
            ));
        }
        Ok(lines.join("\n"))
    }
}

struct AcceptClassCmd;
impl Command for AcceptClassCmd {
    fn name(&self) -> &str {
        "ACCEPT"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["AI-ACCEPT"]
    }
    fn help(&self) -> &str {
        "ACCEPT [SELECTION|id...] [type] — accept classifications (optional type override)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let (ids, override_ty) = if args.is_empty() {
            (ctx.selection.clone(), None)
        } else if args.len() == 1
            && aadt_ai::WallType::parse(&args[0]).is_some()
            && !args[0].chars().all(|c| c.is_ascii_digit())
        {
            (ctx.selection.clone(), Some(args[0].as_str()))
        } else {
            let mut end = args.len();
            let mut ov = None;
            if let Some(last) = args.last() {
                if aadt_ai::WallType::parse(last).is_some() && last.parse::<u64>().is_err() {
                    ov = Some(last.as_str());
                    end = args.len() - 1;
                }
            }
            (parse_entity_targets(ctx, &args[..end])?, ov)
        };
        if ids.is_empty() {
            return Err("ACCEPT: no targets (SELECT first or pass ids)".into());
        }
        let n = aadt_ai::accept_classifications(&mut ctx.doc, &ids, override_ty);
        Ok(format!("accepted {n} classification(s)"))
    }
}

struct RejectClassCmd;
impl Command for RejectClassCmd {
    fn name(&self) -> &str {
        "REJECT"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["AI-REJECT"]
    }
    fn help(&self) -> &str {
        "REJECT [SELECTION|id...] — reject proposed classifications"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let ids = if args.is_empty() {
            ctx.selection.clone()
        } else {
            parse_entity_targets(ctx, args)?
        };
        if ids.is_empty() {
            return Err("REJECT: no targets".into());
        }
        let n = aadt_ai::reject_classifications(&mut ctx.doc, &ids);
        Ok(format!("rejected {n} classification(s)"))
    }
}

struct FreezeClassCmd;
impl Command for FreezeClassCmd {
    fn name(&self) -> &str {
        "FREEZE"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["AI-FREEZE"]
    }
    fn help(&self) -> &str {
        "FREEZE [SELECTION|id...] — freeze accepted (empty = all accepted)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let ids = if args.is_empty() {
            Vec::new()
        } else {
            parse_entity_targets(ctx, args)?
        };
        let n = aadt_ai::freeze_classifications(&mut ctx.doc, &ids);
        Ok(format!("frozen {n} classification(s)"))
    }
}

/// Natural-language drafting via Foundry Local (or local rule parser for simple phrases).
struct AskCmd;
impl Command for AskCmd {
    fn name(&self) -> &str {
        "ASK"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["NL", "SAY", "AI"]
    }
    fn help(&self) -> &str {
        "ASK <plain English> — e.g. create a room of 12' x 16' · add a window of 5' to the selected wall"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if args.is_empty() {
            return Err(
                "ASK needs a phrase — try: ASK create a room of 12' x 16'".into(),
            );
        }
        let utterance = args.join(" ");
        let nl_ctx = nl_context_from(ctx);
        let client = aadt_ai::make_client(aadt_ai::AiBackend::FoundryLocal, "");
        let mut audit = aadt_ai::AuditLog::new();
        let plan = match aadt_ai::interpret_nl(client.as_ref(), &mut audit, &utterance, &nl_ctx) {
            Ok(p) => p,
            Err(e) => {
                return Ok(format!(
                    "ASK skipped: {e}. Tip: start Foundry Local (phi-3.5-mini), or use a known phrase like \"create a room of 12' x 16'\"."
                ));
            }
        };
        // Seed door/window blocks before INSERT from NL.
        aadt_command::ensure_place_blocks(&mut ctx.doc);
        let interp = Interpreter::with_builtins();
        let mut lines = vec![format!(
            "ASK ({:?}) — {}",
            match plan.source {
                aadt_ai::NlSource::Local => "local",
                aadt_ai::NlSource::Model => "foundry",
            },
            plan.note
        )];
        for cmd in &plan.commands {
            match interp.eval_line(ctx, cmd) {
                Ok(Some(m)) => lines.push(format!("→ {cmd}  ·  {m}")),
                Ok(None) => lines.push(format!("→ {cmd}")),
                Err(e) => lines.push(format!("→ {cmd}  ·  error: {e}")),
            }
        }
        Ok(lines.join("\n"))
    }
}

fn nl_context_from(ctx: &Context) -> aadt_ai::NlContext {
    use aadt_model::Geometry;
    let units = ctx.doc.properties().units.clone();
    let o = ctx.doc.reference_origin();
    let mut selection = Vec::new();
    for id in &ctx.selection {
        let Some(e) = ctx.doc.get(*id) else {
            continue;
        };
        let (kind, a, b) = match &e.geometry {
            Geometry::Line(s) => (
                "LINE".into(),
                Some((s.a.x, s.a.y)),
                Some((s.b.x, s.b.y)),
            ),
            Geometry::Polyline(_) => ("POLYLINE".into(), None, None),
            Geometry::Circle(_) => ("CIRCLE".into(), None, None),
            Geometry::Arc(_) => ("ARC".into(), None, None),
            Geometry::Text(_) => ("TEXT".into(), None, None),
            Geometry::Insert(_) => ("INSERT".into(), None, None),
        };
        selection.push(aadt_ai::NlSelectionFact {
            id: id.0,
            kind,
            layer: e.layer.clone(),
            a,
            b,
        });
    }
    aadt_ai::NlContext {
        units,
        origin_x: o.x,
        origin_y: o.y,
        selection,
    }
}
