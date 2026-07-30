//! CLI-registered commands that bridge the substrate command interpreter to the
//! file-I/O and AADT-engine layers.
//!
//! These live in the CLI (not `aadt-command`) so the substrate command core stays
//! free of engine dependencies: `OPEN`/`SAVE` use `aadt-dxf`/`aadt-store`, and
//! `SECTION`/`STANDARDS` use `aadt-detect`/`aadt-standards`. They are registered
//! onto an interpreter that already carries the generic drawing built-ins.

use aadt_command::{Command, Context, Interpreter};
use aadt_embed::{default_classifier, Classifier, ElementClass, EMBED_DIM};
use aadt_geometry::Point2;
use aadt_model::EntityId;
use aadt_standards::{apply_layer_set, apply_standard_layers, generate_section, LayerSet};
use aadt_vecdb::{accept, map_metadata, set_class, set_layer, Thresholds, VectorDb};
use std::cell::RefCell;
use std::path::Path;

/// Register the file-I/O and engine commands onto an interpreter.
pub fn register(interp: &mut Interpreter) {
    let registry = interp.registry_mut();
    registry.register(Box::new(OpenCmd));
    registry.register(Box::new(SaveCmd));
    registry.register(Box::new(HistoryCmd));
    registry.register(Box::new(MapMetaCmd::new()));
    registry.register(Box::new(LayerSetCmd));
    registry.register(Box::new(StandardsCmd));
    registry.register(Box::new(SectionCmd));
    registry.register(Box::new(BlockLibCmd));
    registry.register(Box::new(HatchLibCmd));
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
        let msg = format!("saved {} entities to {path}", ctx.doc.len());
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

/// `MAPMETA` — embeddings-vector-db metadata mapping. Classifies every element
/// from its geometry (local ML model), auto-assigns confident predictions to the
/// AIA standard layer + model-space category, and queues low-confidence ones for
/// reconciliation. Vectors + statuses persist in the `.aadt` file.
struct MapState {
    db: VectorDb,
    path: Option<String>,
}

struct MapMetaCmd {
    st: RefCell<Option<MapState>>,
}

impl MapMetaCmd {
    fn new() -> Self {
        Self {
            st: RefCell::new(None),
        }
    }

    /// Load the vector DB from the current `.aadt` path (or start fresh) whenever
    /// the drawing path changes, so state persists across commands in a session.
    fn sync(&self, ctx: &Context) {
        let path = ctx.drawing_path.clone();
        let mut g = self.st.borrow_mut();
        let reload = match &*g {
            None => true,
            Some(s) => s.path != path,
        };
        if reload {
            let model_id = default_classifier().model_id().to_string();
            let db = path
                .as_ref()
                .filter(|p| is_aadt_path(p))
                .and_then(|p| aadt_store::read_vecdb_path(p).ok().flatten())
                .filter(|db| db.model_id() == model_id)
                .unwrap_or_else(|| VectorDb::new(model_id, EMBED_DIM));
            *g = Some(MapState { db, path });
        }
    }

    /// Persist the in-memory DB back into the `.aadt` file, if there is one.
    fn persist(&self, ctx: &Context) {
        if let Some(p) = ctx.drawing_path.as_ref().filter(|p| is_aadt_path(p)) {
            if let Some(s) = self.st.borrow().as_ref() {
                let _ = aadt_store::write_vecdb_path(p, &s.db);
            }
        }
    }

    fn do_map(&self, ctx: &mut Context) -> Result<String, String> {
        let report = {
            let clf = default_classifier();
            let mut g = self.st.borrow_mut();
            let state = g.as_mut().expect("state synced");
            map_metadata(&mut ctx.doc, &mut state.db, &clf, Thresholds::default())
        };
        ctx.selection = report.pending_ids.clone();
        self.persist(ctx);
        let tail = if report.pending > 0 {
            format!(
                " — reconcile with MAPMETA RECONCILE (ids: {})",
                ids_str(&report.pending_ids)
            )
        } else {
            String::new()
        };
        let reapplied = if report.reapplied > 0 {
            format!(" · {} manual re-applied", report.reapplied)
        } else {
            String::new()
        };
        Ok(format!(
            "MAPMETA: {} classified · {} auto-assigned · {} pending{reapplied}{tail}",
            report.total, report.auto_assigned, report.pending
        ))
    }

    fn do_reconcile(&self, ctx: &mut Context) -> Result<String, String> {
        let clf = default_classifier();
        let g = self.st.borrow();
        let state = g.as_ref().expect("state synced");
        let pend = state.db.pending();
        if pend.is_empty() {
            return Ok("MAPMETA: nothing pending — every element is classified".into());
        }
        let mut ids = Vec::new();
        let mut lines = Vec::new();
        for e in &pend {
            ids.push(e.id);
            let cands = ctx
                .doc
                .get(e.id)
                .map(|ent| clf.classify(ent, &ctx.doc).candidates)
                .unwrap_or_default();
            let top: Vec<String> = cands
                .iter()
                .take(3)
                .map(|(c, p)| format!("{} {:.0}%", c.label(), p * 100.0))
                .collect();
            lines.push(format!(
                "  #{} (conf {:.0}%) → {} | candidates: {}",
                e.id.0,
                e.confidence * 100.0,
                e.layer,
                top.join(", ")
            ));
        }
        drop(g);
        ctx.selection = ids;
        Ok(format!(
            "{} pending — MAPMETA ACCEPT <id> or MAPMETA SET <id> <class|layer>:\n{}",
            lines.len(),
            lines.join("\n")
        ))
    }

    fn do_accept(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_id(args, 1, "MAPMETA ACCEPT")?;
        let ok = {
            let mut g = self.st.borrow_mut();
            let state = g.as_mut().expect("state synced");
            accept(&mut ctx.doc, &mut state.db, EntityId(id))
        };
        if !ok {
            return Err(format!(
                "MAPMETA ACCEPT: no entity #{id} with a prediction (run MAPMETA first)"
            ));
        }
        self.persist(ctx);
        let layer = self
            .st
            .borrow()
            .as_ref()
            .and_then(|s| s.db.get(EntityId(id)).map(|e| e.layer.clone()))
            .unwrap_or_default();
        ctx.selection = vec![EntityId(id)];
        Ok(format!("MAPMETA: accepted #{id} → {layer} (confirmed)"))
    }

    fn do_set(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_id(args, 1, "MAPMETA SET")?;
        let target = args.get(2..).map(|s| s.join(" ")).unwrap_or_default();
        let target = target.trim();
        if target.is_empty() {
            return Err("MAPMETA SET: give a class (Wall/Column/…) or a layer name".into());
        }
        let class = ElementClass::ALL
            .iter()
            .find(|c| c.label().eq_ignore_ascii_case(target))
            .copied();
        let ok = {
            let mut g = self.st.borrow_mut();
            let state = g.as_mut().expect("state synced");
            match class {
                Some(c) => set_class(&mut ctx.doc, &mut state.db, EntityId(id), c),
                None => set_layer(&mut ctx.doc, &mut state.db, EntityId(id), target),
            }
        };
        if !ok {
            return Err(format!("MAPMETA SET: no entity #{id}"));
        }
        self.persist(ctx);
        ctx.selection = vec![EntityId(id)];
        Ok(format!("MAPMETA: set #{id} → {target} (overridden)"))
    }

    fn do_info(&self, ctx: &Context, args: &[String]) -> Result<String, String> {
        let id = parse_id(args, 1, "MAPMETA INFO")?;
        let clf = default_classifier();
        let Some(ent) = ctx.doc.get(EntityId(id)) else {
            return Err(format!("MAPMETA INFO: no entity #{id}"));
        };
        let pred = clf.classify(ent, &ctx.doc);
        let cands: Vec<String> = pred
            .candidates
            .iter()
            .take(3)
            .map(|(c, p)| format!("{} {:.0}%", c.label(), p * 100.0))
            .collect();
        let similar = self
            .st
            .borrow()
            .as_ref()
            .map(|s| s.db.query(&pred.embedding, 3, Some(EntityId(id))))
            .unwrap_or_default();
        let sim: Vec<String> = similar
            .iter()
            .map(|(eid, sc)| format!("#{} ({:.2})", eid.0, sc))
            .collect();
        let cat = pred.category.map(|c| c.as_str()).unwrap_or("(layer only)");
        Ok(format!(
            "#{id}: {} → layer {} / category {} (conf {:.0}%)\n  candidates: {}\n  similar: {}",
            pred.class.label(),
            pred.layer,
            cat,
            pred.confidence * 100.0,
            cands.join(", "),
            if sim.is_empty() {
                "(run MAPMETA to populate the vector db)".into()
            } else {
                sim.join(", ")
            }
        ))
    }
}

fn parse_id(args: &[String], idx: usize, cmd: &str) -> Result<u64, String> {
    args.get(idx)
        .ok_or_else(|| format!("{cmd}: missing entity id"))?
        .parse()
        .map_err(|_| format!("{cmd}: entity id must be an integer"))
}

fn ids_str(ids: &[EntityId]) -> String {
    ids.iter()
        .map(|i| format!("#{}", i.0))
        .collect::<Vec<_>>()
        .join(", ")
}

impl Command for MapMetaCmd {
    fn name(&self) -> &str {
        "MAPMETA"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["CLASSIFY", "VECTORDB", "VDB"]
    }
    fn help(&self) -> &str {
        "MAPMETA [ALL]|RECONCILE|ACCEPT id|SET id <class|layer>|INFO id — map elements to AIA layer + category via the embeddings vector db"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        self.sync(ctx);
        let sub = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_default();
        match sub.as_str() {
            "" | "ALL" | "RUN" | "MAP" => self.do_map(ctx),
            "RECONCILE" | "PENDING" | "LIST" => self.do_reconcile(ctx),
            "ACCEPT" | "OK" => self.do_accept(ctx, args),
            "SET" | "OVERRIDE" => self.do_set(ctx, args),
            "INFO" | "WHY" => self.do_info(ctx, args),
            other => Err(format!(
                "MAPMETA: unknown subcommand {other:?} (ALL|RECONCILE|ACCEPT id|SET id <class|layer>|INFO id)"
            )),
        }
    }
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
