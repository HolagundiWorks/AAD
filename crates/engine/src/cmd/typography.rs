//! Typography commands: the font presets and roles, an inventory of the fonts a drawing uses, and a
//! migration of its text styles to a preset's fonts with a preview first.
//!
//! The migration changes only a text style's `font` field. Height, width factor, obliquing,
//! backwards/upside-down flags, alignment, rotation and insertion points are never touched, so the
//! geometry of the drawing stays as it is. Styles whose purpose cannot be told from how they are used
//! are listed as unclassified and left alone until the caller assigns a role. One migration is one
//! undo step, so it can be rolled back.

use cadcraft_doc::{Drawing, EntityKind, Space};
use cadcraft_fonts::registry::{self, Face, Role, Typography};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("fonts.presets", "Font Presets", run_presets)
            .params("{} → {presets: [{id, name, description, families, installed, license, upstream, note}], roles: [{id, face, weight}]} (which preset fonts are installed on this machine)")
            .noundo(),
        CommandSpec::new("fonts.resolve", "Resolve Font Roles", run_resolve)
            .params("{preset?: id (default engineering)} → {preset, roles: [{role, family, weight, requested, substituted}]} (a missing family falls back and is reported as substituted)")
            .noundo(),
        CommandSpec::new("fonts.scan", "Scan Fonts", run_scan)
            .params("{} → {styles: [{name, font, installed, usage, role, reason}], unclassified, missingFonts} (the text styles of the drawing, where they are used, whether their font is installed, and the role each most likely has)")
            .noundo(),
        CommandSpec::new("fonts.migrate", "Migrate Fonts", run_migrate)
            .params("{preset?: id, roles?: {styleName: role}, apply?: bool} → {changes: [{style, from, to, role}], skipped, warnings, applied} (apply false previews; apply true changes the `font` of the classified styles only, as one undo step)"),
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
        .map(|r| json!({"role": r.role.id(), "family": r.family, "weight": r.weight, "requested": r.requested, "substituted": r.substituted}))
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
    let mut visit = |e: &cadcraft_doc::Entity| match &e.kind {
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
        let stroke =
            ts.font.is_empty() || ts.font.eq_ignore_ascii_case(cadcraft_fonts::BUILTIN_FONT) || ts.font.to_ascii_lowercase().ends_with(".shx");
        let installed = stroke || cadcraft_fonts::ttf::find(&ts.font).is_some();
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

fn run_migrate(s: &mut Session, p: &Value) -> Result<Value> {
    let t = parse_preset(p, "fonts.migrate")?;
    let apply = p.get("apply").and_then(Value::as_bool).unwrap_or(false);
    let overrides: std::collections::BTreeMap<String, Role> = match p.get("roles") {
        None | Some(Value::Null) => Default::default(),
        Some(Value::Object(o)) => {
            let mut m = std::collections::BTreeMap::new();
            for (k, v) in o {
                let role = v
                    .as_str()
                    .and_then(Role::from_id)
                    .ok_or_else(|| bad("fonts.migrate", format!("`{k}`: unknown role (use {})", registry::ROLES.map(|r| r.id()).join(", "))))?;
                m.insert(k.to_ascii_lowercase(), role);
            }
            m
        }
        Some(_) => return Err(bad("fonts.migrate", "`roles` must be an object of style name to role")),
    };
    let (mut changes, mut skipped, mut warnings) = (Vec::new(), Vec::new(), Vec::new());
    let mut edits: Vec<(String, String)> = Vec::new();
    {
        let d = s.doc()?;
        let usage = usage_of(d);
        for ts in &d.text_styles {
            let u = usage.get(&ts.name.to_ascii_lowercase()).cloned().unwrap_or_default();
            let role = overrides.get(&ts.name.to_ascii_lowercase()).copied().or_else(|| guess_role(&ts.name, &u).map(|(r, _)| r));
            let Some(role) = role else {
                skipped.push(json!({"style": ts.name, "reason": "its purpose cannot be told: assign a role in `roles` to include it"}));
                continue;
            };
            let r = t.resolve(role);
            if r.substituted {
                // Never swap in a different font than the preset asked for: report it and leave the style as it is.
                skipped.push(json!({"style": ts.name, "reason": format!("`{}` is not installed on this machine (it would fall back to `{}`)", r.requested, r.family)}));
                continue;
            }
            if r.family.eq_ignore_ascii_case(&ts.font) {
                skipped.push(json!({"style": ts.name, "reason": "already uses this font"}));
                continue;
            }
            // Characters the new font lacks would be drawn from a fallback font or not at all.
            let text: String = u.samples.concat();
            let missing = registry::missing_glyphs(&r.family, &text);
            if !missing.is_empty() {
                warnings
                    .push(json!({"style": ts.name, "message": format!("`{}` has no glyph for {:?}", r.family, missing.iter().collect::<String>())}));
            }
            changes.push(json!({"style": ts.name, "from": ts.font, "to": r.family, "role": role.id(), "weight": r.weight, "uses": u.total()}));
            edits.push((ts.name.clone(), r.family));
        }
    }
    if !changes.is_empty() {
        warnings.push(json!({"message": "Only the font name changes; heights, width factors, obliquing and positions are kept. Text widths differ between fonts, so check dense schedules and title blocks for overlaps."}));
    }
    if apply {
        let d = s.doc_mut()?;
        for (name, family) in &edits {
            if let Some(ts) = d.text_styles.iter_mut().find(|ts| &ts.name == name) {
                ts.font = family.clone();
            }
        }
    }
    Ok(json!({"preset": t.preset, "changes": changes, "skipped": skipped, "warnings": warnings, "applied": apply && !edits.is_empty()}))
}
