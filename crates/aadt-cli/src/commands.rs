//! CLI-registered commands that bridge the substrate command interpreter to the
//! file-I/O and AADT-engine layers.
//!
//! These live in the CLI (not `aadt-command`) so the substrate command core stays
//! free of engine dependencies: `OPEN`/`SAVE` use `aadt-dxf`/`aadt-store`, and
//! `SECTION`/`STANDARDS` use `aadt-detect`/`aadt-standards`. They are registered
//! onto an interpreter that already carries the generic drawing built-ins.

use aadt_assist::{
    rules::{check_as_suggestions, RuleId},
    Routed, SuggestionSet,
};
use aadt_command::{Command, Context, Interpreter};
use aadt_embed::{default_classifier, Classifier, ElementClass, EMBED_DIM};
use aadt_geometry::{Bbox, Point2};
use aadt_graph::{promote::promote_with_audit, Graph};
use aadt_model::{EntityId, Geometry, Layer, Text};
use aadt_object::{AuditLog, ObjectId, ObjectStore};
use aadt_standards::{
    apply_layer_set, apply_standard_layers, catalog_csv, catalog_markdown, find_layer,
    generate_section, LayerSet,
};
use aadt_vecdb::{
    accept, find_by_text, find_duplicates, find_similar_to, map_metadata, set_class, set_layer,
    CorpusDb, Thresholds, VectorDb,
};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Register the file-I/O and engine commands onto an interpreter.
pub fn register(interp: &mut Interpreter) {
    let registry = interp.registry_mut();
    // Shared Object-DB session state — SAVE persists it into `.vdb`, and the
    // OBJECTS / RELATIONS commands read/promote it.
    let objstate = Rc::new(RefCell::new(ObjState::default()));
    registry.register(Box::new(OpenCmd));
    registry.register(Box::new(SaveCmd {
        st: objstate.clone(),
    }));
    registry.register(Box::new(HistoryCmd));
    let mapstate: SharedMapState = Rc::new(RefCell::new(None));
    registry.register(Box::new(MapMetaCmd::new(mapstate.clone())));
    registry.register(Box::new(FindCmd { st: mapstate }));
    let corpus = Rc::new(RefCell::new(None));
    registry.register(Box::new(AskDocsCmd { st: corpus.clone() }));
    registry.register(Box::new(WithinCmd));
    let suggestions: SharedSuggestions = Rc::new(RefCell::new(SuggestState::default()));
    registry.register(Box::new(SuggestCmd {
        st: suggestions.clone(),
    }));
    registry.register(Box::new(CheckCmd {
        st: suggestions,
        corpus,
    }));
    registry.register(Box::new(AskCmd));
    registry.register(Box::new(TakeoffCmd));
    registry.register(Box::new(BoqCmd));
    registry.register(Box::new(ObjectsCmd {
        st: objstate.clone(),
    }));
    registry.register(Box::new(RelationsCmd { st: objstate }));
    registry.register(Box::new(LayerSetCmd));
    registry.register(Box::new(StandardsCmd));
    registry.register(Box::new(LayerReportCmd));
    registry.register(Box::new(LayerLegendCmd));
    registry.register(Box::new(SectionCmd));
    registry.register(Box::new(BlockLibCmd));
    registry.register(Box::new(HatchLibCmd));
    registry.register(Box::new(ServerCmd {
        st: Rc::new(RefCell::new(ServerState::default())),
    }));
}

fn is_aadt_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("aadt"))
        .unwrap_or(false)
}

fn is_vdb_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("vdb"))
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
        Some("vdb") => aadt_vdb::load_path(p)
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
        Some("vdb") => aadt_vdb::save_path(doc, p).map_err(|e| e.to_string()),
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
        "OPEN path.dxf|path.dwg|path.aadt|path.vdb — replace the document with an imported drawing"
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

struct SaveCmd {
    st: Rc<RefCell<ObjState>>,
}
impl Command for SaveCmd {
    fn name(&self) -> &str {
        "SAVE"
    }
    fn help(&self) -> &str {
        "SAVE path.dxf|path.dwg|path.aadt|path.vdb — write the document (DXF, DWG subset, native .aadt, or ShilpiDB .vdb)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let path = path_from_args(args, "SAVE")?;
        if is_vdb_path(&path) {
            // ShilpiDB is a whole-file format, so the Object DB is written with
            // the geometry here (not additively like `.aadt`). Sync first so the
            // current drawing's objects are carried forward even if OBJECTS
            // hasn't been run this session.
            let mut st = self.st.borrow_mut();
            st.sync(ctx);
            aadt_vdb::save_path_with_objects(&ctx.doc, &path, &st.objects, &st.graph, &st.audit)
                .map_err(|e| e.to_string())?;
        } else {
            save_document(&ctx.doc, &path)?;
        }
        ctx.drawing_path = Some(path.clone());
        let msg = format!("saved {} entities to {path}", ctx.doc.len());
        Ok(msg)
    }
}

/// Live connection to a `shilpid` server, shared across `SERVER` invocations.
#[derive(Default)]
struct ServerState {
    client: Option<aadt_vdb::VdbClient>,
    host: String,
}

/// Talk to a running ShilpiDB server (`shilpid`): connect, then push the
/// current drawing's entities up, pull them back, or flush the server to disk.
struct ServerCmd {
    st: Rc<RefCell<ServerState>>,
}
impl Command for ServerCmd {
    fn name(&self) -> &str {
        "SERVER"
    }
    fn help(&self) -> &str {
        "SERVER CONNECT [host:port] | STATUS | PUSH | PULL | SAVE | DISCONNECT — talk to a shilpid server (default 127.0.0.1:7420)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let sub = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .ok_or_else(|| {
                "SERVER: expected a subcommand (CONNECT|STATUS|PUSH|PULL|SAVE|DISCONNECT)"
                    .to_string()
            })?;
        let mut st = self.st.borrow_mut();
        match sub.as_str() {
            "CONNECT" => {
                let host = args
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| "127.0.0.1:7420".to_string());
                let client = aadt_vdb::VdbClient::connect(&host)
                    .map_err(|e| format!("SERVER CONNECT {host}: {e}"))?;
                st.client = Some(client);
                st.host = host.clone();
                Ok(format!("connected to shilpid at {host}"))
            }
            "DISCONNECT" => {
                let was = std::mem::take(&mut st.host);
                st.client = None;
                Ok(if was.is_empty() {
                    "not connected".to_string()
                } else {
                    format!("disconnected from {was}")
                })
            }
            "STATUS" => {
                let host = st.host.clone();
                match st.client.as_mut() {
                    Some(client) => {
                        client.ping().map_err(|e| format!("SERVER STATUS: {e}"))?;
                        Ok(format!("connected to {host} (ping ok)"))
                    }
                    None => Ok("not connected".to_string()),
                }
            }
            "PUSH" => {
                let host = st.host.clone();
                let client = st.client.as_mut().ok_or_else(|| {
                    "SERVER PUSH: not connected (run SERVER CONNECT first)".to_string()
                })?;
                let n = client
                    .push_document(&ctx.doc)
                    .map_err(|e| format!("SERVER PUSH: {e}"))?;
                Ok(format!("pushed {n} entities to {host}"))
            }
            "PULL" => {
                let host = st.host.clone();
                let client = st.client.as_mut().ok_or_else(|| {
                    "SERVER PULL: not connected (run SERVER CONNECT first)".to_string()
                })?;
                let n = client
                    .restore_into(&mut ctx.doc)
                    .map_err(|e| format!("SERVER PULL: {e}"))?;
                Ok(format!("pulled {n} entities from {host} into the drawing"))
            }
            "SAVE" => {
                let host = st.host.clone();
                let client = st
                    .client
                    .as_mut()
                    .ok_or_else(|| "SERVER SAVE: not connected".to_string())?;
                client.save().map_err(|e| format!("SERVER SAVE: {e}"))?;
                Ok(format!("asked {host} to flush its database to disk"))
            }
            other => Err(format!(
                "SERVER: unknown subcommand '{other}' (CONNECT|STATUS|PUSH|PULL|SAVE|DISCONNECT)"
            )),
        }
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

struct LayerReportCmd;
impl Command for LayerReportCmd {
    fn name(&self) -> &str {
        "LAYERREPORT"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["STANDARDSREPORT", "LAYERSTD"]
    }
    fn help(&self) -> &str {
        "LAYERREPORT path.csv|path.md — export the HCW layer standard (format from extension)"
    }
    fn run(&self, _ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let path = path_from_args(args, "LAYERREPORT")?;
        let is_md = Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
            .unwrap_or(false);
        let body = if is_md {
            catalog_markdown()
        } else {
            catalog_csv()
        };
        std::fs::write(&path, &body).map_err(|e| format!("LAYERREPORT: {path}: {e}"))?;
        let kind = if is_md { "Markdown" } else { "CSV" };
        Ok(format!("wrote {kind} layer standard to {path}"))
    }
}

struct LayerLegendCmd;
impl Command for LayerLegendCmd {
    fn name(&self) -> &str {
        "LAYERLEGEND"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["LEGEND", "LAYERKEY"]
    }
    fn help(&self) -> &str {
        "LAYERLEGEND [x y [height]] — draw a text key of the layers in use (props + metadata) on G-LEGEND"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let ox = args.first().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
        let oy = args.get(1).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
        let h = args
            .get(2)
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|v| *v > 0.0)
            .unwrap_or(250.0);
        let used = ctx.doc.layer_usage_counts();
        if used.is_empty() {
            return Err("LAYERLEGEND: no layers in use".into());
        }
        let count = used.len();
        let row = h * 1.8;
        // Swatches would need to live on the real layers (colour is ByLayer),
        // which would pollute takeoff — so the key is a text table; the ACI
        // number identifies the colour instead.
        let legend_layer = "G-LEGEND";
        ctx.doc.transact("LAYERLEGEND", |d| {
            if d.layer(legend_layer).is_none() {
                d.add_layer(
                    Layer::new(legend_layer)
                        .with_color(7)
                        .with_lineweight_mm(0.18),
                );
            }
            d.add_geometry(
                legend_layer.to_string(),
                Geometry::Text(Text::new(
                    "LAYER LEGEND".to_string(),
                    Point2::new(ox, oy + row),
                    h * 1.3,
                    0.0,
                )),
            );
            for (i, (name, n)) in used.iter().enumerate() {
                let (aci, lt, lw) = d
                    .layer(name)
                    .map(|l| (l.color, format!("{:?}", l.linetype), l.lineweight_mm))
                    .unwrap_or((7, "Continuous".to_string(), 0.25));
                let meta = find_layer(name)
                    .map(|l| format!("   [{} · {}]", l.discipline.label(), l.element.label()))
                    .unwrap_or_default();
                let text = format!("{name}   ×{n}   ACI {aci} · {lt} · {lw:.2}mm{meta}");
                let y = oy - (i as f64) * row;
                d.add_geometry(
                    legend_layer.to_string(),
                    Geometry::Text(Text::new(text, Point2::new(ox, y), h, 0.0)),
                );
            }
        });
        Ok(format!("drew legend of {count} layers on {legend_layer}"))
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

/// Shared handle to the session's embeddings vector DB, so `MAPMETA` and `FIND`
/// query the same vectors without each re-reading the `.aadt` file.
type SharedMapState = Rc<RefCell<Option<MapState>>>;

/// Load the vector DB from the current `.aadt` path (or start fresh) whenever the
/// drawing path changes, so state persists across commands in a session.
fn sync_map_state(st: &SharedMapState, ctx: &Context) {
    let path = ctx.drawing_path.clone();
    let mut g = st.borrow_mut();
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

struct MapMetaCmd {
    st: SharedMapState,
}

impl MapMetaCmd {
    fn new(st: SharedMapState) -> Self {
        Self { st }
    }

    fn sync(&self, ctx: &Context) {
        sync_map_state(&self.st, ctx);
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

/// Shared Object-DB session state for `OBJECTS` / `RELATIONS` (AI-native
/// Phase 1): the promoted objects, relationship graph, and append-only audit
/// log, synced against the drawing's `.aadt` path.
#[derive(Default)]
struct ObjState {
    objects: ObjectStore,
    graph: Graph,
    audit: AuditLog,
    path: Option<String>,
    loaded: bool,
}

impl ObjState {
    /// (Re)load persisted objects when the drawing path changes.
    fn sync(&mut self, ctx: &Context) {
        let path = ctx.drawing_path.clone();
        if self.loaded && self.path == path {
            return;
        }
        self.path = path.clone();
        self.loaded = true;
        self.objects = ObjectStore::new();
        self.graph = Graph::new();
        self.audit = AuditLog::new();
        match path.as_deref() {
            Some(p) if is_aadt_path(p) => {
                if let Ok(Some((o, g, a))) = aadt_store::read_objects_path(p) {
                    self.objects = o;
                    self.graph = g;
                    self.audit = a;
                }
            }
            // ShilpiDB carries the Object DB in its v2 sections.
            Some(p) if is_vdb_path(p) => {
                if let Ok(db) = aadt_vdb::ShilpiDb::open(p) {
                    self.objects = db.objects().clone();
                    self.graph = db.graph().clone();
                    self.audit = db.audit().clone();
                }
            }
            _ => {}
        }
    }

    /// Persist to the drawing's `.aadt` file, if there is one.
    fn persist(&self) {
        if let Some(p) = self.path.as_ref().filter(|p| is_aadt_path(p)) {
            let _ = aadt_store::write_objects_path(p, &self.objects, &self.graph, &self.audit);
        }
    }
}

fn object_line(o: &aadt_object::EngObject, graph: &Graph) -> String {
    let props: Vec<String> = o.props.iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!(
        "  #{} {} [{} entit{}, {} relation{}] {}",
        o.id.0,
        o.kind,
        o.entities.len(),
        if o.entities.len() == 1 { "y" } else { "ies" },
        graph.relations_of(o.id).len(),
        if graph.relations_of(o.id).len() == 1 {
            ""
        } else {
            "s"
        },
        props.join(" ")
    )
}

/// `OBJECTS` — promote the drawing's detected elements into the Object DB +
/// Graph DB (deterministic; audited), and inspect them.
struct ObjectsCmd {
    st: Rc<RefCell<ObjState>>,
}

impl Command for ObjectsCmd {
    fn name(&self) -> &str {
        "OBJECTS"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["OBJ", "PROMOTE"]
    }
    fn help(&self) -> &str {
        "OBJECTS [LIST|INFO id|AUDIT] — promote detection into intelligent objects + relationship graph (Object DB / Graph DB)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let mut st = self.st.borrow_mut();
        st.sync(ctx);
        let sub = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_default();
        match sub.as_str() {
            "" | "RUN" | "BUILD" => {
                let promoted = promote_with_audit(&ctx.doc, &mut st.audit);
                st.objects = promoted.objects;
                st.graph = promoted.graph;
                st.persist();
                let counts: Vec<String> = st
                    .objects
                    .kind_counts()
                    .into_iter()
                    .map(|(k, n)| format!("{n} {k}"))
                    .collect();
                Ok(format!(
                    "OBJECTS: promoted {} object(s) ({}) with {} relation(s); audit +{}",
                    st.objects.len(),
                    if counts.is_empty() {
                        "none".into()
                    } else {
                        counts.join(", ")
                    },
                    st.graph.len(),
                    st.objects.len(),
                ))
            }
            "LIST" => {
                if st.objects.is_empty() {
                    return Ok("no objects yet — run OBJECTS first".into());
                }
                let lines: Vec<String> = st
                    .objects
                    .iter()
                    .map(|o| object_line(o, &st.graph))
                    .collect();
                Ok(format!(
                    "{} object(s):\n{}",
                    st.objects.len(),
                    lines.join("\n")
                ))
            }
            "INFO" => {
                let id = ObjectId(parse_id(args, 1, "OBJECTS INFO")?);
                let Some(o) = st.objects.get(id) else {
                    return Err(format!("OBJECTS INFO: no object #{}", id.0));
                };
                let mut out = object_line(o, &st.graph);
                for e in st.graph.relations_of(id) {
                    out.push_str(&format!("\n    #{} {} #{}", e.from.0, e.rel, e.to.0));
                }
                ctx.selection = o.entities.clone();
                Ok(out)
            }
            "AUDIT" => {
                if st.audit.is_empty() {
                    return Ok("audit log is empty".into());
                }
                let lines: Vec<String> = st
                    .audit
                    .entries()
                    .iter()
                    .rev()
                    .take(20)
                    .map(|e| {
                        format!(
                            "  [{}] {} {} #{} by {}{}{}",
                            e.seq,
                            e.action,
                            e.new,
                            e.object.0,
                            e.user,
                            if e.ai_suggested { " (AI)" } else { "" },
                            if e.approved { " ✓" } else { " (pending)" },
                        )
                    })
                    .collect();
                Ok(format!(
                    "{} audit entr{} (latest first):\n{}",
                    st.audit.len(),
                    if st.audit.len() == 1 { "y" } else { "ies" },
                    lines.join("\n")
                ))
            }
            other => Err(format!(
                "OBJECTS: unknown subcommand {other:?} (LIST|INFO id|AUDIT)"
            )),
        }
    }
}

/// `RELATIONS <id>` — the typed relationship edges of one object.
struct RelationsCmd {
    st: Rc<RefCell<ObjState>>,
}

impl Command for RelationsCmd {
    fn name(&self) -> &str {
        "RELATIONS"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["REL"]
    }
    fn help(&self) -> &str {
        "RELATIONS id — typed engineering relationships of an object (Graph DB)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let mut st = self.st.borrow_mut();
        st.sync(ctx);
        let id = ObjectId(parse_id(args, 0, "RELATIONS")?);
        let Some(o) = st.objects.get(id) else {
            return Err(format!(
                "RELATIONS: no object #{} (run OBJECTS first)",
                id.0
            ));
        };
        let edges = st.graph.relations_of(id);
        ctx.selection = o.entities.clone();
        if edges.is_empty() {
            return Ok(format!("#{} {} has no relations", id.0, o.kind));
        }
        let name = |oid: ObjectId| -> String {
            st.objects
                .get(oid)
                .map(|x| format!("{} #{}", x.kind, oid.0))
                .unwrap_or_else(|| format!("#{}", oid.0))
        };
        let lines: Vec<String> = edges
            .iter()
            .map(|e| format!("  {} —{}→ {}", name(e.from), e.rel, name(e.to)))
            .collect();
        Ok(format!(
            "{} relation(s) of {} #{}:\n{}",
            edges.len(),
            o.kind,
            id.0,
            lines.join("\n")
        ))
    }
}

/// `WITHIN` — select entities in a window using ShilpiDB's spatial index (a bbox
/// query over `aadt-vdb`), instead of a linear scan.
struct WithinCmd;
impl Command for WithinCmd {
    fn name(&self) -> &str {
        "WITHIN"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["VSELECT", "VWINDOW"]
    }
    fn help(&self) -> &str {
        "WITHIN x1 y1 x2 y2 — select entities inside a window via the ShilpiDB spatial index"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let coord = |i: usize, name: &str| -> Result<f64, String> {
            args.get(i)
                .ok_or_else(|| format!("WITHIN: missing {name}"))?
                .parse::<f64>()
                .map_err(|_| format!("WITHIN: bad {name}"))
        };
        let rect = Bbox::from_corners(
            Point2::new(coord(0, "x1")?, coord(1, "y1")?),
            Point2::new(coord(2, "x2")?, coord(3, "y2")?),
        );
        // Build the ShilpiDB spatial index over the live drawing and query it.
        let index = aadt_vdb::build_index(&ctx.doc);
        let ids = aadt_vdb::query_index_bbox(&index, rect);
        ctx.selection = ids.clone();
        Ok(format!(
            "WITHIN: {} entit{} in the window (ShilpiDB index over {} total)",
            ids.len(),
            if ids.len() == 1 { "y" } else { "ies" },
            index.len()
        ))
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

/// `FIND` — semantic search over the drawing (AI-native Phase 2). Text queries
/// match against each element's label (layer name + annotation text) using the
/// character n-gram text model; `SIMILAR` and `DUPES` search the geometric
/// embedding space held in the session's vector db.
struct FindCmd {
    st: SharedMapState,
}

impl FindCmd {
    /// Default number of hits reported by a text search.
    const TOP_K: usize = 10;
    /// Cosine similarity at or above which two elements read as near-duplicates.
    const DUP_THRESHOLD: f32 = 0.995;

    fn do_text(&self, ctx: &mut Context, query: &str) -> Result<String, String> {
        let hits = find_by_text(&ctx.doc, query, Self::TOP_K);
        let hits: Vec<_> = hits.into_iter().filter(|h| h.score > 0.0).collect();
        if hits.is_empty() {
            return Ok(format!("FIND: nothing matches {query:?}"));
        }
        ctx.selection = hits.iter().map(|h| h.id).collect();
        let lines: Vec<String> = hits
            .iter()
            .map(|h| format!("  #{} {} ({:.0}%)", h.id.0, h.layer, h.score * 100.0))
            .collect();
        Ok(format!(
            "FIND {query:?}: {} match(es), selected:\n{}",
            hits.len(),
            lines.join("\n")
        ))
    }

    fn do_similar(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = EntityId(parse_id(args, 1, "FIND SIMILAR")?);
        let hits = {
            let g = self.st.borrow();
            let Some(state) = g.as_ref() else {
                return Err("FIND SIMILAR: run MAPMETA first to build the vector db".into());
            };
            if state.db.get(id).is_none() {
                return Err(format!(
                    "FIND SIMILAR: no embedding for #{} — run MAPMETA first",
                    id.0
                ));
            }
            find_similar_to(&state.db, id, Self::TOP_K)
        };
        // A non-positive cosine means actively dissimilar — not a match.
        let hits: Vec<_> = hits.into_iter().filter(|h| h.score > 0.0).collect();
        if hits.is_empty() {
            return Ok(format!("FIND SIMILAR: nothing else resembles #{}", id.0));
        }
        ctx.selection = hits.iter().map(|h| h.id).collect();
        let lines: Vec<String> = hits
            .iter()
            .map(|h| format!("  #{} {} ({:.0}%)", h.id.0, h.layer, h.score * 100.0))
            .collect();
        Ok(format!(
            "{} element(s) similar to #{}, selected:\n{}",
            hits.len(),
            id.0,
            lines.join("\n")
        ))
    }

    fn do_dupes(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let threshold = match args.get(1) {
            Some(s) => s
                .parse::<f32>()
                .map_err(|_| format!("FIND DUPES: bad threshold {s:?} (want 0.0–1.0)"))?,
            None => Self::DUP_THRESHOLD,
        };
        let pairs = {
            let g = self.st.borrow();
            let Some(state) = g.as_ref() else {
                return Err("FIND DUPES: run MAPMETA first to build the vector db".into());
            };
            find_duplicates(&state.db, threshold)
        };
        if pairs.is_empty() {
            return Ok(format!(
                "FIND DUPES: no near-duplicate geometry at ≥{:.0}% similarity",
                threshold * 100.0
            ));
        }
        let mut selection = Vec::new();
        for p in &pairs {
            if !selection.contains(&p.a) {
                selection.push(p.a);
            }
            if !selection.contains(&p.b) {
                selection.push(p.b);
            }
        }
        ctx.selection = selection;
        let lines: Vec<String> = pairs
            .iter()
            .take(20)
            .map(|p| format!("  #{} ≈ #{} ({:.1}%)", p.a.0, p.b.0, p.similarity * 100.0))
            .collect();
        Ok(format!(
            "{} duplicate pair(s) at ≥{:.0}% similarity:\n{}",
            pairs.len(),
            threshold * 100.0,
            lines.join("\n")
        ))
    }
}

impl Command for FindCmd {
    fn name(&self) -> &str {
        "FIND"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["SEARCH", "SEMSEARCH"]
    }
    fn help(&self) -> &str {
        "FIND <text>|SIMILAR id|DUPES [threshold] — semantic search over the drawing (selects the hits)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        sync_map_state(&self.st, ctx);
        let sub = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_default();
        match sub.as_str() {
            "" => Err("FIND: give a text query, or SIMILAR id / DUPES [threshold]".into()),
            "SIMILAR" | "LIKE" => self.do_similar(ctx, args),
            "DUPES" | "DUPLICATES" => self.do_dupes(ctx, args),
            // Anything else is a free-text query (may be several words).
            _ => self.do_text(ctx, args.join(" ").trim()),
        }
    }
}

/// `ASKDOCS` — retrieval over locally indexed spec/standard documents
/// (AI-native Phase 2). Text files are chunked per paragraph, embedded with the
/// character n-gram model, and searched by cosine similarity. Every answer cites
/// its source, so the retrieved passage is the evidence — nothing is generated.
struct AskDocsCmd {
    st: SharedCorpus,
}

/// Shared handle to the session's indexed corpus, so `ASKDOCS` and `CHECK`
/// (which cites passages in its findings) read the same documents.
type SharedCorpus = Rc<RefCell<Option<CorpusState>>>;

struct CorpusState {
    corpus: CorpusDb,
    path: Option<String>,
}

impl AskDocsCmd {
    /// Passages returned per query.
    const TOP_K: usize = 3;

    /// (Re)load the corpus whenever the drawing path changes.
    fn sync(&self, ctx: &Context) {
        let path = ctx.drawing_path.clone();
        let mut g = self.st.borrow_mut();
        let reload = match &*g {
            None => true,
            Some(s) => s.path != path,
        };
        if reload {
            let corpus = path
                .as_ref()
                .filter(|p| is_aadt_path(p))
                .and_then(|p| aadt_store::read_corpus_path(p).ok().flatten())
                .unwrap_or_default();
            *g = Some(CorpusState { corpus, path });
        }
    }

    /// Persist the corpus back into the drawing's `.aadt` file, if there is one.
    fn persist(&self) {
        if let Some(s) = self.st.borrow().as_ref() {
            if let Some(p) = s.path.as_ref().filter(|p| is_aadt_path(p)) {
                let _ = aadt_store::write_corpus_path(p, &s.corpus);
            }
        }
    }

    fn do_index(&self, args: &[String]) -> Result<String, String> {
        let path = path_from_args(&args[1..], "ASKDOCS INDEX")?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("ASKDOCS INDEX: cannot read {path}: {e}"))?;
        let added = {
            let mut g = self.st.borrow_mut();
            let state = g.as_mut().expect("state synced");
            state.corpus.index_text(&path, &text)
        };
        self.persist();
        if added == 0 {
            return Ok(format!("ASKDOCS: {path} has no indexable text"));
        }
        Ok(format!("ASKDOCS: indexed {added} passage(s) from {path}"))
    }

    fn do_sources(&self) -> Result<String, String> {
        let g = self.st.borrow();
        let state = g.as_ref().expect("state synced");
        let sources = state.corpus.sources();
        if sources.is_empty() {
            return Ok("ASKDOCS: no documents indexed — ASKDOCS INDEX <path>".into());
        }
        let lines: Vec<String> = sources
            .iter()
            .map(|s| {
                let n = state
                    .corpus
                    .chunks()
                    .iter()
                    .filter(|c| c.source == *s)
                    .count();
                format!("  {s} ({n} passage(s))")
            })
            .collect();
        Ok(format!(
            "{} document(s), {} passage(s):\n{}",
            sources.len(),
            state.corpus.len(),
            lines.join("\n")
        ))
    }

    fn do_ask(&self, query: &str) -> Result<String, String> {
        let g = self.st.borrow();
        let state = g.as_ref().expect("state synced");
        if state.corpus.is_empty() {
            return Err("ASKDOCS: no documents indexed — ASKDOCS INDEX <path> first".into());
        }
        let results = state.corpus.search(query, Self::TOP_K);
        if results.is_empty() {
            return Ok(format!("ASKDOCS: nothing in the corpus matches {query:?}"));
        }
        // BM25 scores are unbounded and comparable only within one result set,
        // so rank them rather than dressing them up as a percentage.
        let lines: Vec<String> = results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                format!(
                    "  {}. [{}:{}] (score {:.2})\n     {}",
                    i + 1,
                    r.source,
                    r.seq,
                    r.score,
                    r.text
                )
            })
            .collect();
        Ok(format!(
            "ASKDOCS {query:?} — {} passage(s), most relevant first:\n{}",
            results.len(),
            lines.join("\n")
        ))
    }
}

impl Command for AskDocsCmd {
    fn name(&self) -> &str {
        "ASKDOCS"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["DOCS", "RAG"]
    }
    fn help(&self) -> &str {
        "ASKDOCS <question>|INDEX path|SOURCES — retrieve cited passages from indexed spec documents"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        self.sync(ctx);
        let sub = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_default();
        match sub.as_str() {
            "" => Err("ASKDOCS: ask a question, or INDEX <path> / SOURCES".into()),
            "INDEX" | "ADD" => self.do_index(args),
            "SOURCES" | "LIST" => self.do_sources(),
            _ => self.do_ask(args.join(" ").trim()),
        }
    }
}

/// Shared session state for `SUGGEST` / `CHECK` (AI-native Phase 3): the
/// current proposal set plus the audit log that records every decision.
#[derive(Default)]
struct SuggestState {
    set: SuggestionSet,
    audit: AuditLog,
    path: Option<String>,
    loaded: bool,
}

type SharedSuggestions = Rc<RefCell<SuggestState>>;

impl SuggestState {
    /// (Re)load the persisted audit log when the drawing path changes. The
    /// proposal set itself is session-scoped — proposals are derived from the
    /// current geometry, so they are regenerated rather than restored.
    fn sync(&mut self, ctx: &Context) {
        let path = ctx.drawing_path.clone();
        if self.loaded && self.path == path {
            return;
        }
        self.path = path.clone();
        self.loaded = true;
        self.set = SuggestionSet::new();
        self.audit = AuditLog::new();
        if let Some(p) = path.as_ref().filter(|p| is_aadt_path(p)) {
            if let Ok(Some((_, _, a))) = aadt_store::read_objects_path(p) {
                self.audit = a;
            }
        }
    }

    /// Persist the audit log, preserving whatever objects/graph are on disk.
    /// Decisions about AI proposals belong in the same append-only trail as
    /// every other object mutation.
    fn persist(&self) {
        if let Some(p) = self.path.as_ref().filter(|p| is_aadt_path(p)) {
            let (objects, graph) = aadt_store::read_objects_path(p)
                .ok()
                .flatten()
                .map(|(o, g, _)| (o, g))
                .unwrap_or_default();
            let _ = aadt_store::write_objects_path(p, &objects, &graph, &self.audit);
        }
    }
}

/// Render one proposal as a display line.
fn suggestion_line(s: &aadt_assist::Suggestion) -> String {
    format!(
        "  #{} [{}] {} (conf {:.0}%, {})",
        s.id,
        s.kind,
        s.summary,
        s.confidence * 100.0,
        s.status.as_str()
    )
}

/// `SUGGEST` — the AI service layer's proposal surface. Services propose;
/// nothing is applied until `ACCEPT` names a proposal explicitly.
struct SuggestCmd {
    st: SharedSuggestions,
}

impl SuggestCmd {
    fn do_dims(&self, ctx: &mut Context) -> Result<String, String> {
        let mut st = self.st.borrow_mut();
        let found = aadt_assist::suggest_dimensions(&ctx.doc);
        let n = found.len();
        // Refresh only the dimension proposals — outstanding code findings stay.
        st.set
            .replace_kind(aadt_assist::SuggestionKind::Dimension, found);
        if n == 0 {
            return Ok("SUGGEST: every detected wall already carries a dimension".into());
        }
        ctx.selection = st.set.iter().flat_map(|s| s.targets.clone()).collect();
        let lines: Vec<String> = st
            .set
            .iter()
            .filter(|s| s.kind == aadt_assist::SuggestionKind::Dimension)
            .map(suggestion_line)
            .collect();
        Ok(format!(
            "SUGGEST: {n} dimension proposal(s) — nothing applied yet; SUGGEST ACCEPT <id> or REJECT <id>:\n{}",
            lines.join("\n")
        ))
    }

    fn do_list(&self) -> Result<String, String> {
        let st = self.st.borrow();
        if st.set.is_empty() {
            return Ok("SUGGEST: no proposals — run SUGGEST DIMS or CHECK first".into());
        }
        let lines: Vec<String> = st.set.iter().map(suggestion_line).collect();
        let counts: Vec<String> = st
            .set
            .kind_counts()
            .into_iter()
            .map(|(k, n)| format!("{n} {k}"))
            .collect();
        Ok(format!(
            "{} proposal(s) ({}):\n{}",
            st.set.len(),
            counts.join(", "),
            lines.join("\n")
        ))
    }

    fn do_why(&self, args: &[String]) -> Result<String, String> {
        let id = parse_id(args, 1, "SUGGEST WHY")? as u32;
        let st = self.st.borrow();
        let Some(s) = st.set.get(id) else {
            return Err(format!("SUGGEST WHY: no proposal #{id}"));
        };
        Ok(format!(
            "#{} [{}] {}\n  confidence: {:.0}%\n  status: {}\n  rationale: {}\n  applicable: {}",
            s.id,
            s.kind,
            s.summary,
            s.confidence * 100.0,
            s.status.as_str(),
            s.rationale,
            if s.action.is_applicable() {
                "yes — SUGGEST ACCEPT applies it"
            } else {
                "no — advisory only"
            }
        ))
    }

    fn do_accept(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_id(args, 1, "SUGGEST ACCEPT")? as u32;
        let mut st = self.st.borrow_mut();
        let SuggestState { set, audit, .. } = &mut *st;
        let summary = aadt_assist::apply(set, &mut ctx.doc, audit, id)
            .map_err(|e| format!("SUGGEST: {e}"))?;
        st.persist();
        Ok(format!("SUGGEST: accepted #{id} — {summary} (undoable)"))
    }

    fn do_reject(&self, args: &[String]) -> Result<String, String> {
        let id = parse_id(args, 1, "SUGGEST REJECT")? as u32;
        let mut st = self.st.borrow_mut();
        let SuggestState { set, audit, .. } = &mut *st;
        let summary = aadt_assist::reject(set, audit, id).map_err(|e| format!("SUGGEST: {e}"))?;
        st.persist();
        Ok(format!("SUGGEST: rejected #{id} — {summary} (recorded)"))
    }

    fn do_accept_all(&self, ctx: &mut Context) -> Result<String, String> {
        let mut st = self.st.borrow_mut();
        let ids: Vec<u32> = st
            .set
            .pending()
            .iter()
            .filter(|s| s.action.is_applicable())
            .map(|s| s.id)
            .collect();
        if ids.is_empty() {
            return Ok("SUGGEST: nothing pending that can be applied".into());
        }
        let SuggestState { set, audit, .. } = &mut *st;
        let mut applied = 0;
        for id in &ids {
            if aadt_assist::apply(set, &mut ctx.doc, audit, *id).is_ok() {
                applied += 1;
            }
        }
        st.persist();
        Ok(format!(
            "SUGGEST: accepted {applied} proposal(s) (each undoable individually)"
        ))
    }
}

impl Command for SuggestCmd {
    fn name(&self) -> &str {
        "SUGGEST"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["PROPOSE"]
    }
    fn help(&self) -> &str {
        "SUGGEST DIMS|LIST|WHY id|ACCEPT id|ACCEPTALL|REJECT id — AI proposals; nothing applies until you accept"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        self.st.borrow_mut().sync(ctx);
        let sub = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_default();
        match sub.as_str() {
            "" | "LIST" => self.do_list(),
            "DIMS" | "DIMENSIONS" => self.do_dims(ctx),
            "WHY" | "INFO" => self.do_why(args),
            "ACCEPT" | "OK" => self.do_accept(ctx, args),
            "ACCEPTALL" | "ACCEPT-ALL" => self.do_accept_all(ctx),
            "REJECT" | "NO" => self.do_reject(args),
            other => Err(format!(
                "SUGGEST: unknown subcommand {other:?} (DIMS|LIST|WHY id|ACCEPT id|ACCEPTALL|REJECT id)"
            )),
        }
    }
}

/// `CHECK` — deterministic code/standards rule packs. Findings are advisory and
/// cite the governing passage when a spec corpus has been indexed.
struct CheckCmd {
    st: SharedSuggestions,
    corpus: SharedCorpus,
}

impl Command for CheckCmd {
    fn name(&self) -> &str {
        "CHECK"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["AUDIT-RULES", "CODECHECK"]
    }
    fn help(&self) -> &str {
        "CHECK [rule|RULES] — run code/standards rule packs (cites indexed spec passages)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let arg = args.first().map(|s| s.to_ascii_uppercase());
        if arg.as_deref() == Some("RULES") {
            let lines: Vec<String> = RuleId::ALL
                .iter()
                .map(|r| format!("  {} — {}", r, r.description()))
                .collect();
            return Ok(format!(
                "{} rule(s):\n{}",
                RuleId::ALL.len(),
                lines.join("\n")
            ));
        }
        let only = match args.first() {
            None => None,
            Some(a) => match RuleId::parse(a) {
                Some(r) => Some(r),
                None => {
                    return Err(format!(
                        "CHECK: unknown rule {a:?} — try CHECK RULES for the list"
                    ))
                }
            },
        };

        self.st.borrow_mut().sync(ctx);
        let found = {
            let cg = self.corpus.borrow();
            let corpus = cg.as_ref().map(|c| &c.corpus);
            check_as_suggestions(&ctx.doc, corpus, only)
        };
        let n = found.len();
        {
            let mut st = self.st.borrow_mut();
            // Refresh only the findings — outstanding dimension proposals stay.
            st.set
                .replace_kind(aadt_assist::SuggestionKind::CodeCheck, found);
            ctx.selection = st
                .set
                .iter()
                .filter(|s| s.kind == aadt_assist::SuggestionKind::CodeCheck)
                .flat_map(|s| s.targets.clone())
                .collect();
        }
        if n == 0 {
            return Ok("CHECK: no findings — every checked element meets its threshold".into());
        }
        let st = self.st.borrow();
        let lines: Vec<String> = st
            .set
            .iter()
            .filter(|s| s.kind == aadt_assist::SuggestionKind::CodeCheck)
            .map(|s| format!("{}\n      {}", suggestion_line(s), s.rationale))
            .collect();
        Ok(format!(
            "CHECK: {n} finding(s) — advisory, nothing changed:\n{}",
            lines.join("\n")
        ))
    }
}

/// `ASK` — natural-language command routing. Maps a phrase to a *command
/// string* and shows it; it never runs it and never touches geometry directly.
struct AskCmd;

impl Command for AskCmd {
    fn name(&self) -> &str {
        "ASK"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["NL"]
    }
    fn help(&self) -> &str {
        "ASK <phrase> — route natural language to a command (shows it; run it yourself)"
    }
    fn run(&self, _ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let phrase = args.join(" ");
        if phrase.trim().is_empty() {
            return Err("ASK: give a phrase, e.g. ASK draw a wall from 0,0 to 5,0".into());
        }
        match aadt_assist::route(&phrase) {
            Routed::Command {
                command,
                confidence,
                rationale,
            } => Ok(format!(
                "ASK: {phrase:?} → `{command}` (conf {:.0}%, {rationale})\n  \
                 not executed — run it yourself if that is what you meant",
                confidence * 100.0
            )),
            Routed::Unroutable { candidates, reason } => {
                let tail = if candidates.is_empty() {
                    String::new()
                } else {
                    format!(" — did you mean: {}", candidates.join(", "))
                };
                Err(format!("ASK: {reason}{tail}"))
            }
        }
    }
}

/// `TAKEOFF` — quantity takeoff over the Object DB (AI-native Phase 4).
/// Promotes the drawing into objects, measures them, and reports. `SAVE`
/// persists the run into the `.aadt` takeoff tables.
struct TakeoffCmd;

impl TakeoffCmd {
    fn report(run: &aadt_takeoff::TakeoffRun) -> String {
        let t = run.totals();
        let mut out = format!(
            "TAKEOFF (rev {}): {} wall(s), {} room(s), {} opening(s)\n  \
             wall area {:.2} m² net · volume {:.3} m³ · {} bricks\n  \
             floor area {:.2} m² · paintable {:.2} m²",
            run.doc_revision,
            t.walls,
            t.rooms,
            t.openings,
            t.wall_area,
            t.wall_volume,
            t.bricks,
            t.floor_area,
            t.paint_area,
        );
        // Assumptions a reviewer must confirm before pricing.
        let assumed = run.assumed_openings();
        if !assumed.is_empty() {
            out.push_str(&format!(
                "\n  note: {} opening(s) sized from defaults, not drawn dimensions",
                assumed.len()
            ));
        }
        let unattached = run
            .quantities
            .openings
            .iter()
            .filter(|o| o.in_wall.is_none())
            .count();
        if unattached > 0 {
            out.push_str(&format!(
                "\n  note: {unattached} opening(s) not attributed to a wall — not deducted"
            ));
        }
        out
    }
}

impl Command for TakeoffCmd {
    fn name(&self) -> &str {
        "TAKEOFF"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["QTO", "QUANTITIES"]
    }
    fn help(&self) -> &str {
        "TAKEOFF [WALLS|ROOMS|OPENINGS|SAVE] — measure quantities from the Object DB"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let cfg = aadt_takeoff::TakeoffConfig::default();
        let run = aadt_takeoff::run(&ctx.doc, &cfg);
        let sub = args
            .first()
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_default();
        match sub.as_str() {
            "" | "ALL" | "TOTALS" => Ok(Self::report(&run)),
            "WALLS" => {
                if run.quantities.walls.is_empty() {
                    return Ok("TAKEOFF: no walls detected".into());
                }
                let lines: Vec<String> = run
                    .quantities
                    .walls
                    .iter()
                    .map(|w| {
                        format!(
                            "  #{} {} — {:.3} m × {:.3} m thick, net {:.2} m² (less {:.2} m² openings), {:.3} m³, {} bricks [{}]",
                            w.object.0, w.layer, w.length, w.thickness, w.net_area,
                            w.opening_area, w.volume, w.bricks, w.cost_code
                        )
                    })
                    .collect();
                Ok(format!(
                    "{} wall(s):\n{}",
                    run.quantities.walls.len(),
                    lines.join("\n")
                ))
            }
            "ROOMS" => {
                if run.quantities.rooms.is_empty() {
                    return Ok("TAKEOFF: no rooms detected".into());
                }
                let lines: Vec<String> = run
                    .quantities
                    .rooms
                    .iter()
                    .map(|r| {
                        format!(
                            "  #{} {} — floor {:.2} m², perimeter {:.2} m, wall finish {:.2} m², paint {:.2} m² [{}]",
                            r.object.0, r.layer, r.floor_area, r.perimeter,
                            r.wall_finish_area, r.paint_area, r.cost_code
                        )
                    })
                    .collect();
                Ok(format!(
                    "{} room(s):\n{}",
                    run.quantities.rooms.len(),
                    lines.join("\n")
                ))
            }
            "OPENINGS" => {
                if run.quantities.openings.is_empty() {
                    return Ok("TAKEOFF: no openings detected".into());
                }
                let lines: Vec<String> = run
                    .quantities
                    .openings
                    .iter()
                    .map(|o| {
                        format!(
                            "  #{} {} — {:.3} × {:.3} m = {:.3} m² [{}]{}{}",
                            o.object.0,
                            o.kind,
                            o.width,
                            o.height,
                            o.area,
                            o.cost_code,
                            if o.measured { "" } else { " (assumed size)" },
                            match o.in_wall {
                                Some(w) => format!(" in wall #{}", w.0),
                                None => " (no host wall)".into(),
                            }
                        )
                    })
                    .collect();
                Ok(format!(
                    "{} opening(s):\n{}",
                    run.quantities.openings.len(),
                    lines.join("\n")
                ))
            }
            "SAVE" | "PUBLISH" => {
                let path = resolve_drawing_path(ctx, None)?;
                if !is_aadt_path(&path) {
                    return Err("TAKEOFF SAVE: .aadt file required".into());
                }
                let data = aadt_store::run_to_row_data(&run, None);
                let id = aadt_store::append_takeoff_run(&path, &data)
                    .map_err(|e| format!("TAKEOFF SAVE: {e}"))?;
                Ok(format!(
                    "TAKEOFF: saved run #{id} ({} wall / {} room / {} opening rows)",
                    data.walls.len(),
                    data.rooms.len(),
                    data.openings.len()
                ))
            }
            other => Err(format!(
                "TAKEOFF: unknown subcommand {other:?} (WALLS|ROOMS|OPENINGS|SAVE)"
            )),
        }
    }
}

/// `BOQ` — the bill of quantities, aggregated by CSI MasterFormat cost code.
struct BoqCmd;

impl Command for BoqCmd {
    fn name(&self) -> &str {
        "BOQ"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["BILL"]
    }
    fn help(&self) -> &str {
        "BOQ — bill of quantities by CSI MasterFormat cost code"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let run = aadt_takeoff::run(&ctx.doc, &aadt_takeoff::TakeoffConfig::default());
        if run.boq.is_empty() {
            return Ok("BOQ: nothing measured — the drawing has no promotable objects".into());
        }
        let mut out = format!("BOQ — {} line(s):", run.boq.len());
        let mut current = "";
        for l in &run.boq.lines {
            if l.cost_code.code != current {
                current = l.cost_code.code;
                out.push_str(&format!("\n  {}", l.cost_code));
            }
            out.push_str(&format!(
                "\n    {:<32} {:>12.3} {}",
                l.description, l.quantity, l.unit
            ));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod server_tests {
    use super::*;
    use aadt_command::Interpreter;

    /// Evaluate one line against a fresh interpreter with the CLI commands
    /// registered (a new, disconnected `ServerState` each call).
    fn run(line: &str) -> Result<Option<String>, String> {
        let mut interp = Interpreter::with_builtins();
        register(&mut interp);
        let mut ctx = Context::new();
        interp.eval_line(&mut ctx, line)
    }

    #[test]
    fn server_requires_a_subcommand() {
        assert!(run("SERVER").is_err());
    }

    #[test]
    fn unknown_subcommand_errors() {
        let e = run("SERVER FROB").unwrap_err();
        assert!(e.contains("unknown subcommand"), "unexpected error: {e}");
    }

    #[test]
    fn status_and_disconnect_when_offline() {
        assert_eq!(run("SERVER STATUS").unwrap(), Some("not connected".into()));
        assert_eq!(
            run("SERVER DISCONNECT").unwrap(),
            Some("not connected".into())
        );
    }

    #[test]
    fn push_and_pull_require_a_connection() {
        assert!(run("SERVER PUSH").unwrap_err().contains("not connected"));
        assert!(run("SERVER PULL").unwrap_err().contains("not connected"));
    }
}

#[cfg(test)]
mod layer_legend_tests {
    use super::*;
    use aadt_command::Interpreter;
    use aadt_geometry::Segment;

    fn interp() -> (Interpreter, Context) {
        let mut interp = Interpreter::with_builtins();
        register(&mut interp);
        (interp, Context::new())
    }

    #[test]
    fn draws_a_text_row_per_used_layer_plus_title() {
        let (interp, mut ctx) = interp();
        ctx.doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        ctx.doc.add_geometry(
            "A-WINDOW",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(0.0, 1.0))),
        );
        let msg = interp
            .eval_line(&mut ctx, "LAYERLEGEND 0 0 100")
            .unwrap()
            .unwrap();
        assert!(msg.contains("2 layers"), "msg: {msg}");
        // Title + one row per used layer.
        let legend_texts = ctx
            .doc
            .iter_entities()
            .filter(|e| e.layer == "G-LEGEND" && matches!(e.geometry, Geometry::Text(_)))
            .count();
        assert_eq!(legend_texts, 3, "title + 2 rows");
    }

    #[test]
    fn empty_drawing_errors() {
        let (interp, mut ctx) = interp();
        let e = interp.eval_line(&mut ctx, "LAYERLEGEND").unwrap_err();
        assert!(e.contains("no layers in use"), "err: {e}");
    }
}
