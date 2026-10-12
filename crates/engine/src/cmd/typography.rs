//! Typography commands: the font presets and roles, an inventory of the fonts a drawing uses, and a
//! migration of its text styles to a preset's fonts with a preview first.
//!
//! The migration changes only a text style's `font` field. Height, width factor, obliquing,
//! backwards/upside-down flags, alignment, rotation and insertion points are never touched, so the
//! geometry of the drawing stays as it is. Styles whose purpose cannot be told from how they are used
//! are listed as unclassified and left alone until the caller assigns a role. One migration is one
//! undo step, so it can be rolled back.

use aad_doc::{Drawing, EntityKind, Space};
use aad_fonts::registry::{self, Face, Role, Typography};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("fonts.presets", "Font Presets", run_presets)
            .params("{} → {presets: [{id, name, description, families, installed, license, upstream, note}], roles: [{id, face, weight}]} (which preset fonts are installed on this machine)")
            .noundo(),
        CommandSpec::new("fonts.resolve", "Resolve Font Roles", run_resolve)
            .params("{preset?: id (default engineering)} → {preset, roles: [{role, family, weight, requested, substituted}]} (a missing family falls back and is reported as substituted; weightInstalled false means the role is drawn at Regular)")
            .noundo(),
        CommandSpec::new("fonts.scan", "Scan Fonts", run_scan)
            .params("{} → {styles: [{name, font, installed, usage, role, reason}], unclassified, missingFonts} (the text styles of the drawing, where they are used, whether their font is installed, and the role each most likely has)")
            .noundo(),
        CommandSpec::new("fonts.migrate", "Migrate Fonts", run_migrate)
            .params("{preset?: id, roles?: {styleName: role}, apply?: bool, root?: project folder} → {changes: [{style, from, to, role}], skipped, warnings, growth: [{handle, text, oldWidth, newWidth, overlaps}], applied} (apply false previews; apply true changes the `font` of the classified styles only, as one undo step; with root it runs over the project drawings, backs each changed one up to backups/fonts-N/, writes report.json and returns migrationId)"),
        CommandSpec::new("fonts.rollback", "Roll Back Font Migration", run_rollback)
            .params("{root: project folder, id: migrationId} → {restored, left} (restores the backed-up drawings; a drawing edited since the migration is left as it is)")
            .noundo(),
    ]
}

fn parse_preset(p: &Value, cmd: &str) -> Result<Typography> {
    let id = p.get("preset").and_then(Value::as_str).unwrap_or(registry::DEFAULT_PRESET);
    if registry::preset(id).is_none() {
        return Err(bad(cmd, format!("unknown preset `{id}`: use one of {}", registry::PRESETS.iter().map(|p| p.id).collect::<Vec<_>>().join(", "))));
    }
    Ok(Typography { preset: id.into(), overrides: Vec::new() })
}

fn face_name(f: Face) -> &'static str {
    match f {
        Face::Sans => "sans",
        Face::Condensed => "condensed",
        Face::Mono => "mono",
    }
}

fn run_presets(_s: &mut Session, _p: &Value) -> Result<Value> {
    let presets: Vec<Value> = registry::PRESETS
        .iter()
        .map(|p| {
            json!({
                "id": p.id, "name": p.name, "description": p.description,
                "families": {"sans": p.sans, "condensed": p.condensed, "mono": p.mono, "serif": p.serif},
                "installed": {"sans": registry::installed(p.sans), "condensed": registry::installed(p.condensed), "mono": registry::installed(p.mono)},
                "license": p.license, "upstream": p.upstream, "note": p.note,
            })
        })
        .collect();
    let roles: Vec<Value> = registry::ROLES
        .iter()
        .map(|r| {
            let (f, w) = r.default_face();
            json!({"id": r.id(), "face": face_name(f), "weight": w})
        })
        .collect();
    Ok(json!({"presets": presets, "roles": roles, "default": registry::DEFAULT_PRESET}))
}

fn run_resolve(_s: &mut Session, p: &Value) -> Result<Value> {
    let t = parse_preset(p, "fonts.resolve")?;
    let roles: Vec<Value> = t
        .resolve_all()
        .into_iter()
        .map(|r| json!({"role": r.role.id(), "family": r.family, "weight": r.weight, "requested": r.requested, "substituted": r.substituted, "weightInstalled": r.weight_installed}))
        .collect();
    Ok(json!({"preset": t.preset, "roles": roles}))
}

/// How many places use each text style.
#[derive(Default, Clone)]
struct Usage {
    text: usize,
    mtext: usize,
    dim_styles: usize,
    leader_styles: usize,
    /// A few example strings, for the glyph check.
    samples: Vec<String>,
}

impl Usage {
    fn total(&self) -> usize {
        self.text + self.mtext + self.dim_styles + self.leader_styles
    }
    fn json(&self) -> Value {
        json!({"text": self.text, "mtext": self.mtext, "dimStyles": self.dim_styles, "leaderStyles": self.leader_styles})
    }
}

const MAX_SAMPLES: usize = 40;

fn usage_of(d: &Drawing) -> std::collections::BTreeMap<String, Usage> {
    let mut out: std::collections::BTreeMap<String, Usage> = std::collections::BTreeMap::new();
    let key = |s: &str| s.to_ascii_lowercase();
    let mut visit = |e: &aad_doc::Entity| match &e.kind {
        EntityKind::Text(t) => {
            let u = out.entry(key(&t.style)).or_default();
            u.text += 1;
            if u.samples.len() < MAX_SAMPLES {
                u.samples.push(t.value.clone());
            }
        }
        EntityKind::MText(t) => {
            let u = out.entry(key(&t.style)).or_default();
            u.mtext += 1;
            if u.samples.len() < MAX_SAMPLES {
                u.samples.push(t.contents.clone());
            }
        }
        _ => {}
    };
    for e in d.model.iter() {
        visit(e);
    }
    for l in &d.layouts {
        if let Some(store) = d.space(&Space::Paper(l.name.clone())) {
            for e in store.iter() {
                visit(e);
            }
        }
    }
    for b in d.blocks.values() {
        for e in b.entities.iter() {
            visit(e);
        }
    }
    for ds in &d.dim_styles {
        out.entry(key(&ds.text_style)).or_default().dim_styles += 1;
    }
    for ms in &d.mleader_styles {
        out.entry(key(&ms.text_style)).or_default().leader_styles += 1;
    }
    out
}

/// The role a style most likely has, and why. `None`: cannot be told, so the caller must choose.
fn guess_role(name: &str, u: &Usage) -> Option<(Role, &'static str)> {
    let n = name.to_ascii_lowercase();
    // The styles AAD creates for each role are named for it.
    if let Some(role) = n.strip_prefix("aad-").and_then(Role::from_id) {
        return Some((role, "a style made for this role"));
    }
    let has = |w: &[&str]| w.iter().any(|x| n.contains(x));
    if u.dim_styles > 0 && u.text + u.mtext == 0 || has(&["dim", "level", "elev"]) {
        return Some((Role::Dimension, "used by dimension styles or named for dimensions"));
    }
    if has(&["sched", "table", "tbl"]) {
        return Some((Role::Schedule, "named for schedules or tables"));
    }
    if has(&["mono", "coord", "code", "id"]) && !has(&["ident"]) {
        return Some((Role::Monospace, "named for coordinates, codes or identifiers"));
    }
    if has(&["title", "head", "sheet"]) {
        return Some((Role::Heading, "named for titles or headings"));
    }
    if has(&["name", "company", "firm", "logo"]) && has(&["plate", "block", "company", "firm"]) {
        return Some((Role::Nameplate, "named for the nameplate or title block"));
    }
    if has(&["small", "cond", "narrow", "tb_", "tblk"]) {
        return Some((Role::Condensed, "named for compact text"));
    }
    if n == "standard" || has(&["note", "annot", "general", "text", "room", "label"]) {
        return Some((Role::Primary, "the general annotation style"));
    }
    None
}

fn run_scan(s: &mut Session, _p: &Value) -> Result<Value> {
    let d = s.doc()?;
    let usage = usage_of(d);
    let mut styles = Vec::new();
    let (mut unclassified, mut missing) = (Vec::new(), Vec::new());
    for ts in &d.text_styles {
        let u = usage.get(&ts.name.to_ascii_lowercase()).cloned().unwrap_or_default();
        let stroke = ts.font.is_empty() || aad_fonts::is_builtin_font(&ts.font) || ts.font.to_ascii_lowercase().ends_with(".shx");
        let installed = stroke || aad_fonts::ttf::find(&ts.font).is_some();
        if !installed {
            missing.push(ts.font.clone());
        }
        let guess = guess_role(&ts.name, &u);
        if guess.is_none() {
            unclassified.push(ts.name.clone());
        }
        styles.push(json!({
            "name": ts.name, "font": ts.font, "bigFont": ts.big_font, "stroke": stroke, "installed": installed, "usage": u.json(), "used": u.total() > 0,
            "role": guess.map(|(r, _)| r.id()), "reason": guess.map(|(_, why)| why),
        }));
    }
    missing.sort();
    missing.dedup();
    Ok(json!({"styles": styles, "unclassified": unclassified, "missingFonts": missing}))
}

/// The width of `text` at `height` in the font `name` (the stroke font when it is not a TrueType font).
fn text_width(name: &str, text: &str, height: f64, width_factor: f64) -> f64 {
    match aad_fonts::ttf::find(name) {
        Some(face) => aad_fonts::ttf::width(&face, text, height, width_factor).unwrap_or(0.0),
        None => aad_fonts::line_width(text, height, width_factor),
    }
}

struct Migration {
    changes: Vec<Value>,
    skipped: Vec<Value>,
    warnings: Vec<Value>,
    /// Text that gets wider than it was: it may clip or run into its neighbours.
    growth: Vec<Value>,
    applied: bool,
}

fn parse_roles(p: &Value) -> Result<std::collections::BTreeMap<String, Role>> {
    match p.get("roles") {
        None | Some(Value::Null) => Ok(Default::default()),
        Some(Value::Object(o)) => {
            let mut m = std::collections::BTreeMap::new();
            for (k, v) in o {
                let role = v
                    .as_str()
                    .and_then(Role::from_id)
                    .ok_or_else(|| bad("fonts.migrate", format!("`{k}`: unknown role (use {})", registry::ROLES.map(|r| r.id()).join(", "))))?;
                m.insert(k.to_ascii_lowercase(), role);
            }
            Ok(m)
        }
        Some(_) => Err(bad("fonts.migrate", "`roles` must be an object of style name to role")),
    }
}

/// Plan (and with `apply`, make) the font changes of the active drawing. Only the `font` field of a
/// text style changes. Text that would get wider is reported, with the neighbours it may run into.
fn migrate_active(s: &mut Session, t: &Typography, overrides: &std::collections::BTreeMap<String, Role>, apply: bool) -> Result<Migration> {
    let mut m = Migration { changes: vec![], skipped: vec![], warnings: vec![], growth: vec![], applied: false };
    let mut edits: Vec<(String, String)> = Vec::new();
    {
        let d = s.doc()?;
        let usage = usage_of(d);
        for ts in &d.text_styles {
            let u = usage.get(&ts.name.to_ascii_lowercase()).cloned().unwrap_or_default();
            let role = overrides.get(&ts.name.to_ascii_lowercase()).copied().or_else(|| guess_role(&ts.name, &u).map(|(r, _)| r));
            let Some(role) = role else {
                m.skipped.push(json!({"style": ts.name, "reason": "its purpose cannot be told: assign a role in `roles` to include it"}));
                continue;
            };
            let r = t.resolve(role);
            if r.substituted {
                // Never swap in a different font than the preset asked for: report it and leave the style as it is.
                m.skipped.push(json!({"style": ts.name, "reason": format!("`{}` is not installed on this machine (it would fall back to `{}`)", r.requested, r.family)}));
                continue;
            }
            // The face at the role's weight when it is installed, else the Regular face (said so).
            let weighted = registry::family_at(&r.family, r.weight);
            let shown = if r.weight_installed { weighted } else { r.family.clone() };
            if !r.weight_installed {
                m.warnings.push(json!({"style": ts.name, "message": format!("`{}` is not installed as its own face: this role is drawn at Regular weight here", registry::family_at(&r.family, r.weight))}));
            }
            if shown.eq_ignore_ascii_case(&ts.font) {
                m.skipped.push(json!({"style": ts.name, "reason": "already uses this font"}));
                continue;
            }
            let missing = registry::missing_glyphs(&r.family, &u.samples.concat());
            if !missing.is_empty() {
                m.warnings
                    .push(json!({"style": ts.name, "message": format!("`{}` has no glyph for {:?}", r.family, missing.iter().collect::<String>())}));
            }
            m.changes.push(json!({"style": ts.name, "from": ts.font, "to": shown, "role": role.id(), "weight": r.weight, "uses": u.total()}));
            edits.push((ts.name.clone(), shown));
        }
        // Text that gets wider, measured with the old and the new font (single-line text).
        let new_font: std::collections::HashMap<String, &String> = edits.iter().map(|(n, f)| (n.to_ascii_lowercase(), f)).collect();
        // Left-aligned, unrotated lines as boxes: handle, x, y, new width, height.
        let mut boxes: Vec<(u64, f64, f64, f64, f64)> = Vec::new();
        let mut grown: Vec<(u64, f64, f64, String, String)> = Vec::new();
        for e in d.model.iter() {
            let EntityKind::Text(t) = &e.kind else { continue };
            let Some(style) = d.text_styles.iter().find(|ts| ts.name.eq_ignore_ascii_case(&t.style)) else { continue };
            let h = if t.height > 0.0 { t.height } else { style.height.max(1e-9) };
            let old = text_width(&style.font, &t.value, h, t.width_factor);
            let new = new_font.get(&t.style.to_ascii_lowercase()).map_or(old, |f| text_width(f, &t.value, h, t.width_factor));
            if t.rotation == 0.0 && matches!(t.halign, aad_doc::HAlign::Left) {
                boxes.push((e.handle.0, t.insert.x, t.insert.y, new, h));
            }
            if new > old * 1.05 + 1e-9 && grown.len() < 20 {
                grown.push((e.handle.0, old, new, t.style.clone(), t.value.chars().take(40).collect()));
            }
        }
        for (handle, old, new, style, text) in grown {
            // Which neighbours the widened line now reaches: same baseline band, starting inside it.
            let hits: Vec<u64> = boxes
                .iter()
                .find(|b| b.0 == handle)
                .map(|me| {
                    boxes
                        .iter()
                        .filter(|o| o.0 != me.0 && (o.2 - me.2).abs() < me.4.max(o.4) && o.1 >= me.1 && o.1 < me.1 + me.3)
                        .map(|o| o.0)
                        .take(5)
                        .collect()
                })
                .unwrap_or_default();
            m.growth.push(json!({"handle": handle, "style": style, "text": text, "oldWidth": old, "newWidth": new, "overlaps": hits}));
        }
    }
    if !m.changes.is_empty() {
        m.warnings.push(json!({"message": "Only the font name changes; heights, width factors, obliquing and positions are kept. Text widths differ between fonts: check the listed text and dense schedules and title blocks."}));
    }
    if apply && !edits.is_empty() {
        let d = s.doc_mut()?;
        for (name, family) in &edits {
            if let Some(ts) = d.text_styles.iter_mut().find(|ts| &ts.name == name) {
                ts.font = family.clone();
            }
        }
        m.applied = true;
    }
    Ok(m)
}

fn migration_json(t: &Typography, m: &Migration) -> Value {
    json!({"preset": t.preset, "changes": m.changes, "skipped": m.skipped, "warnings": m.warnings, "growth": m.growth, "applied": m.applied})
}

fn run_migrate(s: &mut Session, p: &Value) -> Result<Value> {
    let t = parse_preset(p, "fonts.migrate")?;
    let apply = p.get("apply").and_then(Value::as_bool).unwrap_or(false);
    let overrides = parse_roles(p)?;
    match p.get("root").and_then(Value::as_str) {
        None => Ok(migration_json(&t, &migrate_active(s, &t, &overrides, apply)?)),
        Some(root) => migrate_project(s, &t, &overrides, apply, std::path::Path::new(root)),
    }
}

fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, x| (h ^ u64::from(*x)).wrapping_mul(0x0100_0000_01b3))
}

/// The drawings of the project at `root`, from its manifest (`project.json`, `generated.drawings`).
fn project_files(root: &std::path::Path) -> Result<Vec<String>> {
    let text =
        std::fs::read_to_string(root.join("project.json")).map_err(|e| bad("fonts.migrate", format!("{}: no project.json ({e})", root.display())))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| bad("fonts.migrate", format!("project.json cannot be read: {e}")))?;
    let list = v["generated"]["drawings"].as_array().ok_or_else(|| bad("fonts.migrate", "the project has no generated drawings"))?;
    Ok(list
        .iter()
        .filter_map(|d| d["rel_path"].as_str())
        .filter(|p| !p.contains("..") && !p.starts_with('/'))
        .map(str::to_string)
        .take(5000)
        .collect())
}

fn abs_of(root: &std::path::Path, rel: &str) -> std::path::PathBuf {
    rel.split('/').fold(root.to_path_buf(), |p, seg| p.join(seg))
}

/// The project's drawings, preview or apply. Applying copies each changed drawing to
/// `backups/fonts-<n>/` first, saves it, and writes `report.json` there with the hash of what was
/// written, so [`run_rollback`] can restore the drawings (and leave alone any edited since).
fn migrate_project(
    s: &mut Session,
    t: &Typography,
    overrides: &std::collections::BTreeMap<String, Role>,
    apply: bool,
    root: &std::path::Path,
) -> Result<Value> {
    let files = project_files(root)?;
    let mut n = 1;
    while root.join("backups").join(format!("fonts-{n:03}")).exists() {
        n += 1;
    }
    let id = format!("fonts-{n:03}");
    let dir = root.join("backups").join(&id);
    let (mut out, mut touched) = (Vec::new(), Vec::new());
    for rel in &files {
        let path = abs_of(root, rel);
        if !path.is_file() {
            out.push(json!({"file": rel, "error": "the file is missing"}));
            continue;
        }
        let r = s.execute("open", &json!({"path": path.to_str().unwrap_or_default()}))?;
        let idx = r["index"].as_u64().map_or(s.active, |i| i as usize);
        let backup = dir.join(rel);
        let outcome = (|| -> Result<Migration> {
            let before = std::fs::read(&path).map_err(|e| bad("fonts.migrate", format!("{rel}: {e}")))?;
            let m = migrate_active(s, t, overrides, apply)?;
            if apply && m.applied {
                if let Some(parent) = backup.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| bad("fonts.migrate", e.to_string()))?;
                }
                std::fs::write(&backup, &before).map_err(|e| bad("fonts.migrate", format!("backup of {rel}: {e}")))?;
                s.execute("qsave", &json!({}))?;
                let after = std::fs::read(&path).map_err(|e| bad("fonts.migrate", format!("{rel}: {e}")))?;
                touched.push(json!({"file": rel, "before": fnv(&before), "after": fnv(&after)}));
            }
            Ok(m)
        })();
        let _ = s.execute("close", &json!({"index": idx}));
        match outcome {
            Ok(m) => out.push(json!({"file": rel, "result": migration_json(t, &m)})),
            Err(e) => out.push(json!({"file": rel, "error": e.to_string()})),
        }
    }
    let mut result = json!({"preset": t.preset, "root": root.display().to_string(), "files": out, "applied": apply && !touched.is_empty()});
    if apply && !touched.is_empty() {
        let report = json!({"id": id, "preset": t.preset, "files": touched, "details": out});
        std::fs::create_dir_all(&dir).map_err(|e| bad("fonts.migrate", e.to_string()))?;
        std::fs::write(dir.join("report.json"), serde_json::to_string_pretty(&report).unwrap_or_default())
            .map_err(|e| bad("fonts.migrate", format!("report: {e}")))?;
        result["migrationId"] = json!(id);
        result["report"] = json!(dir.join("report.json").display().to_string());
    }
    Ok(result)
}

fn run_rollback(_s: &mut Session, p: &Value) -> Result<Value> {
    let root = std::path::PathBuf::from(
        p.get("root").and_then(Value::as_str).ok_or_else(|| bad("fonts.rollback", "`root` (the project folder) is required"))?,
    );
    let id = p.get("id").and_then(Value::as_str).ok_or_else(|| bad("fonts.rollback", "`id` (the migration id from fonts.migrate) is required"))?;
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(bad("fonts.rollback", "`id` is not a migration id"));
    }
    let dir = root.join("backups").join(id);
    let text = std::fs::read_to_string(dir.join("report.json")).map_err(|e| bad("fonts.rollback", format!("no report for `{id}`: {e}")))?;
    let report: Value = serde_json::from_str(&text).map_err(|e| bad("fonts.rollback", format!("the report cannot be read: {e}")))?;
    let (mut restored, mut left) = (Vec::new(), Vec::new());
    for f in report["files"].as_array().cloned().unwrap_or_default() {
        let (Some(rel), Some(after)) = (f["file"].as_str(), f["after"].as_u64()) else { continue };
        if rel.contains("..") || rel.starts_with('/') {
            continue;
        }
        let path = abs_of(&root, rel);
        match std::fs::read(&path) {
            Ok(now) if fnv(&now) == after => match std::fs::copy(dir.join(rel), &path) {
                Ok(_) => restored.push(rel.to_string()),
                Err(e) => left.push(json!({"file": rel, "reason": format!("cannot restore: {e}")})),
            },
            Ok(_) => left.push(json!({"file": rel, "reason": "edited since the migration: left as it is"})),
            Err(_) => left.push(json!({"file": rel, "reason": "the file is missing"})),
        }
    }
    Ok(json!({"restored": restored, "left": left}))
}
