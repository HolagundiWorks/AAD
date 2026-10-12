//! Active Drawing Space: named, reusable layer-state configurations (see docs/ACTIVE_DRAWING_SPACE.md).
//!
//! A configuration stores, for each layer it lists, whether the layer is visible (`on`), frozen and
//! locked, plus an optional current layer. Activating one writes those three flags into the
//! drawing's layer table, which stays the only live source of truth; the engine's lock and freeze
//! rules (`protect.rs`) then apply to the result. Activating never touches an object, a block, a
//! layer name or a layer's colour, linetype or lineweight. "Editable" is derived (not frozen and not
//! locked), never stored, so a configuration cannot disagree with itself.
//!
//! Configurations live in the drawing's project data under `drawingSpaces` and so travel with the
//! file (DXF extension data) and take part in undo. Layers are referenced by name (layers have no
//! other stable identifier): renaming a layer with `layer.set newName` follows the rename in every
//! configuration, a layer that was deleted is reported as `missing` and skipped, and a layer created
//! later is left alone on activation unless the configuration's `unlisted` policy says otherwise.

use aad_doc::Drawing;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

const KEY: &str = "drawingSpaces";
const MAX_SPACES: usize = 200;
const MAX_ENTRIES: usize = 10_000;
const MAX_NAME: usize = 64;
/// A change touching more layers than this carries a warning in its result.
const MANY: usize = 25;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
struct Entry {
    layer: String,
    visible: bool,
    frozen: bool,
    locked: bool,
}

impl Default for Entry {
    fn default() -> Self {
        Entry { layer: String::new(), visible: true, frozen: false, locked: false }
    }
}

impl Entry {
    fn editable(&self) -> bool {
        !self.frozen && !self.locked
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
struct Space {
    id: String,
    name: String,
    description: String,
    /// Floor this configuration is meant for; `None` is every floor.
    floor: Option<String>,
    layers: Vec<Entry>,
    current: Option<String>,
    /// What activation does to layers the configuration does not list: `keep` (default), `hide`, `lock`.
    unlisted: String,
    created: u64,
    modified: u64,
    version: u32,
}

impl Default for Space {
    fn default() -> Self {
        Space {
            id: String::new(),
            name: String::new(),
            description: String::new(),
            floor: None,
            layers: Vec::new(),
            current: None,
            unlisted: "keep".into(),
            created: 0,
            modified: 0,
            version: 1,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
struct Store {
    version: u32,
    active: Option<String>,
    next: u32,
    spaces: Vec<Space>,
}

fn now_secs() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}

fn load(d: &Drawing) -> Store {
    let v = super::project::get(d, KEY);
    let mut s: Store = serde_json::from_value(v).unwrap_or_default();
    s.spaces.truncate(MAX_SPACES);
    for sp in &mut s.spaces {
        sp.layers.truncate(MAX_ENTRIES);
    }
    s
}

fn save(d: &mut Drawing, s: &mut Store) {
    s.version = 1;
    super::project::set(d, KEY, &serde_json::to_value(&*s).unwrap_or(Value::Null));
}

/// Follow `layer.set newName`: every configuration that lists `old` now lists `new`.
pub(crate) fn follow_rename(d: &mut Drawing, old: &str, new: &str) {
    if !d.project.contains_key(KEY) {
        return;
    }
    let mut s = load(d);
    let mut changed = false;
    for sp in &mut s.spaces {
        for e in &mut sp.layers {
            if e.layer.eq_ignore_ascii_case(old) {
                e.layer = new.to_string();
                changed = true;
            }
        }
        if sp.current.as_deref().is_some_and(|c| c.eq_ignore_ascii_case(old)) {
            sp.current = Some(new.to_string());
            changed = true;
        }
    }
    if changed {
        save(d, &mut s);
    }
}

fn entry_of(l: &aad_doc::Layer) -> Entry {
    Entry { layer: l.name.clone(), visible: l.on, frozen: l.frozen, locked: l.locked }
}

fn find(s: &Store, p: &Value, cmd: &str) -> Result<usize> {
    let id = str_param(p, "id");
    let name = str_param(p, "name");
    s.spaces
        .iter()
        .position(|sp| id.is_some_and(|i| sp.id == i) || (id.is_none() && name.is_some_and(|n| sp.name.eq_ignore_ascii_case(n))))
        .ok_or_else(|| bad(cmd, "no such drawing space (give `id` or `name`)"))
}

fn valid_name(n: &str) -> bool {
    let t = n.trim();
    !t.is_empty() && t.chars().count() <= MAX_NAME && !t.chars().any(char::is_control)
}

fn summary(sp: &Space, active: bool) -> Value {
    json!({
        "id": sp.id, "name": sp.name, "description": sp.description, "floor": sp.floor, "layers": sp.layers.len(),
        "current": sp.current, "unlisted": sp.unlisted, "created": sp.created, "modified": sp.modified, "active": active,
    })
}

fn full(sp: &Space, d: &Drawing) -> Value {
    let rows: Vec<Value> = sp
        .layers
        .iter()
        .map(|e| json!({"layer": e.layer, "visible": e.visible, "frozen": e.frozen, "locked": e.locked, "editable": e.editable(), "exists": d.layer(&e.layer).is_some()}))
        .collect();
    let mut v = summary(sp, false);
    v["entries"] = Value::Array(rows);
    v["version"] = json!(sp.version);
    v
}

/// What activating `sp` would change: (layer, field, from, to). Missing layers are returned apart.
fn plan(d: &Drawing, sp: &Space) -> (Vec<(String, &'static str, bool, bool)>, Vec<String>) {
    let mut changes = Vec::new();
    let mut missing = Vec::new();
    let mut listed = std::collections::HashSet::new();
    for e in &sp.layers {
        let Some(l) = d.layer(&e.layer) else {
            missing.push(e.layer.clone());
            continue;
        };
        listed.insert(l.name.to_ascii_lowercase());
        for (field, from, to) in [("visible", l.on, e.visible), ("frozen", l.frozen, e.frozen), ("locked", l.locked, e.locked)] {
            if from != to {
                changes.push((l.name.clone(), field, from, to));
            }
        }
    }
    for l in &d.layers {
        if listed.contains(&l.name.to_ascii_lowercase()) {
            continue;
        }
        match sp.unlisted.as_str() {
            "hide" if l.on => changes.push((l.name.clone(), "visible", true, false)),
            "lock" if !l.locked => changes.push((l.name.clone(), "locked", false, true)),
            _ => {}
        }
    }
    (changes, missing)
}

fn changes_json(c: &[(String, &'static str, bool, bool)]) -> Value {
    Value::Array(c.iter().map(|(l, f, a, b)| json!({"layer": l, "field": f, "from": a, "to": b})).collect())
}

/// Differences between the active configuration and the live layer table.
fn differences(d: &Drawing, sp: &Space) -> Vec<(String, &'static str, bool, bool)> {
    plan(d, sp).0
}

/// `check` are the entries just supplied (they must name existing layers); `all` is the whole list
/// (no duplicates; the current layer must be usable in it). Entries already stored for layers that
/// were deleted since are tolerated: they are reported as `missing` on activation.
fn validate_entries(d: &Drawing, cmd: &str, check: &[Entry], all: &[Entry], current: Option<&str>) -> Result<()> {
    if all.len() > MAX_ENTRIES {
        return Err(bad(cmd, format!("at most {MAX_ENTRIES} layers per drawing space")));
    }
    for e in check {
        if d.layer(&e.layer).is_none() {
            return Err(bad(cmd, format!("no layer `{}`", e.layer)));
        }
    }
    let mut seen = std::collections::HashSet::new();
    for e in all {
        if !seen.insert(e.layer.to_ascii_lowercase()) {
            return Err(bad(cmd, format!("layer `{}` is listed twice", e.layer)));
        }
    }
    if let Some(c) = current {
        if d.layer(c).is_none() {
            return Err(bad(cmd, format!("the current layer `{c}` does not exist")));
        }
        if let Some(e) = all.iter().find(|e| e.layer.eq_ignore_ascii_case(c))
            && (e.frozen || e.locked || !e.visible)
        {
            return Err(bad(cmd, format!("the current layer `{c}` must be visible, thawed and unlocked in the same configuration")));
        }
    }
    Ok(())
}

fn parse_entries(p: &Value, cmd: &str) -> Result<Option<Vec<Entry>>> {
    let Some(arr) = p.get("layers") else { return Ok(None) };
    let arr = arr.as_array().ok_or_else(|| bad(cmd, "`layers` must be a list of {layer, visible?, frozen?, locked?}"))?;
    if arr.len() > MAX_ENTRIES {
        return Err(bad(cmd, format!("at most {MAX_ENTRIES} layers per drawing space")));
    }
    let mut out = Vec::new();
    for v in arr {
        let layer = v.get("layer").and_then(Value::as_str).ok_or_else(|| bad(cmd, "each layer entry needs `layer`"))?.to_string();
        let flag = |k: &str, d: bool| v.get(k).and_then(Value::as_bool).unwrap_or(d);
        out.push(Entry { layer, visible: flag("visible", true), frozen: flag("frozen", false), locked: flag("locked", false) });
    }
    Ok(Some(out))
}

fn unlisted_param(p: &Value, cmd: &str) -> Result<Option<String>> {
    match str_param(p, "unlisted") {
        None => Ok(None),
        Some(u @ ("keep" | "hide" | "lock")) => Ok(Some(u.to_string())),
        Some(_) => Err(bad(cmd, "`unlisted` is keep, hide or lock")),
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("space.list", "Drawing Spaces", run_list)
            .menu(&["Format", "Drawing Space"])
            .alias(&["spaces"])
            .params("{floor?: only spaces for this floor and for every floor} → {active, dirty, spaces}")
            .noundo(),
        CommandSpec::new("space.get", "Drawing Space Details", run_get)
            .params("{id | name} → the configuration with a row per layer (visible, frozen, locked, editable, exists)")
            .noundo(),
        CommandSpec::new("space.save", "Save Drawing Space", run_save)
            .params("{name, description?, floor?, current?: layer | true (the current layer), unlisted?: keep|hide|lock, layers?: [{layer, visible?, frozen?, locked?}] (default: every layer as it is now), activate?: bool (default true)} → {space}"),
        CommandSpec::new("space.update", "Edit Drawing Space", run_update)
            .params("{id | name, newName?, description?, floor?, current?, unlisted?, layers?: [{layer, visible?, frozen?, locked?}] (merged by layer), remove?: [layer]} edits the saved configuration only; the drawing is not changed"),
        CommandSpec::new("space.capture", "Update Drawing Space from Layers", run_capture)
            .params("{id | name} stores every layer's current visible, frozen and locked state in the configuration"),
        CommandSpec::new("space.activate", "Activate Drawing Space", run_activate)
            .params("{id | name} → {space, changed: [{layer, field, from, to}], missing, warnings, current}")
            .alias(&["ads"]),
        CommandSpec::new("space.preview", "Preview Drawing Space", run_preview)
            .params("{id | name} → the changes activation would make; nothing is changed")
            .noundo(),
        CommandSpec::new("space.duplicate", "Duplicate Drawing Space", run_duplicate).params("{id | name, newName?} → {space}"),
        CommandSpec::new("space.delete", "Delete Drawing Space", run_delete).params("{id | name}"),
        CommandSpec::new("space.status", "Drawing Space Status", run_status)
            .params("{} → {active: {id, name} | null, dirty, differences: [{layer, field, saved, now}]}")
            .noundo(),
        CommandSpec::new("space.recover", "Show, Thaw and Unlock All Layers", run_recover)
            .menu(&["Format", "Drawing Space", "Show, Thaw and Unlock All"])
            .params("{} → {changed} makes every layer visible, thawed and unlocked (the safe way out of a bad configuration)"),
    ]
}

fn run_list(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc()?;
    let st = load(d);
    let floor = str_param(p, "floor");
    let active = st.active.clone();
    let dirty = active.as_ref().and_then(|a| st.spaces.iter().find(|sp| &sp.id == a)).is_some_and(|sp| !differences(d, sp).is_empty());
    let spaces: Vec<Value> = st
        .spaces
        .iter()
        .filter(|sp| floor.is_none_or(|f| sp.floor.as_deref().is_none_or(|sf| sf.eq_ignore_ascii_case(f))))
        .map(|sp| summary(sp, active.as_deref() == Some(sp.id.as_str())))
        .collect();
    Ok(json!({"active": active, "dirty": dirty, "spaces": spaces}))
}

fn run_get(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc()?;
    let st = load(d);
    let i = find(&st, p, "space.get")?;
    let mut v = full(&st.spaces[i], d);
    v["active"] = json!(st.active.as_deref() == Some(st.spaces[i].id.as_str()));
    Ok(v)
}

fn run_save(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").map(str::trim).ok_or_else(|| bad("space.save", "`name` is required"))?.to_string();
    if !valid_name(&name) {
        return Err(bad("space.save", format!("a name of 1 to {MAX_NAME} characters without control characters")));
    }
    let explicit = parse_entries(p, "space.save")?;
    let unlisted = unlisted_param(p, "space.save")?.unwrap_or_else(|| "keep".into());
    let d = s.doc_mut()?;
    let mut st = load(d);
    if st.spaces.len() >= MAX_SPACES {
        return Err(bad("space.save", format!("at most {MAX_SPACES} drawing spaces")));
    }
    if st.spaces.iter().any(|sp| sp.name.eq_ignore_ascii_case(&name)) {
        return Err(bad("space.save", format!("a drawing space named `{name}` already exists")));
    }
    let layers = explicit.unwrap_or_else(|| d.layers.iter().map(entry_of).collect());
    let current = match p.get("current") {
        Some(Value::Bool(true)) => Some(d.header.str("CLAYER", "0")),
        Some(Value::String(c)) if !c.is_empty() => Some(c.clone()),
        _ => None,
    };
    validate_entries(d, "space.save", &layers, &layers, current.as_deref())?;
    st.next += 1;
    let now = now_secs();
    let sp = Space {
        id: format!("ds-{}", st.next),
        name,
        description: str_param(p, "description").unwrap_or("").chars().take(500).collect(),
        floor: str_param(p, "floor").map(str::to_string),
        layers,
        current,
        unlisted,
        created: now,
        modified: now,
        version: 1,
    };
    let id = sp.id.clone();
    let out = full(&sp, d);
    st.spaces.push(sp);
    if bool_or(p, "activate", true) {
        st.active = Some(id);
    }
    save(d, &mut st);
    Ok(json!({"space": out}))
}

fn run_update(s: &mut Session, p: &Value) -> Result<Value> {
    let patch = parse_entries(p, "space.update")?;
    let unlisted = unlisted_param(p, "space.update")?;
    let d = s.doc_mut()?;
    let mut st = load(d);
    let i = find(&st, p, "space.update")?;
    let mut sp = st.spaces[i].clone();
    if let Some(n) = str_param(p, "newName").map(str::trim) {
        if !valid_name(n) {
            return Err(bad("space.update", format!("a name of 1 to {MAX_NAME} characters without control characters")));
        }
        if st.spaces.iter().enumerate().any(|(j, o)| j != i && o.name.eq_ignore_ascii_case(n)) {
            return Err(bad("space.update", format!("a drawing space named `{n}` already exists")));
        }
        sp.name = n.to_string();
    }
    if let Some(t) = p.get("description").and_then(Value::as_str) {
        sp.description = t.chars().take(500).collect();
    }
    if let Some(f) = p.get("floor") {
        sp.floor = f.as_str().filter(|x| !x.is_empty()).map(str::to_string);
    }
    if let Some(u) = unlisted {
        sp.unlisted = u;
    }
    if let Some(c) = p.get("current") {
        sp.current = c.as_str().filter(|x| !x.is_empty()).map(str::to_string);
    }
    let checked: Vec<Entry> = patch.clone().unwrap_or_default();
    if let Some(patch) = patch {
        for e in patch {
            match sp.layers.iter_mut().find(|x| x.layer.eq_ignore_ascii_case(&e.layer)) {
                Some(x) => *x = e,
                None => sp.layers.push(e),
            }
        }
    }
    if let Some(Value::Array(rm)) = p.get("remove") {
        let names: Vec<String> = rm.iter().filter_map(Value::as_str).map(str::to_ascii_lowercase).collect();
        sp.layers.retain(|e| !names.contains(&e.layer.to_ascii_lowercase()));
    }
    validate_entries(d, "space.update", &checked, &sp.layers, sp.current.as_deref())?;
    sp.modified = now_secs();
    let out = full(&sp, d);
    st.spaces[i] = sp;
    save(d, &mut st);
    Ok(json!({"space": out}))
}

fn run_capture(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc_mut()?;
    let mut st = load(d);
    let i = find(&st, p, "space.capture")?;
    let layers: Vec<Entry> = d.layers.iter().map(entry_of).collect();
    st.spaces[i].layers = layers;
    st.spaces[i].modified = now_secs();
    let out = full(&st.spaces[i], d);
    save(d, &mut st);
    Ok(json!({"space": out}))
}

fn run_preview(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc()?;
    let st = load(d);
    let i = find(&st, p, "space.preview")?;
    let (changes, missing) = plan(d, &st.spaces[i]);
    Ok(json!({"space": summary(&st.spaces[i], false), "changed": changes_json(&changes), "missing": missing, "many": changes.len() > MANY}))
}

fn run_activate(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc_mut()?;
    let mut st = load(d);
    let i = find(&st, p, "space.activate")?;
    let sp = st.spaces[i].clone();
    let (changes, missing) = plan(d, &sp);

    // Work out the layer table that results, so the current layer can be checked before anything is applied.
    let mut layers = d.layers.clone();
    for (name, field, _, to) in &changes {
        if let Some(l) = layers.iter_mut().find(|l| l.name == *name) {
            match *field {
                "visible" => l.on = *to,
                "frozen" => l.frozen = *to,
                _ => l.locked = *to,
            }
        }
    }
    let usable = |l: &aad_doc::Layer| l.on && !l.frozen && !l.locked;
    let cur = d.header.str("CLAYER", "0");
    let mut warnings = Vec::new();
    let mut new_current: Option<String> = None;
    if let Some(c) = &sp.current {
        match layers.iter().find(|l| l.name.eq_ignore_ascii_case(c)) {
            Some(l) if usable(l) => new_current = Some(l.name.clone()),
            Some(_) => warnings.push(format!("the configured current layer `{c}` is not usable here and was not set")),
            None => warnings.push(format!("the configured current layer `{c}` no longer exists")),
        }
    }
    let now_cur = new_current.clone().unwrap_or_else(|| cur.clone());
    if let Some(l) = layers.iter().find(|l| l.name.eq_ignore_ascii_case(&now_cur)) {
        if l.frozen {
            // A frozen layer cannot be current: move to the first usable layer of the configuration, else "0".
            let fallback = sp
                .layers
                .iter()
                .filter_map(|e| layers.iter().find(|l| l.name.eq_ignore_ascii_case(&e.layer)))
                .find(|l| usable(l))
                .or_else(|| layers.iter().find(|l| l.name == "0" && usable(l)))
                .map(|l| l.name.clone())
                .ok_or_else(|| bad("space.activate", "this configuration would leave no layer to draw on; nothing was changed"))?;
            warnings.push(format!("`{}` is frozen in this configuration, so the current layer is now `{fallback}`", l.name));
            new_current = Some(fallback);
        } else if l.locked || !l.on {
            warnings.push(format!(
                "the current layer `{}` is {}: nothing can be drawn on it until it is changed",
                l.name,
                if l.locked { "locked" } else { "hidden" }
            ));
        }
    }
    if changes.len() > MANY {
        warnings.push(format!("{} layer settings were changed", changes.len()));
    }

    d.layers = layers;
    if let Some(c) = &new_current {
        d.header.set_str("CLAYER", c);
    }
    st.active = Some(sp.id.clone());
    save(d, &mut st);
    let current = d.header.str("CLAYER", "0");
    Ok(json!({
        "space": summary(&sp, true), "changed": changes_json(&changes), "missing": missing, "warnings": warnings, "current": current,
        "message": format!("Active drawing space: {}", sp.name),
    }))
}

fn run_duplicate(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc_mut()?;
    let mut st = load(d);
    if st.spaces.len() >= MAX_SPACES {
        return Err(bad("space.duplicate", format!("at most {MAX_SPACES} drawing spaces")));
    }
    let i = find(&st, p, "space.duplicate")?;
    let mut sp = st.spaces[i].clone();
    let name = str_param(p, "newName").map(|n| n.trim().to_string()).unwrap_or_else(|| format!("{} copy", sp.name));
    if !valid_name(&name) || st.spaces.iter().any(|o| o.name.eq_ignore_ascii_case(&name)) {
        return Err(bad("space.duplicate", format!("`{name}` is not a valid new name, or it already exists")));
    }
    st.next += 1;
    sp.id = format!("ds-{}", st.next);
    sp.name = name;
    sp.created = now_secs();
    sp.modified = sp.created;
    let out = full(&sp, d);
    st.spaces.push(sp);
    save(d, &mut st);
    Ok(json!({"space": out}))
}

fn run_delete(s: &mut Session, p: &Value) -> Result<Value> {
    let d = s.doc_mut()?;
    let mut st = load(d);
    let i = find(&st, p, "space.delete")?;
    let sp = st.spaces.remove(i);
    if st.active.as_deref() == Some(sp.id.as_str()) {
        st.active = None;
    }
    save(d, &mut st);
    Ok(json!({"deleted": sp.name}))
}

fn run_status(s: &mut Session, _p: &Value) -> Result<Value> {
    let d = s.doc()?;
    let st = load(d);
    let Some(sp) = st.active.as_ref().and_then(|a| st.spaces.iter().find(|sp| &sp.id == a)) else {
        return Ok(json!({"active": Value::Null, "dirty": false, "differences": []}));
    };
    let diffs: Vec<Value> =
        differences(d, sp).into_iter().map(|(l, f, now, saved)| json!({"layer": l, "field": f, "saved": saved, "now": now})).collect();
    Ok(json!({"active": {"id": sp.id, "name": sp.name}, "dirty": !diffs.is_empty(), "differences": diffs}))
}

fn run_recover(s: &mut Session, _p: &Value) -> Result<Value> {
    let d = s.doc_mut()?;
    let mut n = 0;
    for l in &mut d.layers {
        if !l.on || l.frozen || l.locked {
            n += 1;
        }
        l.on = true;
        l.frozen = false;
        l.locked = false;
    }
    Ok(json!({"changed": n, "message": format!("{n} layers shown, thawed and unlocked")}))
}

/// The active drawing space and whether the layers differ from it, for read-only context
/// (`cmd::context`): `None` when no space is active.
pub(crate) fn active_summary(d: &Drawing) -> Option<Value> {
    let st = load(d);
    let sp = st.active.as_ref().and_then(|a| st.spaces.iter().find(|sp| &sp.id == a))?;
    Some(json!({"name": sp.name, "floor": sp.floor, "layersInSpace": sp.layers.len(), "layersDifferFromSpace": !differences(d, sp).is_empty()}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_validated() {
        assert!(valid_name("Working Drawings"));
        assert!(!valid_name("   "));
        assert!(!valid_name(&"x".repeat(MAX_NAME + 1)));
        assert!(!valid_name("a\u{7}b"));
    }

    #[test]
    fn a_corrupt_store_loads_as_empty_not_a_crash() {
        let mut d = Drawing::default();
        d.project.insert(KEY.into(), "{not json".into());
        assert!(load(&d).spaces.is_empty());
        d.project.insert(KEY.into(), json!({"spaces": "nope"}).to_string());
        assert!(load(&d).spaces.is_empty());
    }
}
