//! External references (Xrefs): another drawing shown inside this one without being copied into it.
//!
//! An xref is a block whose `xref_path` names its source file. `xref.attach` reads that file
//! (flattened to 2D), copies its model-space objects into the block, and puts the block in the
//! drawing. The source file stays the owner of the geometry: `xref.reload` reads it again and
//! replaces the block's content, so an edit made in the source file shows up here.
//!
//! - Layers of the xref are brought in as `<name>|<layer>`, so they can be shown, frozen, locked or
//!   left out of a plot like any other layer without touching the source file. Their flags are kept
//!   across a reload.
//! - Nested block definitions are copied as `<name>|<block>`.
//! - Paths are stored relative to the drawing that attaches them when it has been saved, so a
//!   project folder can be moved or backed up and keep its references.
//! - If the source drawing uses other units than this one, the reference is inserted scaled by the
//!   ratio, never by rescaling its geometry.
//! - An *overlay* (`overlay: true`) is a reference that exists only for this session, to see
//!   another drawing while working: it is never written when the drawing is saved.
//!
//! `xref.bind` makes the reference an ordinary block (`<name>$0$<layer>` layers) and cuts the link;
//! `xref.detach` removes it. Not implemented: clipping, fading and per-reference layer overrides
//! beyond the layer table, and xrefs nested inside an xref (a nested reference is copied as the
//! objects it showed when its owner was last saved).
#![cfg_attr(target_arch = "wasm32", allow(dead_code, unused_imports))]

use std::collections::BTreeSet;
use std::sync::Arc;

use aad_doc::{Block, Drawing, Entity, EntityKind, Insert, Layer, Space};
use aad_geom::Vec3;
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

/// Prefix of the blocks and layers of a session-only overlay.
pub const OVERLAY: &str = "OVL|";
/// Objects copied from one xref, at most (a hostile or runaway file).
const MAX_ENTITIES: usize = 3_000_000;
const MAX_BLOCKS: usize = 5000;

/// Whether `name` (a block or layer name) belongs to a session-only overlay.
pub fn is_overlay(name: &str) -> bool {
    name.starts_with(OVERLAY)
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("xref.attach", "Attach Xref", run_attach)
            .menu(&["Insert", "External Reference", "Attach"])
            .alias(&["xattach", "xa"])
            .params(
                "{path, name?, at?: [x,y] (default 0,0), scale? (default 1), rotation?: degrees, overlay?: bool} → {name, entities, layers, path}",
            ),
        CommandSpec::new("xref.reload", "Reload Xrefs", run_reload)
            .menu(&["Insert", "External Reference", "Reload"])
            .alias(&["xreload"])
            .params("{name?: one xref (default all)} → {reloaded, missing}"),
        CommandSpec::new("xref.detach", "Detach Xref", run_detach)
            .menu(&["Insert", "External Reference", "Detach"])
            .alias(&["xdetach"])
            .params("{name} → {removed}"),
        CommandSpec::new("xref.bind", "Bind Xref", run_bind)
            .menu(&["Insert", "External Reference", "Bind"])
            .alias(&["xbind"])
            .params("{name} → {block, layers}"),
        CommandSpec::new("xref.list", "List Xrefs", run_list)
            .menu(&["Insert", "External Reference", "List"])
            .alias(&["xref", "xlist"])
            .params("{} → {xrefs: [{name, path, resolved, status, entities, overlay}]}")
            .noundo(),
    ]
}

// ---------------- paths ----------------

#[cfg(not(target_arch = "wasm32"))]
mod paths {
    use std::path::{Component, Path, PathBuf};

    fn clean(p: &Path) -> PathBuf {
        let mut out = PathBuf::new();
        for c in p.components() {
            match c {
                Component::CurDir => {}
                Component::ParentDir => {
                    if !out.pop() {
                        out.push("..");
                    }
                }
                other => out.push(other.as_os_str()),
            }
        }
        out
    }

    /// `to` written relative to the directory `from`; `None` when they share no root (another drive).
    pub fn relative(from: &Path, to: &Path) -> Option<String> {
        let (from, to) = (clean(from), clean(to));
        let (f, t): (Vec<_>, Vec<_>) = (from.components().collect(), to.components().collect());
        let common = f.iter().zip(&t).take_while(|(a, b)| a == b).count();
        if common == 0 {
            return None;
        }
        let mut out: Vec<String> = Vec::new();
        for _ in common..f.len() {
            out.push("..".into());
        }
        for c in &t[common..] {
            out.push(c.as_os_str().to_string_lossy().to_string());
        }
        Some(out.join("/"))
    }

    /// The file a stored path names: absolute as is; relative to the host drawing's folder, then to
    /// the working folder.
    pub fn resolve(host: Option<&str>, stored: &str) -> Option<PathBuf> {
        let p = Path::new(stored);
        if p.is_absolute() {
            return p.is_file().then(|| p.to_path_buf());
        }
        let beside = host.and_then(|h| Path::new(h).parent().map(|d| clean(&d.join(p))));
        beside.filter(|b| b.is_file()).or_else(|| std::fs::canonicalize(p).ok().filter(|c| c.is_file()))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn relative_paths_go_up_and_down() {
            assert_eq!(relative(Path::new("/p/proj"), Path::new("/p/proj/Floors/G.dxf")).as_deref(), Some("Floors/G.dxf"));
            assert_eq!(relative(Path::new("/p/proj/Floors"), Path::new("/p/proj/Layouts/G/B.dxf")).as_deref(), Some("../Layouts/G/B.dxf"));
            assert_eq!(relative(Path::new("/a"), Path::new("/a/./b/../c.dxf")).as_deref(), Some("c.dxf"));
        }
    }
}

// ---------------- reading a source drawing ----------------

#[cfg(not(target_arch = "wasm32"))]
fn read_source(cmd: &str, path: &std::path::Path) -> Result<Drawing> {
    let hooks = super::file::io().ok_or_else(|| bad(cmd, "file formats are not available in this build"))?;
    let meta = std::fs::metadata(path).map_err(|e| bad(cmd, format!("{}: {e}", path.display())))?;
    if meta.len() > 512 * 1024 * 1024 {
        return Err(bad(cmd, "the file is too large to attach"));
    }
    let bytes = std::fs::read(path).map_err(|e| bad(cmd, format!("{}: {e}", path.display())))?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let mut d = (hooks.read)(&bytes, &name).map_err(|e| bad(cmd, e))?;
    crate::flatten::flatten_drawing(&mut d);
    Ok(d)
}

/// Host-drawing units per source-drawing unit; 1 (and a note) when either is unknown.
fn unit_ratio(host: &Drawing, source: &Drawing) -> (f64, Option<String>) {
    match (crate::annot::drawing_unit(host), crate::annot::drawing_unit(source)) {
        (Some(h), Some(s)) if h.metres() > 0.0 => (s.metres() / h.metres(), None),
        _ => (1.0, Some("the units of this drawing or of the xref are not set, so the xref is not scaled".into())),
    }
}

fn clean_name(raw: &str) -> String {
    let s: String = raw.chars().map(|c| if c.is_alphanumeric() || matches!(c, '_' | '-' | ' ') { c } else { '_' }).collect();
    s.trim().chars().take(60).collect()
}

fn remap_blocks(kind: &mut EntityKind, f: &mut dyn FnMut(&str) -> String) {
    match kind {
        EntityKind::Insert(i) => i.block = f(&i.block),
        EntityKind::Dimension(d) => {
            if let Some(b) = &d.block {
                d.block = Some(f(b));
            }
        }
        EntityKind::MLeader(m) => {
            if let Some(i) = &mut m.block {
                i.block = f(&i.block);
            }
        }
        _ => {}
    }
}

struct Loaded {
    entities: usize,
    layers: usize,
}

/// Replace the content of the xref block `name` with the model space of `source`.
fn load_into(host: &mut Drawing, name: &str, source: &Drawing, path: &str) -> Result<Loaded> {
    // Layers: `<name>|<layer>`. A layer that is already there (a reload) keeps its flags.
    let mut layers = 0;
    for l in &source.layers {
        let new = format!("{name}|{}", l.name);
        match host.layer_mut(&new) {
            Some(existing) => {
                existing.color = l.color;
                existing.lineweight = l.lineweight;
                existing.linetype = l.linetype.clone();
            }
            None => {
                host.layers.push(Layer { name: new, on: l.on, frozen: l.frozen, locked: l.locked, plot: l.plot, ..l.clone() });
                layers += 1;
            }
        }
    }
    for lt in &source.linetypes {
        if host.linetype(&lt.name).is_none() {
            host.linetypes.push(lt.clone());
        }
    }

    // Block definitions the source's objects use, copied under `<name>|<block>` (bounded).
    let mut blocks: Vec<(String, Block)> = Vec::new();
    let mut copied: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<(String, usize)> = Vec::new();
    let mut count = 0usize;
    let mut block = Block::new(name);
    block.xref_path = Some(path.to_string());
    let mut note_block = |b: &str, depth: usize, queue: &mut Vec<(String, usize)>| -> String {
        if copied.insert(b.to_ascii_uppercase()) && depth < aad_doc::MAX_BLOCK_DEPTH {
            queue.push((b.to_string(), depth + 1));
        }
        format!("{name}|{b}")
    };
    for e in source.model.iter() {
        count += 1;
        if count > MAX_ENTITIES {
            return Err(bad("xref.attach", "the xref has too many objects"));
        }
        let mut kind = e.kind.clone();
        remap_blocks(&mut kind, &mut |b| note_block(b, 0, &mut queue));
        let mut common = e.common.clone();
        common.layer = format!("{name}|{}", common.layer);
        block.entities.push(Entity { handle: host.new_handle(), common, kind });
    }
    while let Some((b, depth)) = queue.pop() {
        if blocks.len() >= MAX_BLOCKS {
            break;
        }
        let Some(def) = source.block(&b) else { continue };
        let mut nb = Block::new(&format!("{name}|{}", def.name));
        nb.base = def.base;
        nb.description = def.description.clone();
        nb.anonymous = def.anonymous;
        nb.explodable = def.explodable;
        nb.units = def.units;
        for e in def.entities.iter() {
            count += 1;
            if count > MAX_ENTITIES {
                return Err(bad("xref.attach", "the xref has too many objects"));
            }
            let mut kind = e.kind.clone();
            remap_blocks(&mut kind, &mut |x| note_block(x, depth, &mut queue));
            let mut common = e.common.clone();
            // Block content on layer 0 follows the layer of the insert, as it always does.
            if common.layer != "0" {
                common.layer = format!("{name}|{}", common.layer);
                host.ensure_layer(&common.layer);
            }
            nb.entities.push(Entity { handle: host.new_handle(), common, kind });
        }
        blocks.push((nb.name.clone(), nb));
    }
    let n = block.entities.len();
    for (k, b) in blocks {
        host.blocks.insert(k, Arc::new(b));
    }
    // Replace an earlier version of the block, keeping the same name.
    host.blocks.insert(name.to_string(), Arc::new(block));
    Ok(Loaded { entities: n, layers })
}

// ---------------- commands ----------------

fn xref_blocks(d: &Drawing) -> Vec<(String, String)> {
    d.blocks
        .iter()
        .filter_map(|(k, b)| b.xref_path.as_ref().map(|p| (k.clone(), p.clone())))
        .filter(|(k, _)| !k.contains('|') || is_overlay(k) && k.matches('|').count() == 1)
        .collect()
}

fn run_attach(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        return Err(bad("xref.attach", "files cannot be read on the web"));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let raw = str_param(p, "path").ok_or_else(|| bad("xref.attach", "`path` is required"))?;
        let host_path = s.state()?.path.clone();
        let file = paths::resolve(host_path.as_deref(), raw).ok_or_else(|| bad("xref.attach", format!("`{raw}` is not a file")))?;
        let overlay = bool_or(p, "overlay", false);
        let stem = file.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Xref".into());
        let base = clean_name(str_param(p, "name").unwrap_or(&stem));
        if base.is_empty() {
            return Err(bad("xref.attach", "the name has no usable characters"));
        }
        let name = if overlay { format!("{OVERLAY}{base}") } else { base };
        if s.doc()?.blocks.keys().any(|k| k.eq_ignore_ascii_case(&name)) {
            return Err(bad("xref.attach", format!("`{name}` is already in the drawing; use xref.reload, or another name")));
        }
        let at = point_param(p, "at").unwrap_or(aad_geom::Vec2::ZERO);
        let scale = f64_or(p, "scale", 1.0);
        let rot = f64_or(p, "rotation", 0.0);
        if !(scale.is_finite() && scale > 0.0 && rot.is_finite()) {
            return Err(bad("xref.attach", "`scale` must be positive and `rotation` a number"));
        }
        let source = read_source("xref.attach", &file)?;
        // Stored relative to the host when it has a folder, so the project can be moved.
        let stored = match host_path.as_deref().and_then(|h| std::path::Path::new(h).parent().map(std::path::Path::to_path_buf)) {
            Some(dir) => paths::relative(&dir, &file).unwrap_or_else(|| file.display().to_string()),
            None => file.display().to_string(),
        };
        let (ratio, note) = unit_ratio(s.doc()?, &source);
        let loaded = load_into(s.doc_mut()?, &name, &source, &stored)?;
        let sc = ratio * scale;
        let kind = EntityKind::Insert(Insert {
            block: name.clone(),
            insert: Vec3::new(at.x, at.y, 0.0),
            scale: Vec3::new(sc, sc, sc),
            rotation: rot.to_radians(),
            ..Insert::default()
        });
        let layer = if overlay { format!("{name}|0") } else { "0".to_string() };
        let common = aad_doc::Common { layer, ..Default::default() };
        s.doc_mut()?.add(&Space::Model, common, kind)?;
        let mut out = json!({ "name": name, "entities": loaded.entities, "layers": loaded.layers, "path": stored, "overlay": overlay, "scale": sc });
        if let Some(n) = note {
            out["warning"] = json!(n);
        }
        Ok(out)
    }
}

fn run_reload(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        return Err(bad("xref.reload", "files cannot be read on the web"));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let host_path = s.state()?.path.clone();
        let only = str_param(p, "name").map(str::to_string);
        let list: Vec<(String, String)> =
            xref_blocks(s.doc()?).into_iter().filter(|(k, _)| only.as_ref().is_none_or(|o| k.eq_ignore_ascii_case(o))).collect();
        if let Some(o) = &only
            && list.is_empty()
        {
            return Err(bad("xref.reload", format!("no xref named `{o}`")));
        }
        let (mut reloaded, mut missing) = (Vec::new(), Vec::new());
        for (name, stored) in list {
            let Some(file) = paths::resolve(host_path.as_deref(), &stored) else {
                missing.push(json!({ "name": name, "path": stored }));
                continue;
            };
            let source = read_source("xref.reload", &file)?;
            let l = load_into(s.doc_mut()?, &name, &source, &stored)?;
            reloaded.push(json!({ "name": name, "entities": l.entities }));
        }
        let msg = format!("{} xref(s) reloaded, {} missing", reloaded.len(), missing.len());
        Ok(json!({ "reloaded": reloaded, "missing": missing, "message": msg }))
    }
}

fn run_detach(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("xref.detach", "`name` is required"))?.to_string();
    let d = s.doc_mut()?;
    let key = d
        .blocks
        .keys()
        .find(|k| k.eq_ignore_ascii_case(&name) && d.blocks[*k].xref_path.is_some())
        .cloned()
        .ok_or_else(|| bad("xref.detach", format!("no xref named `{name}`")))?;
    let prefix = format!("{key}|");
    let mut removed = 0usize;
    for space in std::iter::once(Space::Model).chain(d.layouts.iter().map(|l| Space::Paper(l.name.clone())).collect::<Vec<_>>()) {
        let doomed: Vec<_> = d
            .space(&space)
            .map(|st| {
                st.iter().filter(|e| matches!(&e.kind, EntityKind::Insert(i) if i.block.eq_ignore_ascii_case(&key))).map(|e| e.handle).collect()
            })
            .unwrap_or_default();
        for h in doomed {
            d.remove_entity(h);
            removed += 1;
        }
    }
    d.blocks.retain(|k, _| !(k == &key || k.starts_with(&prefix)));
    d.layers.retain(|l| !l.name.starts_with(&prefix));
    Ok(json!({ "removed": removed, "message": format!("Detached {key} ({removed} reference(s) removed)") }))
}

fn run_bind(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("xref.bind", "`name` is required"))?.to_string();
    let d = s.doc_mut()?;
    let key = d
        .blocks
        .keys()
        .find(|k| k.eq_ignore_ascii_case(&name) && d.blocks[*k].xref_path.is_some())
        .cloned()
        .ok_or_else(|| bad("xref.bind", format!("no xref named `{name}`")))?;
    if is_overlay(&key) {
        return Err(bad("xref.bind", "an overlay is only for looking; attach the drawing as an xref to bind it"));
    }
    let prefix = format!("{key}|");
    let bound = format!("{key}$0$");
    let rename = |n: &str| -> String { n.strip_prefix(&prefix).map_or_else(|| n.to_string(), |rest| format!("{bound}{rest}")) };
    // Refuse before changing anything if a target name is taken.
    for l in d.layers.iter().filter(|l| l.name.starts_with(&prefix)) {
        if d.layer(&rename(&l.name)).is_some() {
            return Err(bad("xref.bind", format!("layer `{}` already exists", rename(&l.name))));
        }
    }
    for b in d.blocks.keys().filter(|k| k.starts_with(&prefix)) {
        if d.blocks.contains_key(&rename(b)) {
            return Err(bad("xref.bind", format!("block `{}` already exists", rename(b))));
        }
    }
    let mut layers = 0;
    for l in d.layers.iter_mut().filter(|l| l.name.starts_with(&prefix)) {
        l.name = rename(&l.name);
        layers += 1;
    }
    let names: Vec<String> = d.blocks.keys().filter(|k| **k == key || k.starts_with(&prefix)).cloned().collect();
    for old in names {
        let Some(arc) = d.blocks.remove(&old) else { continue };
        let mut b = Arc::try_unwrap(arc).unwrap_or_else(|a| (*a).clone());
        let mut store = aad_doc::EntityStore::new();
        for e in b.entities.iter() {
            let mut e: Entity = (**e).clone();
            if e.common.layer.starts_with(&prefix) {
                e.common.layer = rename(&e.common.layer);
            }
            remap_blocks(&mut e.kind, &mut |x| rename(x));
            store.push(e);
        }
        b.entities = store;
        if old == key {
            b.xref_path = None;
            b.explodable = true;
        } else {
            b.name = rename(&old);
        }
        d.blocks.insert(if old == key { old.clone() } else { rename(&old) }, Arc::new(b));
    }
    // References to the bound block's nested blocks were renamed above; the references to the
    // block itself keep their name.
    Ok(json!({ "block": key, "layers": layers, "message": format!("Bound {key} as a block ({layers} layer(s) renamed to {bound}...)") }))
}

fn run_list(s: &mut Session, _p: &Value) -> Result<Value> {
    let host_path = s.state()?.path.clone();
    let d = s.doc()?;
    let mut inserts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for e in d.model.iter() {
        if let EntityKind::Insert(i) = &e.kind {
            *inserts.entry(i.block.to_ascii_uppercase()).or_insert(0) += 1;
        }
    }
    let out: Vec<Value> = xref_blocks(d)
        .into_iter()
        .map(|(name, stored)| {
            #[cfg(not(target_arch = "wasm32"))]
            let resolved = paths::resolve(host_path.as_deref(), &stored).map(|p| p.display().to_string());
            #[cfg(target_arch = "wasm32")]
            let resolved: Option<String> = {
                let _ = &host_path;
                None
            };
            let entities = d.blocks.get(&name).map_or(0, |b| b.entities.len());
            json!({
                "name": name, "path": stored, "resolved": resolved, "status": if resolved.is_some() { "found" } else { "missing" },
                "entities": entities, "overlay": is_overlay(&name), "references": inserts.get(&name.to_ascii_uppercase()).copied().unwrap_or(0),
            })
        })
        .collect();
    Ok(json!({ "xrefs": out }))
}

/// The drawing as it is written to disk: without the session-only overlays (their blocks, layers
/// and references). Borrowed when there are none.
pub fn without_overlays(d: &Drawing) -> std::borrow::Cow<'_, Drawing> {
    let has = d.blocks.keys().any(|k| is_overlay(k)) || d.layers.iter().any(|l| is_overlay(&l.name));
    if !has {
        return std::borrow::Cow::Borrowed(d);
    }
    let mut c = d.clone();
    let spaces: Vec<Space> =
        std::iter::once(Space::Model).chain(c.layouts.iter().map(|l| Space::Paper(l.name.clone())).collect::<Vec<_>>()).collect();
    for space in spaces {
        let doomed: Vec<_> = c
            .space(&space)
            .map(|st| st.iter().filter(|e| matches!(&e.kind, EntityKind::Insert(i) if is_overlay(&i.block))).map(|e| e.handle).collect())
            .unwrap_or_default();
        for h in doomed {
            c.remove_entity(h);
        }
    }
    c.blocks.retain(|k, _| !is_overlay(k));
    c.layers.retain(|l| !is_overlay(&l.name));
    std::borrow::Cow::Owned(c)
}
