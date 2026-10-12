//! Generating a project from a configuration: the folders, one drawing file per planned drawing
//! (made through the engine's own commands, like any drawing), the Xrefs between them, and a check
//! that everything exists and resolves before the project is marked ready.
//!
//! Stages: `Validating → Generating → LinkingXrefs → Verifying → Ready`. A failure ends in `Failed`,
//! never `Ready`. The filesystem has no transaction across a tree, so the recovery strategy is:
//!
//! - `generation.log.json` in the project folder records the plan, every file this run created (with
//!   a hash of its content) and the file in progress. It is replaced atomically after every step.
//! - Existing files are never overwritten: a drawing that is already there is kept as it is, and a
//!   kept drawing is not modified to add Xrefs. The only file ever rewritten is the one the log says
//!   was in progress when an earlier run was interrupted.
//! - [`inspect_recovery`] reports what an interrupted run left; [`cleanup`] removes only files the
//!   log recorded and whose content is still exactly what was written (a file the user has since
//!   edited is left alone); resuming is `generate` with `resume: true`.
//!
//! Overlay Xrefs are session-only in this engine (never written into the drawing), so they are
//! recorded in the manifest and not attached to the file; only attached Xrefs are written and
//! verified.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use aad_doc::Drawing;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::company::write_atomic;
use super::plan::MANIFEST_FILE;
use super::{Attachment, Issue, Plan, PlannedDrawing, ProjectConfig, Status, plan};
use crate::Session;
use crate::cmd::project;

pub const LOG_FILE: &str = "generation.log.json";

#[derive(Clone, Copy, Debug, Default)]
pub struct GenerateOptions {
    /// Continue an interrupted run (the folder holds a generation log).
    pub resume: bool,
    /// Allow creating the project in a folder that already holds other files.
    pub allow_non_empty: bool,
}

/// What a run did.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Report {
    pub status: Status,
    pub created: Vec<String>,
    /// Planned drawings that already existed and were left exactly as they were.
    pub kept: Vec<String>,
    pub xrefs_attached: usize,
    /// Overlay edges recorded in the manifest only.
    pub xrefs_recorded: usize,
    pub issues: Vec<Issue>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct LogFile {
    id: String,
    rel_path: String,
    hash: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Log {
    project_id: String,
    stage: String,
    planned: usize,
    created: Vec<LogFile>,
    in_progress: Option<String>,
    /// Drawings whose file may be half written (a run stopped while writing them): the only existing
    /// files a resumed run rewrites.
    partial: Vec<String>,
}

/// FNV-1a of a file's bytes: enough to tell whether a file is still what was written.
fn hash_bytes(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, x| (h ^ u64::from(*x)).wrapping_mul(0x0100_0000_01b3))
}

fn file_hash(p: &Path) -> Option<u64> {
    std::fs::read(p).ok().map(|b| hash_bytes(&b))
}

fn write_log(root: &Path, log: &Log) -> Result<(), String> {
    write_atomic(&root.join(LOG_FILE), serde_json::to_string_pretty(log).map_err(|e| e.to_string())?.as_bytes())
}

fn read_log(root: &Path) -> Option<Log> {
    serde_json::from_str(&std::fs::read_to_string(root.join(LOG_FILE)).ok()?).ok()
}

fn write_manifest(root: &Path, cfg: &ProjectConfig) -> Result<(), String> {
    write_atomic(&root.join(MANIFEST_FILE), cfg.to_json().as_bytes())
}

fn abs(root: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(root.to_path_buf(), |p, seg| p.join(seg))
}

/// Make a new drawing, run `init` on it, write it to `path` and close it again.
fn make_file(s: &mut Session, path: &Path, init: &dyn Fn(&mut Session) -> crate::Result<()>) -> crate::Result<()> {
    let keep = s.active;
    let had = s.docs.len();
    let i = s.open_drawing(Drawing::new_metric(), "New", None);
    let r = init(s).and_then(|()| s.execute("saveas", &json!({ "path": path.to_str().unwrap_or_default() })).map(|_| ()));
    let _ = s.execute("close", &json!({ "index": i }));
    s.active = if keep < had { keep } else { s.docs.len().saturating_sub(1) };
    r
}

/// Open a file, returning its index.
fn open(s: &mut Session, path: &Path) -> crate::Result<usize> {
    let r = s.execute("open", &json!({ "path": path.to_str().unwrap_or_default() }))?;
    Ok(r["index"].as_u64().map_or(s.active, |i| i as usize))
}

fn close(s: &mut Session, i: usize) {
    let _ = s.execute("close", &json!({ "index": i }));
}

/// The data every generated drawing carries: its identity and number, the project, the floor.
fn init_drawing(s: &mut Session, cfg: &ProjectConfig, plan: &Plan, d: &PlannedDrawing) -> crate::Result<()> {
    let per_m = if cfg.project.units.length == "m" { 1.0 } else { 1000.0 };
    s.execute("drawing.units", &json!({ "units": cfg.project.units.length, "mode": "metadata" }))?;
    let floor = d.floor_id.as_ref().and_then(|id| cfg.floors.iter().find(|f| &f.id == id));
    let elevation = d.floor_id.as_ref().and_then(|id| plan.elevations.iter().find(|(f, _)| f == id)).map(|(_, z)| *z);
    let details = json!({
        "title": cfg.project.name, "owner": cfg.project.client.name, "address": cfg.project.address.line1,
        "location": cfg.project.address.city, "pid": cfg.project.code,
    });
    let floors: Vec<serde_json::Value> = cfg
        .floors_in_order()
        .iter()
        .map(|f| match f.floor_to_floor {
            Some(h) => json!({"name": f.name, "floorToFloor": h / per_m}),
            None => json!({"name": f.name}),
        })
        .collect();
    let meta = json!({
        "projectId": cfg.project.id, "drawingId": d.id, "kind": d.kind, "number": d.number, "title": d.title,
        "setId": d.set_id, "floorId": d.floor_id, "floorName": floor.map(|f| f.name.clone()), "elevation": elevation,
    });
    let doc = s.doc_mut()?;
    project::set(doc, "details", &details);
    project::set(doc, "floors", &serde_json::Value::Array(floors));
    project::set(doc, "aadDrawing", &meta);
    // The floor commands find the compatibility manifest through this pointer, relative to the drawing.
    project::set(doc, "projectManifest", &json!(super::relative_path(&d.rel_path, super::compat::FILE)));
    apply_typography(doc, &cfg.typography.to_registry());
    Ok(())
}

/// Give a drawing the text styles of a typography: one `AAD-<Role>` style per role, naming the
/// preset's family at the role's weight (the intended font, whether or not it is installed on this
/// machine), `Standard` and the leader styles on the Primary font, dimension styles on the
/// Dimension font. Heights, widths and every other style property stay as they are.
pub fn apply_typography(d: &mut Drawing, t: &aad_fonts::registry::Typography) {
    use aad_fonts::registry::{ROLES, Role, family_at, style_name};
    let preset = t.preset();
    let font_of = |role: Role| {
        let r = t.resolve(role);
        let face = t.overrides.iter().find(|(o, _, _)| *o == role).map_or_else(|| role.default_face().0, |(_, f, _)| *f);
        family_at(preset.family(face), r.weight)
    };
    for role in ROLES {
        let name = style_name(role);
        let font = font_of(role);
        match d.text_styles.iter_mut().find(|s| s.name == name) {
            Some(s) => s.font = font,
            None => d.text_styles.push(aad_doc::TextStyle { name, font, ..aad_doc::TextStyle::default() }),
        }
    }
    let primary = font_of(Role::Primary);
    if let Some(s) = d.text_styles.iter_mut().find(|s| s.name.eq_ignore_ascii_case("Standard")) {
        s.font = primary;
    }
    let (dim, lead) = (style_name(Role::Dimension), style_name(Role::Primary));
    for ds in &mut d.dim_styles {
        ds.text_style = dim.clone();
    }
    for ms in &mut d.mleader_styles {
        ms.text_style = lead.clone();
    }
}

/// A name for each Xref target that is unique in the project.
fn xref_names(plan: &Plan) -> HashMap<String, String> {
    let mut used: HashSet<String> = HashSet::new();
    let mut out = HashMap::new();
    for d in &plan.drawings {
        let stem = d.rel_path.rsplit('/').next().unwrap_or("").rsplit_once('.').map_or("", |(s, _)| s);
        let base: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
        let mut name = base.clone();
        let mut n = 2;
        while !used.insert(name.to_ascii_uppercase()) {
            name = format!("{base}_{n}");
            n += 1;
        }
        out.insert(d.id.clone(), name);
    }
    out
}

fn fail(cfg: &mut ProjectConfig, root: &Path, report: &mut Report, why: String) {
    report.issues.push(Issue::new("generation", why));
    report.status = Status::Failed;
    cfg.generation.status = Status::Failed;
    let _ = write_manifest(root, cfg);
}

/// Create the project described by `cfg` in `root`. Nothing is written when the configuration or
/// the folder does not pass the checks (the report says why).
pub fn generate(s: &mut Session, root: &Path, mut cfg: ProjectConfig, opts: GenerateOptions) -> Report {
    let mut report = Report { status: Status::Validating, created: vec![], kept: vec![], xrefs_attached: 0, xrefs_recorded: 0, issues: vec![] };
    // ---- Validating
    let pl = match plan(&cfg) {
        Ok(p) => p,
        Err(issues) => {
            report.issues = issues;
            report.status = Status::Draft;
            return report;
        }
    };
    if cfg.naming_rules.file_extension != "dxf" {
        report.issues.push(Issue::new("naming_rules.file_extension", "this build writes DXF only: DWG generation is not available yet"));
    }
    let log = read_log(root);
    match (&log, opts.resume) {
        (Some(_), false) => {
            report.issues.push(Issue::new("root", "an earlier generation was interrupted here: continue it (resume) or clean it up first"))
        }
        (None, true) => report.issues.push(Issue::new("root", "there is no interrupted generation to resume in this folder")),
        _ => {}
    }
    if let Ok(text) = std::fs::read_to_string(root.join(MANIFEST_FILE)) {
        match ProjectConfig::from_json(&text) {
            Ok(old) if old.project.id != cfg.project.id => report.issues.push(Issue::new("root", "this folder holds a different project")),
            Ok(_) => {}
            Err(e) => report.issues.push(Issue::new("root", e)),
        }
    } else if root.is_dir() && log.is_none() && !opts.allow_non_empty && std::fs::read_dir(root).map(|mut d| d.next().is_some()).unwrap_or(false) {
        report.issues.push(Issue::new("root", "the folder is not empty: choose an empty folder or allow using it"));
    }
    if !report.issues.is_empty() {
        report.status = Status::Draft;
        return report;
    }

    // ---- Generating
    cfg.generation.status = Status::Generating;
    let mut log = log.unwrap_or(Log { project_id: cfg.project.id.clone(), planned: pl.drawings.len(), ..Log::default() });
    log.stage = "generating".into();
    if let Some(id) = log.in_progress.take() {
        log.partial.push(id);
    }
    let started = (|| -> Result<(), String> {
        for f in &pl.folders {
            std::fs::create_dir_all(abs(root, f)).map_err(|e| format!("cannot create {f}: {e}"))?;
        }
        write_manifest(root, &cfg)?;
        write_log(root, &log)
    })();
    if let Err(e) = started {
        fail(&mut cfg, root, &mut report, e);
        return report;
    }
    report.status = Status::Generating;
    let mut created_ids: HashSet<String> = HashSet::new();
    for d in &pl.drawings {
        let path = abs(root, &d.rel_path);
        let ours_partial = log.partial.contains(&d.id);
        if path.exists() && !ours_partial {
            // Already there: the user's file, or one an earlier run finished. Left exactly as it is.
            if log.created.iter().any(|c| c.id == d.id) {
                created_ids.insert(d.id.clone());
            }
            report.kept.push(d.rel_path.clone());
            continue;
        }
        log.in_progress = Some(d.id.clone());
        if !log.partial.contains(&d.id) {
            log.partial.push(d.id.clone());
        }
        if let Err(e) = write_log(root, &log) {
            fail(&mut cfg, root, &mut report, e);
            return report;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = make_file(s, &path, &|s| init_drawing(s, &cfg, &pl, d)) {
            fail(&mut cfg, root, &mut report, format!("{}: {e}", d.rel_path));
            return report;
        }
        log.created.retain(|c| c.id != d.id);
        log.created.push(LogFile { id: d.id.clone(), rel_path: d.rel_path.clone(), hash: file_hash(&path).unwrap_or(0) });
        log.in_progress = None;
        log.partial.retain(|p| p != &d.id);
        created_ids.insert(d.id.clone());
        report.created.push(d.rel_path.clone());
        if let Err(e) = write_log(root, &log) {
            fail(&mut cfg, root, &mut report, e);
            return report;
        }
    }

    // ---- Linking Xrefs (only in files this run made: a kept drawing is never modified)
    report.status = Status::LinkingXrefs;
    log.stage = "linking".into();
    let _ = write_log(root, &log);
    let names = xref_names(&pl);
    let by_id: HashMap<&str, &PlannedDrawing> = pl.drawings.iter().map(|d| (d.id.as_str(), d)).collect();
    let mut hosts: Vec<&str> = pl.xrefs.iter().map(|x| x.host_id.as_str()).collect();
    hosts.dedup();
    let mut seen_hosts: HashSet<&str> = HashSet::new();
    for host_id in hosts.into_iter().filter(|h| seen_hosts.insert(h)) {
        let Some(host) = by_id.get(host_id) else { continue };
        let edges: Vec<_> = pl.xrefs.iter().filter(|x| x.host_id == host_id).collect();
        report.xrefs_recorded += edges.iter().filter(|x| x.attachment == Attachment::Overlay).count();
        if !created_ids.contains(host_id) {
            continue;
        }
        let host_path = abs(root, &host.rel_path);
        let result = (|| -> crate::Result<usize> {
            let i = open(s, &host_path)?;
            let mut n = 0;
            let r = (|| -> crate::Result<()> {
                for x in edges.iter().filter(|x| x.attachment == Attachment::Attach) {
                    let Some(t) = by_id.get(x.target_id.as_str()) else { continue };
                    let name = names.get(&t.id).cloned().unwrap_or_default();
                    s.execute("xref.attach", &json!({"path": abs(root, &t.rel_path).to_str().unwrap_or_default(), "name": name, "at": [0, 0]}))?;
                    n += 1;
                }
                s.execute("qsave", &json!({})).map(|_| ())
            })();
            close(s, i);
            r.map(|()| n)
        })();
        match result {
            Ok(n) => report.xrefs_attached += n,
            Err(e) => {
                fail(&mut cfg, root, &mut report, format!("linking {}: {e}", host.rel_path));
                return report;
            }
        }
        // The host changed: record its new content hash so cleanup still recognises it as ours.
        if let Some(c) = log.created.iter_mut().find(|c| c.id == host_id) {
            c.hash = file_hash(&host_path).unwrap_or(0);
        }
        let _ = write_log(root, &log);
    }

    // ---- Verifying
    report.status = Status::Verifying;
    log.stage = "verifying".into();
    let _ = write_log(root, &log);
    for d in &pl.drawings {
        let path = abs(root, &d.rel_path);
        let wanted: Vec<_> = pl.xrefs.iter().filter(|x| x.host_id == d.id && x.attachment == Attachment::Attach).collect();
        let check = (|| -> Result<Vec<Issue>, String> {
            let i = open(s, &path).map_err(|e| format!("does not open: {e}"))?;
            let mut out = Vec::new();
            if created_ids.contains(&d.id) && !wanted.is_empty() {
                match s.execute("xref.list", &json!({})) {
                    Ok(v) => {
                        let list = v["xrefs"].as_array().cloned().unwrap_or_default();
                        for x in &wanted {
                            let name = by_id.get(x.target_id.as_str()).and_then(|t| names.get(&t.id)).cloned().unwrap_or_default();
                            let found = list.iter().find(|e| e["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(&name)));
                            match found {
                                None => out.push(Issue::new(&d.rel_path, format!("the Xref `{name}` is not in the drawing"))),
                                Some(e) if e["status"] != "found" => out.push(Issue::new(&d.rel_path, format!("the Xref `{name}` does not resolve"))),
                                Some(_) => {}
                            }
                        }
                    }
                    Err(e) => out.push(Issue::new(&d.rel_path, format!("the Xrefs cannot be listed: {e}"))),
                }
            }
            close(s, i);
            Ok(out)
        })();
        match check {
            Ok(v) => report.issues.extend(v),
            Err(e) => report.issues.push(Issue::new(&d.rel_path, e)),
        }
    }
    // The compatibility manifest for the floor commands, made from project.json and checked against it.
    match super::compat::write(root, &cfg, &pl) {
        Ok(()) => report.issues.extend(super::compat::check(root, &cfg, &pl)),
        Err(e) => report.issues.extend(e),
    }
    cfg.generated.drawings = pl.drawings.clone();
    cfg.generated.xrefs = pl.xrefs.clone();
    if report.issues.is_empty() {
        cfg.generation.status = Status::Ready;
        report.status = Status::Ready;
        if write_manifest(root, &cfg).is_ok() {
            let _ = std::fs::remove_file(root.join(LOG_FILE));
        }
    } else {
        cfg.generation.status = Status::Failed;
        report.status = Status::Failed;
        let _ = write_manifest(root, &cfg);
    }
    report
}

/// What an interrupted generation left in `root`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Recovery {
    pub stage: String,
    pub planned: usize,
    /// Files the log recorded that still have exactly the content that was written.
    pub unchanged: Vec<String>,
    /// Recorded files that were edited since (cleanup will not touch them).
    pub modified: Vec<String>,
    /// Recorded files that are gone.
    pub missing: Vec<String>,
    /// The file that was being written when the run stopped (it may be incomplete).
    pub in_progress: Option<String>,
}

pub fn inspect_recovery(root: &Path) -> Option<Recovery> {
    let log = read_log(root)?;
    let (mut unchanged, mut modified, mut missing) = (vec![], vec![], vec![]);
    for c in &log.created {
        match file_hash(&abs(root, &c.rel_path)) {
            None => missing.push(c.rel_path.clone()),
            Some(h) if h == c.hash => unchanged.push(c.rel_path.clone()),
            Some(_) => modified.push(c.rel_path.clone()),
        }
    }
    let in_progress = log.in_progress.clone();
    Some(Recovery { stage: log.stage, planned: log.planned, unchanged, modified, missing, in_progress })
}

/// Remove what an interrupted run created, and only that: files the log recorded whose content is
/// still exactly what was written. Edited files stay. The log is removed and the manifest marked
/// failed. Returns the files removed and the files left.
pub fn cleanup(root: &Path) -> Result<(Vec<String>, Vec<String>), String> {
    let log = read_log(root).ok_or("there is no interrupted generation in this folder")?;
    let (mut removed, mut left) = (vec![], vec![]);
    for c in &log.created {
        let path = abs(root, &c.rel_path);
        match file_hash(&path) {
            Some(h) if h == c.hash => match std::fs::remove_file(&path) {
                Ok(()) => removed.push(c.rel_path.clone()),
                Err(_) => left.push(c.rel_path.clone()),
            },
            Some(_) => left.push(c.rel_path.clone()),
            None => {}
        }
    }
    if let Ok(text) = std::fs::read_to_string(root.join(MANIFEST_FILE))
        && let Ok(mut cfg) = ProjectConfig::from_json(&text)
    {
        cfg.generation.status = Status::Failed;
        let _ = write_manifest(root, &cfg);
    }
    let _ = std::fs::remove_file(root.join(LOG_FILE));
    Ok((removed, left))
}
