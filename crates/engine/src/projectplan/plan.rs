//! Validation and the generation plan: what folders, drawings and Xrefs a configuration means.
//! Pure functions of the configuration (no clock, no disk) so the same configuration always gives
//! the same plan, with the same stable drawing ids, and a regeneration can tell what already exists.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::{
    Attachment, DrawingKind, DrawingSet, FloorPolicy, Issue, MAX_DRAWINGS, MAX_FLOORS, MAX_RULES, MAX_SETS, ProjectConfig, Selector, is_code,
};

/// The manifest file name inside a project folder.
pub const MANIFEST_FILE: &str = "project.json";

/// One drawing the project will have.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct PlannedDrawing {
    /// Stable: derived from the project, set, floor and kind, so it survives renames and regeneration.
    pub id: String,
    pub kind: DrawingKind,
    pub set_id: Option<String>,
    pub floor_id: Option<String>,
    /// Relative to the project root, with `/` separators.
    pub rel_path: String,
    pub number: String,
    pub title: String,
}

/// One Xref from a host drawing to a target drawing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct PlannedXref {
    pub id: String,
    pub host_id: String,
    pub target_id: String,
    pub attachment: Attachment,
    /// From the host drawing's folder to the target file.
    pub relative_path: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Plan {
    pub folders: Vec<String>,
    pub drawings: Vec<PlannedDrawing>,
    pub xrefs: Vec<PlannedXref>,
    /// Other files the generator writes (the manifest, schedule files).
    pub files: Vec<String>,
    /// Floor id and its resolved elevation, bottom to top.
    pub elevations: Vec<(String, f64)>,
}

/// FNV-1a over the parts: a stable 64-bit hash for derived ids.
fn stable_hash(parts: &[&str]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for b in p.bytes().chain(std::iter::once(0xff)) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

fn derived_id(prefix: &str, parts: &[&str]) -> String {
    format!("{prefix}-{:016x}", stable_hash(parts))
}

fn kind_name(k: DrawingKind) -> &'static str {
    match k {
        DrawingKind::Master => "master",
        DrawingKind::Model => "model",
        DrawingKind::Floor => "floor",
    }
}

/// Windows device names that cannot be file names.
fn reserved(seg: &str) -> bool {
    let stem = seg.split('.').next().unwrap_or("").to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.chars().last().is_some_and(|c| c.is_ascii_digit() && c != '0'))
}

/// A relative path that stays inside the project: no drive, no `..`, no empty or odd segments.
pub fn safe_rel(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= 200
        && !p.starts_with('/')
        && p.split('/').all(|seg| {
            !seg.is_empty()
                && seg != "."
                && seg != ".."
                && !seg.ends_with('.')
                && !seg.ends_with(' ')
                && !reserved(seg)
                && seg.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ' | '(' | ')'))
        })
}

/// The path from the folder of `from_file` to `to_file` (both relative to the project root).
pub fn relative_path(from_file: &str, to_file: &str) -> String {
    let from: Vec<&str> = from_file.split('/').collect();
    let to: Vec<&str> = to_file.split('/').collect();
    let from_dir = from.get(..from.len().saturating_sub(1)).unwrap_or(&[]);
    let common = from_dir.iter().zip(to.iter()).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<&str> = vec![".."; from_dir.len().saturating_sub(common)];
    parts.extend(to.get(common..).unwrap_or(&[]));
    parts.join("/")
}

/// The values a naming pattern can use.
struct Vars<'a> {
    project: &'a str,
    set: &'a str,
    floor: &'a str,
    seq: usize,
}

/// A pattern is literal characters safe in a file name plus `{project}`, `{set}`, `{floor}`,
/// `{seq}` and `{seq:0N}`.
fn pattern_ok(p: &str) -> Result<(), String> {
    if p.is_empty() || p.len() > 120 {
        return Err("a pattern must be 1 to 120 characters".into());
    }
    let mut chars = p.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut name = String::new();
            loop {
                match chars.next() {
                    Some('}') => break,
                    Some(c) => name.push(c),
                    None => return Err("a `{` is not closed".into()),
                }
            }
            let ok = matches!(name.as_str(), "project" | "set" | "floor" | "seq")
                || name.strip_prefix("seq:0").is_some_and(|w| w.len() == 1 && w.chars().all(|c| ('1'..='6').contains(&c)));
            if !ok {
                return Err(format!("`{{{name}}}` is not a known field (project, set, floor, seq, seq:03)"));
            }
        } else if !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) {
            return Err(format!("`{c}` cannot be used in a file name pattern"));
        }
    }
    Ok(())
}

fn expand(pattern: &str, v: &Vars) -> String {
    let mut out = String::new();
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c != '{' {
            out.push(c);
            continue;
        }
        let mut name = String::new();
        for c in chars.by_ref() {
            if c == '}' {
                break;
            }
            name.push(c);
        }
        match name.as_str() {
            "project" => out.push_str(v.project),
            "set" => out.push_str(v.set),
            "floor" => out.push_str(v.floor),
            "seq" => out.push_str(&v.seq.to_string()),
            other => {
                if let Some(w) = other.strip_prefix("seq:0").and_then(|w| w.parse::<usize>().ok()) {
                    out.push_str(&format!("{:0w$}", v.seq, w = w));
                }
            }
        }
    }
    out
}

/// Each floor's elevation, bottom to top. An elevation that is given is used as it is; one that is
/// not is the previous elevation plus the previous floor-to-floor height. When neither exists the
/// floor is reported: a missing value is never replaced by a default.
pub fn resolve_elevations(cfg: &ProjectConfig) -> Result<Vec<(String, f64)>, Vec<Issue>> {
    let mut issues = Vec::new();
    let mut out: Vec<(String, f64)> = Vec::new();
    let mut prev: Option<(f64, Option<f64>)> = None;
    for (i, f) in cfg.floors_in_order().into_iter().enumerate() {
        let at = format!("floors[{i}]");
        for (name, v) in
            [("elevation", f.elevation), ("floor_to_floor", f.floor_to_floor), ("clear_height", f.clear_height), ("slab_thickness", f.slab_thickness)]
        {
            if v.is_some_and(|x| !x.is_finite()) {
                issues.push(Issue::new(format!("{at}.{name}"), "must be a finite number"));
            }
        }
        if f.floor_to_floor.is_some_and(|h| h.is_finite() && h <= 0.0) {
            issues.push(Issue::new(format!("{at}.floor_to_floor"), "must be greater than zero"));
        }
        let z = match (f.elevation.filter(|x| x.is_finite()), prev) {
            (Some(z), _) => Some(z),
            (None, Some((pz, Some(h)))) if h.is_finite() && h > 0.0 => Some(pz + h),
            (None, Some(_)) => {
                issues.push(Issue::new(
                    format!("{at}.elevation"),
                    format!("`{}` has no elevation and the floor below has no floor-to-floor height to derive it from", f.name),
                ));
                None
            }
            (None, None) => {
                issues.push(Issue::new(
                    format!("{at}.elevation"),
                    format!("`{}` is the lowest floor: it needs an elevation relative to the datum", f.name),
                ));
                None
            }
        };
        if let Some(z) = z {
            if let Some((last, _)) = out.last().map(|(_, z)| (*z, ()))
                && z <= last
            {
                issues.push(Issue::new(format!("{at}.elevation"), format!("`{}` at {z} is not above the floor below it ({last})", f.name)));
            }
            out.push((f.id.clone(), z));
            prev = Some((z, f.floor_to_floor.filter(|h| h.is_finite())));
        } else {
            prev = None;
        }
    }
    if issues.is_empty() { Ok(out) } else { Err(issues) }
}

fn set_enabled_floor(cfg: &ProjectConfig, set: &DrawingSet, floor_id: &str) -> bool {
    set.enabled && cfg.matrix.iter().find(|c| c.floor_id == floor_id && c.set_id == set.id).is_none_or(|c| c.create)
}

/// The structural checks (everything that can be said without expanding the plan).
fn check_config(cfg: &ProjectConfig) -> Vec<Issue> {
    let mut v: Vec<Issue> = Vec::new();
    let mut add = |path: &str, msg: String| v.push(Issue::new(path, msg));
    if cfg.schema_version == 0 || cfg.schema_version > super::SCHEMA_VERSION {
        add("schema_version", format!("{} is not supported (this build reads up to {})", cfg.schema_version, super::SCHEMA_VERSION));
    }
    let p = &cfg.project;
    if !is_code(&p.id) {
        add("project.id", "is missing or has characters other than letters, digits, - and _".into());
    }
    if !is_code(&p.code) {
        add("project.code", "is required: letters, digits, - and _ only (it is used in file names)".into());
    }
    if p.name.trim().is_empty() {
        add("project.name", "is required".into());
    }
    if !matches!(p.units.length.as_str(), "mm" | "m") {
        add("project.units.length", format!("`{}` is not supported: use mm or m", p.units.length));
    }
    if cfg.floors.is_empty() || cfg.floors.len() > MAX_FLOORS {
        add("floors", format!("a project needs 1 to {MAX_FLOORS} floors, this has {}", cfg.floors.len()));
    }
    let (mut ids, mut names, mut codes, mut seqs) = (HashSet::new(), HashSet::new(), HashSet::new(), HashSet::new());
    for (i, f) in cfg.floors.iter().enumerate() {
        if !is_code(&f.id) || !ids.insert(f.id.clone()) {
            add(&format!("floors[{i}].id"), "is missing, has unusual characters or repeats another floor".into());
        }
        if f.name.trim().is_empty() || !names.insert(f.name.trim().to_lowercase()) {
            add(&format!("floors[{i}].name"), "is empty or repeats another floor".into());
        }
        if !is_code(&f.code) || !codes.insert(f.code.to_lowercase()) {
            add(&format!("floors[{i}].code"), "is missing, has unusual characters or repeats another floor".into());
        }
        if !seqs.insert(f.sequence) {
            add(&format!("floors[{i}].sequence"), format!("{} is used by another floor", f.sequence));
        }
    }
    if cfg.drawing_sets.len() > MAX_SETS {
        add("drawing_sets", format!("at most {MAX_SETS} sets"));
    }
    let (mut sids, mut scodes) = (HashSet::new(), HashSet::new());
    for (i, s) in cfg.drawing_sets.iter().enumerate() {
        if !is_code(&s.id) || !sids.insert(s.id.clone()) {
            add(&format!("drawing_sets[{i}].id"), "is missing, has unusual characters or repeats another set".into());
        }
        if !is_code(&s.code) || !scodes.insert(s.code.to_lowercase()) {
            add(&format!("drawing_sets[{i}].code"), "is missing, has unusual characters or repeats another set".into());
        }
        if s.name.trim().is_empty() {
            add(&format!("drawing_sets[{i}].name"), "is required".into());
        }
        if !s.folder.is_empty() && !safe_rel(&s.folder) {
            add(&format!("drawing_sets[{i}].folder"), format!("`{}` is not a safe folder name", s.folder));
        }
    }
    for (i, c) in cfg.matrix.iter().enumerate() {
        if !ids.contains(&c.floor_id) || !sids.contains(&c.set_id) {
            add(&format!("matrix[{i}]"), "names a floor or set that does not exist".into());
        }
    }
    let n = &cfg.naming_rules;
    for (name, pat) in [
        ("drawing_pattern", &n.drawing_pattern),
        ("model_pattern", &n.model_pattern),
        ("master_pattern", &n.master_pattern),
        ("number_pattern", &n.number_pattern),
    ] {
        if let Err(e) = pattern_ok(pat) {
            add(&format!("naming_rules.{name}"), e);
        }
    }
    if !matches!(n.file_extension.as_str(), "dxf" | "dwg") {
        add("naming_rules.file_extension", format!("`{}` is not supported: use dxf or dwg", n.file_extension));
    }
    if cfg.xref_rules.len() > MAX_RULES {
        add("xref_rules", format!("at most {MAX_RULES} rules"));
    }
    for (i, r) in cfg.xref_rules.iter().enumerate() {
        for (side, sel) in [("host", &r.host), ("target", &r.target)] {
            if sel.kind != DrawingKind::Master && !scodes.contains(&sel.set.to_lowercase()) {
                add(&format!("xref_rules[{i}].{side}.set"), format!("`{}` is not the code of a drawing set", sel.set));
            }
        }
    }
    for (i, f) in cfg.folders.iter().enumerate() {
        if !safe_rel(f) {
            add(&format!("folders[{i}]"), format!("`{f}` is not a safe folder path"));
        }
    }
    v
}

fn matches_selector(sel: &Selector, d: &PlannedDrawing, sets: &HashMap<&str, &DrawingSet>) -> bool {
    if sel.kind != d.kind {
        return false;
    }
    sel.kind == DrawingKind::Master || d.set_id.as_deref().and_then(|s| sets.get(s)).is_some_and(|s| s.code.eq_ignore_ascii_case(&sel.set))
}

/// Expand a configuration that passed [`check_config`] into folders, drawings and Xrefs.
fn build(cfg: &ProjectConfig) -> Plan {
    let proj = cfg.project.code.as_str();
    let ext = cfg.naming_rules.file_extension.as_str();
    let mut drawings: Vec<PlannedDrawing> = Vec::new();
    let mut folders: Vec<String> = Vec::new();
    let add_folder = |f: String, folders: &mut Vec<String>| {
        if !folders.contains(&f) {
            folders.push(f);
        }
    };
    for f in &cfg.folders {
        add_folder(f.clone(), &mut folders);
    }
    if cfg.folders.is_empty() {
        for f in ["project-assets/logos", "project-assets/references", "sheets/layouts", "sheets/exports", "backups"] {
            add_folder(f.to_string(), &mut folders);
        }
        if !cfg.schedules.is_empty() {
            add_folder("schedules/openings".into(), &mut folders);
        }
    }
    let mut files = vec![MANIFEST_FILE.to_string()];
    if cfg.master_drawing {
        add_folder("drawings/00_Master".into(), &mut folders);
        let name = expand(&cfg.naming_rules.master_pattern, &Vars { project: proj, set: "MASTER", floor: "", seq: 1 });
        drawings.push(PlannedDrawing {
            id: derived_id("drw", &[&cfg.project.id, "master", "", ""]),
            kind: DrawingKind::Master,
            set_id: None,
            floor_id: None,
            rel_path: format!("drawings/00_Master/{name}.{ext}"),
            number: expand(&cfg.naming_rules.number_pattern, &Vars { project: proj, set: "MASTER", floor: "", seq: 1 }),
            title: "Master".into(),
        });
    }
    let floors = cfg.floors_in_order();
    for set in cfg.drawing_sets.iter().filter(|s| s.enabled) {
        let folder = format!("drawings/{}", if set.folder.is_empty() { set.name.trim() } else { set.folder.as_str() });
        add_folder(folder.clone(), &mut folders);
        let mut seq = 0usize;
        if set.has_model {
            seq += 1;
            let name = expand(&cfg.naming_rules.model_pattern, &Vars { project: proj, set: &set.code, floor: "", seq });
            drawings.push(PlannedDrawing {
                id: derived_id("drw", &[&cfg.project.id, kind_name(DrawingKind::Model), &set.id, ""]),
                kind: DrawingKind::Model,
                set_id: Some(set.id.clone()),
                floor_id: None,
                rel_path: format!("{folder}/{name}.{ext}"),
                number: expand(&cfg.naming_rules.number_pattern, &Vars { project: proj, set: &set.code, floor: "", seq }),
                title: format!("{} Model", set.name),
            });
        }
        if set.floor_policy == FloorPolicy::PerFloor {
            for fl in floors.iter().filter(|f| set_enabled_floor(cfg, set, &f.id)) {
                seq += 1;
                let vars = Vars { project: proj, set: &set.code, floor: &fl.code, seq };
                drawings.push(PlannedDrawing {
                    id: derived_id("drw", &[&cfg.project.id, kind_name(DrawingKind::Floor), &set.id, &fl.id]),
                    kind: DrawingKind::Floor,
                    set_id: Some(set.id.clone()),
                    floor_id: Some(fl.id.clone()),
                    rel_path: format!("{folder}/{}.{ext}", expand(&cfg.naming_rules.drawing_pattern, &vars)),
                    number: expand(&cfg.naming_rules.number_pattern, &vars),
                    title: format!("{} {}", fl.name, set.name),
                });
            }
        }
    }
    if !cfg.schedules.is_empty() {
        files.push("schedules/openings/opening_schedule.json".into());
        files.push("schedules/openings/opening_schedule.csv".into());
    }
    // Rules become explicit edges.
    let sets: HashMap<&str, &DrawingSet> = cfg.drawing_sets.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut xrefs: Vec<PlannedXref> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for rule in &cfg.xref_rules {
        for host in drawings.iter().filter(|d| matches_selector(&rule.host, d, &sets)) {
            for target in drawings.iter().filter(|d| matches_selector(&rule.target, d, &sets)) {
                if host.id == target.id || (rule.target.same_floor && (host.floor_id.is_none() || host.floor_id != target.floor_id)) {
                    continue;
                }
                if seen.insert((host.id.clone(), target.id.clone())) {
                    xrefs.push(PlannedXref {
                        id: derived_id("xrf", &[&host.id, &target.id]),
                        host_id: host.id.clone(),
                        target_id: target.id.clone(),
                        attachment: rule.attachment,
                        relative_path: relative_path(&host.rel_path, &target.rel_path),
                    });
                }
            }
        }
    }
    let elevations = resolve_elevations(cfg).unwrap_or_default();
    Plan { folders, drawings, xrefs, files, elevations }
}

/// Checks that need the expanded plan: how many drawings, names that collide, Xref cycles.
fn check_plan(plan: &Plan) -> Vec<Issue> {
    let mut v = Vec::new();
    if plan.drawings.len() > MAX_DRAWINGS {
        v.push(Issue::new("drawings", format!("{} drawings is more than the limit of {MAX_DRAWINGS}", plan.drawings.len())));
        return v;
    }
    let mut paths: HashMap<String, &str> = HashMap::new();
    for d in &plan.drawings {
        if !safe_rel(&d.rel_path) {
            v.push(Issue::new(&d.rel_path, "is not a safe path inside the project"));
        }
        if let Some(other) = paths.insert(d.rel_path.to_lowercase(), &d.title) {
            v.push(Issue::new(&d.rel_path, format!("is the file name of both `{other}` and `{}`: change the naming pattern", d.title)));
        }
    }
    // Cycles: Kahn's algorithm (no recursion, whatever the graph).
    let index: HashMap<&str, usize> = plan.drawings.iter().enumerate().map(|(i, d)| (d.id.as_str(), i)).collect();
    let mut indegree = vec![0usize; plan.drawings.len()];
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); plan.drawings.len()];
    for x in &plan.xrefs {
        if let (Some(&h), Some(&t)) = (index.get(x.host_id.as_str()), index.get(x.target_id.as_str())) {
            if let Some(o) = out.get_mut(t) {
                o.push(h);
            }
            if let Some(d) = indegree.get_mut(h) {
                *d += 1;
            }
        }
    }
    let mut queue: Vec<usize> = indegree.iter().enumerate().filter(|(_, d)| **d == 0).map(|(i, _)| i).collect();
    let mut done = 0usize;
    while let Some(n) = queue.pop() {
        done += 1;
        for &m in out.get(n).map(Vec::as_slice).unwrap_or(&[]) {
            if let Some(d) = indegree.get_mut(m) {
                *d = d.saturating_sub(1);
                if *d == 0 {
                    queue.push(m);
                }
            }
        }
    }
    if done < plan.drawings.len() {
        let stuck: Vec<&str> = indegree
            .iter()
            .enumerate()
            .filter(|(_, d)| **d > 0)
            .filter_map(|(i, _)| plan.drawings.get(i).map(|d| d.rel_path.as_str()))
            .take(6)
            .collect();
        v.push(Issue::new("xref_rules", format!("the Xref rules make drawings reference each other in a circle, including {}", stuck.join(", "))));
    }
    v
}

/// Everything wrong with the configuration; empty when [`plan`] will succeed.
pub fn validate(cfg: &ProjectConfig) -> Vec<Issue> {
    let mut v = check_config(cfg);
    v.extend(cfg.typography.issues());
    if let Err(e) = resolve_elevations(cfg) {
        v.extend(e);
    }
    if v.is_empty() {
        v.extend(check_plan(&build(cfg)));
    }
    v
}

/// The plan for a valid configuration, or every issue that stops it.
pub fn plan(cfg: &ProjectConfig) -> Result<Plan, Vec<Issue>> {
    let issues = validate(cfg);
    if issues.is_empty() { Ok(build(cfg)) } else { Err(issues) }
}

/// What is already on disk where the project would be created (read-only).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Preflight {
    pub root_exists: bool,
    pub root_is_empty: bool,
    /// Planned files that already exist: generation must not overwrite them silently.
    pub existing_files: Vec<String>,
}

pub fn preflight(root: &std::path::Path, plan: &Plan) -> Preflight {
    let mut p = Preflight { root_exists: root.is_dir(), ..Preflight::default() };
    p.root_is_empty = p.root_exists && std::fs::read_dir(root).map(|mut d| d.next().is_none()).unwrap_or(false);
    if p.root_exists {
        let planned = plan.files.iter().cloned().chain(plan.drawings.iter().map(|d| d.rel_path.clone()));
        p.existing_files = planned.filter(|f| root.join(f).exists()).collect();
    }
    p
}
