//! Sheets: the paper-space side of a drawing. A layout is a sheet; this module gives it a title
//! block with fields, lets a finished sheet be saved as a reusable template, and lists and edits
//! its viewports. There is no Layout panel; sheet setup is done through these commands.
//!
//! - **Title block** (`sheet.titleblock`): the block `AAD-TITLEBLOCK`, drawn in millimetres of real
//!   paper size and inserted at the lower right of the printable area, with a border around it. Its
//!   fields are block attributes (`PROJECT`, `CLIENT`, `TITLE`, `FLOOR`, `DRAWN`, `DATE`, `SHEET`,
//!   `SCALE`, `REV`). They start from the project details; `sheet.fields` sets them. Any block with
//!   attributes of those tags works, so a template with its own title block is filled the same way.
//! - **Templates** (`sheet.template.*`): everything on the sheet except its viewports, the blocks it
//!   uses, and the page setup, saved by name in the drawing (`scope: drawing`, travels with the
//!   file) or for the user (`scope: user`, one JSON file in the AAD settings folder). Applying a
//!   template sets the page up, places the objects, and replaces what an earlier template put on
//!   that layout, never anything the user drew.
//! - **Viewports** (`viewport.list|add|remove`): the viewports of a layout with their scale, size,
//!   lock and frozen layers.
//!
//! Not implemented: revision tables, automatic sheet numbering across a set, and templates that
//! carry viewports (viewports are added per drawing, since what they show is the drawing's own).

use std::collections::BTreeMap;
use std::sync::Arc;

use aad_doc::{Attrib, Block, Common, Drawing, Entity, EntityKind, HAlign, PageSetup, Space, Text, VAlign};
use aad_geom::{Bounds2, Vec2};
use serde_json::{Map, Value, json};

use super::draw::make_text;
use super::helpers::{line, lwpoly, rect_vertices};
use super::project;
use super::*;
use crate::{Result, Session};

pub const TITLE_BLOCK: &str = "AAD-TITLEBLOCK";
const BORDER_LAYER: &str = "A-ANNO-BORD";
const TITLE_LAYER: &str = "A-ANNO-TTLB";
/// The fields of the standard title block, in the order the panel shows them.
pub const FIELDS: &[(&str, &str)] = &[
    ("PROJECT", "Project"),
    ("CLIENT", "Client"),
    ("TITLE", "Drawing title"),
    ("FLOOR", "Floor"),
    ("DRAWN", "Drawn by"),
    ("DATE", "Date"),
    ("SHEET", "Sheet no."),
    ("SCALE", "Scale"),
    ("REV", "Revision"),
];
const MAX_TEMPLATE_ENTITIES: usize = 20_000;
const MAX_TEMPLATE_BLOCKS: usize = 200;
const MAX_TEMPLATE_BYTES: u64 = 8_000_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("sheet.titleblock", "Title Block", run_titleblock)
            .menu(&["Architecture", "Sheets", "Title Block"])
            .params("{layout?, fields?: {PROJECT, CLIENT, TITLE, FLOOR, DRAWN, DATE, SHEET, SCALE, REV}} → {handle, fields, created}"),
        CommandSpec::new("sheet.fields", "Sheet Fields", run_fields)
            .menu(&["Architecture", "Sheets", "Sheet Fields"])
            .params("{layout?, fields?: {TAG: value}} → {fields: {TAG: value}} (without `fields`: just reads them)"),
        CommandSpec::new("sheet.template.save", "Save Sheet Template", run_tpl_save)
            .menu(&["Architecture", "Sheets", "Save Sheet Template"])
            .params("{name, layout?, scope?: drawing|user, folder? (user scope: a shared folder instead of the settings folder)} → {name, entities, blocks}")
            .noundo(),
        CommandSpec::new("sheet.template.list", "Sheet Templates", run_tpl_list)
            .params("{folder?} → {templates: [{name, scope, paper, entities}]}")
            .noundo(),
        CommandSpec::new("sheet.template.apply", "Apply Sheet Template", run_tpl_apply)
            .menu(&["Architecture", "Sheets", "Apply Sheet Template"])
            .params("{name, layout?, folder?, fields?: {TAG: value}} → {placed, replaced}"),
        CommandSpec::new("sheet.template.delete", "Delete Sheet Template", run_tpl_delete)
            .params("{name, scope?: drawing|user, folder?} → {deleted}")
            .noundo(),
        CommandSpec::new("viewport.list", "List Viewports", run_vp_list).params("{layout?} → {viewports: [{handle, rect, scale, scaleText, locked, frozen}]}").noundo(),
        CommandSpec::new("viewport.add", "Add Viewport", run_vp_add).params("{layout?, p1?, p2? (paper units; default the printable area), scale?: \"1:100\", locked?: bool} → {handle}"),
        CommandSpec::new("viewport.remove", "Remove Viewport", run_vp_remove).params("{handle}"),
    ]
}

// ---------------- helpers ----------------

/// The layout a command acts on: `layout` or the current one (not the Model tab).
fn layout_of(s: &Session, cmd: &str, p: &Value) -> Result<String> {
    if let Some(n) = str_param(p, "layout") {
        return s.doc()?.layout(n).map(|l| l.name.clone()).ok_or_else(|| bad(cmd, format!("no layout named `{n}`")));
    }
    match s.layout_space() {
        Space::Paper(n) => Ok(n),
        Space::Model => Err(bad(cmd, "switch to a layout or pass `layout`")),
    }
}

/// Run `f` with `layout` as the current layout, then go back to where the user was.
fn in_layout<T>(s: &mut Session, layout: &str, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
    let prev = s.layout_space();
    s.execute("layout.set", &json!({ "name": layout }))?;
    let r = f(s);
    let back = match prev {
        Space::Paper(n) => n,
        Space::Model => "Model".to_string(),
    };
    let _ = s.execute("layout.set", &json!({ "name": back }));
    r
}

fn sheet_of(d: &Drawing, layout: &str, cmd: &str) -> Result<aad_render::Sheet> {
    aad_render::sheet(d, layout).ok_or_else(|| bad(cmd, format!("no layout named `{layout}`")))
}

fn scale_text(scale: f64) -> String {
    if !(scale.is_finite() && scale > 0.0) {
        return String::new();
    }
    if scale <= 1.0 {
        let n = 1.0 / scale;
        if (n - n.round()).abs() < 1e-6 * n { format!("1:{}", n.round()) } else { format!("1:{n:.2}") }
    } else if (scale - scale.round()).abs() < 1e-9 {
        format!("{}:1", scale.round())
    } else {
        format!("{scale:.2}:1")
    }
}

fn ensure_layer(d: &mut Drawing, name: &str, aci: i16, desc: &str) {
    super::hcw::ensure_layer(d, name, aci, "Continuous", 25, desc);
}

fn attdef(style: &str, tag: &str, prompt: &str, at: Vec2, height: f64) -> EntityKind {
    EntityKind::AttDef(Attrib {
        tag: tag.to_string(),
        text: Text {
            insert: aad_geom::Vec3::new(at.x, at.y, 0.0),
            align_pt: None,
            height,
            value: String::new(),
            rotation: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            style: style.to_string(),
            halign: HAlign::Left,
            valign: VAlign::Baseline,
        },
        invisible: false,
        constant: false,
        prompt: prompt.to_string(),
        props: Default::default(),
    })
}

/// Make the title block definition if the drawing has none. Drawn in millimetres, its insertion
/// point is the lower right corner of the panel.
fn ensure_title_block(s: &mut Session) -> Result<()> {
    if s.doc()?.block(TITLE_BLOCK).is_some() {
        return Ok(());
    }
    const W: f64 = 180.0;
    const ROW: f64 = 14.0;
    const H: f64 = ROW * 4.0;
    let style = s.doc()?.header.str("TEXTSTYLE", "Standard");
    // (tag, label, x from the left of the panel, width, row from the top)
    let cells: [(&str, &str, f64, f64, usize); 9] = [
        ("PROJECT", "PROJECT", 0.0, 180.0, 0),
        ("CLIENT", "CLIENT", 0.0, 90.0, 1),
        ("TITLE", "DRAWING TITLE", 90.0, 90.0, 1),
        ("FLOOR", "FLOOR", 0.0, 60.0, 2),
        ("DRAWN", "DRAWN BY", 60.0, 60.0, 2),
        ("DATE", "DATE", 120.0, 60.0, 2),
        ("SHEET", "SHEET", 0.0, 60.0, 3),
        ("SCALE", "SCALE", 60.0, 60.0, 3),
        ("REV", "REVISION", 120.0, 60.0, 3),
    ];
    let mut kinds: Vec<(String, EntityKind)> = Vec::new();
    let x = |v: f64| v - W;
    let y = |row: usize| H - ROW * (row as f64 + 1.0);
    kinds.push(("0".into(), lwpoly(rect_vertices(Vec2::new(x(0.0), 0.0), Vec2::new(0.0, H)), true)));
    for r in 1..4 {
        let yy = H - ROW * r as f64;
        // Rows below the first are split into cells; the first row spans the panel.
        kinds.push(("0".into(), line(Vec2::new(x(0.0), yy), Vec2::new(0.0, yy))));
    }
    for (_, _, x0, w, row) in cells.iter().filter(|c| c.1 != "PROJECT") {
        if *x0 > 0.0 {
            kinds.push(("0".into(), line(Vec2::new(x(*x0), y(*row)), Vec2::new(x(*x0), y(*row) + ROW))));
        }
        let _ = w;
    }
    for (tag, label, x0, _, row) in cells {
        let (cx, cy) = (x(x0) + 2.0, y(row));
        kinds.push(("0".into(), make_text(s, Vec2::new(cx, cy + ROW - 4.0), label, 2.0, 0.0, (HAlign::Left, VAlign::Baseline))));
        kinds.push(("0".into(), attdef(&style, tag, label, Vec2::new(cx, cy + 3.0), 4.5)));
    }
    let d = s.doc_mut()?;
    let mut b = Block::new(TITLE_BLOCK);
    b.description = "O-Draft title block".into();
    for (layer, kind) in kinds {
        let handle = d.new_handle();
        b.entities.push(Entity { handle, common: Common { layer, ..Default::default() }, kind });
    }
    d.blocks.insert(TITLE_BLOCK.into(), Arc::new(b));
    Ok(())
}

/// The handles of the title block references (or any block with title fields) in `layout`.
fn title_inserts(d: &Drawing, layout: &str) -> Vec<aad_doc::Handle> {
    let Some(l) = d.layouts.iter().find(|l| l.name == layout) else { return Vec::new() };
    l.entities
        .iter()
        .filter(|e| matches!(&e.kind, EntityKind::Insert(i) if i.block.eq_ignore_ascii_case(TITLE_BLOCK) || i.attribs.iter().any(|a| FIELDS.iter().any(|(t, _)| a.tag.eq_ignore_ascii_case(t)))))
        .map(|e| e.handle)
        .collect()
}

fn read_fields(d: &Drawing, layout: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(l) = d.layouts.iter().find(|l| l.name == layout) else { return out };
    for h in title_inserts(d, layout) {
        if let Some(EntityKind::Insert(i)) = l.entities.get(h).map(|e| e.kind.clone()) {
            for a in &i.attribs {
                out.entry(a.tag.to_ascii_uppercase()).or_insert_with(|| a.text.value.clone());
            }
        }
    }
    out
}

fn write_fields(d: &mut Drawing, layout: &str, fields: &BTreeMap<String, String>) -> usize {
    let mut changed = 0;
    for h in title_inserts(d, layout) {
        let Some(l) = d.layouts.iter_mut().find(|l| l.name == layout) else { continue };
        l.entities.modify(h, |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                for a in &mut i.attribs {
                    if let Some(v) = fields.get(&a.tag.to_ascii_uppercase())
                        && a.text.value != *v
                    {
                        a.text.value = v.clone();
                        changed += 1;
                    }
                }
            }
        });
    }
    changed
}

/// What a new title block starts with: the project's details and floor, the first viewport's scale.
fn default_fields(d: &Drawing, layout: &str) -> BTreeMap<String, String> {
    let det = project::get(d, "details");
    let text = |k: &str| det.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let mut m = BTreeMap::new();
    m.insert("PROJECT".into(), text("title"));
    m.insert("CLIENT".into(), text("owner"));
    m.insert("DRAWN".into(), if text("engineer").is_empty() { text("architect") } else { text("engineer") });
    m.insert("SHEET".into(), layout.to_string());
    if let Some(f) = project::get(d, "projectFile").get("floor").and_then(Value::as_str) {
        m.insert("FLOOR".into(), f.to_string());
    }
    if let Some(l) = d.layouts.iter().find(|l| l.name == layout)
        && let Some(v) = l.entities.iter().find_map(|e| match &e.kind {
            EntityKind::Viewport(v) if v.id != 1 && v.view_height > 1e-12 => Some(v.height / v.view_height),
            _ => None,
        })
    {
        m.insert("SCALE".into(), scale_text(v));
    }
    m
}

fn string_map(v: Option<&Value>) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    if let Some(o) = v.and_then(Value::as_object) {
        if o.len() > 200 {
            return Err(bad("sheet.fields", "too many fields"));
        }
        for (k, val) in o {
            let t = match val {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Null => String::new(),
                _ => return Err(bad("sheet.fields", format!("`{k}` must be text"))),
            };
            if t.chars().count() > 500 {
                return Err(bad("sheet.fields", format!("`{k}` is too long")));
            }
            out.insert(k.to_ascii_uppercase(), t);
        }
    }
    Ok(out)
}

// ---------------- title block and fields ----------------

fn run_titleblock(s: &mut Session, p: &Value) -> Result<Value> {
    let layout = layout_of(s, "sheet.titleblock", p)?;
    let given = string_map(p.get("fields"))?;
    let existing = title_inserts(s.doc()?, &layout);
    let mut created = false;
    if existing.is_empty() {
        ensure_title_block(s)?;
        let sh = sheet_of(s.doc()?, &layout, "sheet.titleblock")?;
        let prt = sh.printable;
        let k = 1.0 / sh.unit_mm;
        let at = Vec2::new(prt.max.x, prt.min.y);
        in_layout(s, &layout, |s| {
            ensure_layer(s.doc_mut()?, BORDER_LAYER, 7, "Sheet border");
            ensure_layer(s.doc_mut()?, TITLE_LAYER, 7, "Title block");
            super::hcw::add_on(s, BORDER_LAYER, lwpoly(rect_vertices(prt.min, prt.max), true))?;
            s.execute("insert", &json!({ "name": TITLE_BLOCK, "at": [at.x, at.y], "scale": k }))?;
            Ok(())
        })?;
        created = true;
    }
    let mut fields = default_fields(s.doc()?, &layout);
    fields.extend(read_fields(s.doc()?, &layout).into_iter().filter(|(_, v)| !v.is_empty()));
    fields.extend(given);
    write_fields(s.doc_mut()?, &layout, &fields);
    let handle = title_inserts(s.doc()?, &layout).first().map(|h| h.hex());
    Ok(json!({ "handle": handle, "fields": read_fields(s.doc()?, &layout), "created": created }))
}

fn run_fields(s: &mut Session, p: &Value) -> Result<Value> {
    let layout = layout_of(s, "sheet.fields", p)?;
    let given = string_map(p.get("fields"))?;
    if !given.is_empty() {
        if title_inserts(s.doc()?, &layout).is_empty() {
            return Err(bad("sheet.fields", "this sheet has no title block: add one with sheet.titleblock or a template"));
        }
        write_fields(s.doc_mut()?, &layout, &given);
    }
    let known: Vec<Value> = FIELDS.iter().map(|(t, l)| json!({ "tag": t, "label": l })).collect();
    Ok(json!({ "fields": read_fields(s.doc()?, &layout), "known": known, "titleBlock": !title_inserts(s.doc()?, &layout).is_empty() }))
}

// ---------------- templates ----------------

fn user_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let base = std::env::var_os("APPDATA").map(std::path::PathBuf::from).or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(std::path::PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config")))
        })?;
        Some(base.join("AAD").join("sheet-templates"))
    }
}

/// The folder user templates live in: `folder` (an office-wide shared folder, for example) or the AAD
/// settings folder.
fn dir_of(p: &Value) -> Option<std::path::PathBuf> {
    match str_param(p, "folder") {
        Some(f) => Some(std::path::PathBuf::from(f)),
        None => user_dir(),
    }
}

fn template_file(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    dir.join(format!("{}.json", safe_name(name)))
}

fn safe_name(raw: &str) -> String {
    let s: String = raw.chars().map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') { c } else { '_' }).collect();
    s.trim().chars().take(60).collect()
}

fn check_name(cmd: &str, raw: &str) -> Result<String> {
    let n = safe_name(raw);
    if n.is_empty() { Err(bad(cmd, "the template needs a name")) } else { Ok(n) }
}

fn capture(d: &Drawing, layout: &str) -> Result<Value> {
    let l = d.layouts.iter().find(|l| l.name == layout).ok_or_else(|| bad("sheet.template.save", format!("no layout named `{layout}`")))?;
    let mut entities = Vec::new();
    let mut want: Vec<(String, usize)> = Vec::new();
    for e in l.entities.iter() {
        if matches!(e.kind, EntityKind::Viewport(_)) {
            continue;
        }
        if entities.len() >= MAX_TEMPLATE_ENTITIES {
            return Err(bad("sheet.template.save", "the sheet has too many objects for a template"));
        }
        if let EntityKind::Insert(i) = &e.kind {
            want.push((i.block.clone(), 0));
        }
        entities.push(serde_json::to_value(&**e).map_err(|x| bad("sheet.template.save", x.to_string()))?);
    }
    let mut blocks: Vec<Value> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    while let Some((name, depth)) = want.pop() {
        if !seen.insert(name.to_ascii_uppercase()) || blocks.len() >= MAX_TEMPLATE_BLOCKS {
            continue;
        }
        let Some(b) = d.block(&name) else { continue };
        let mut es = Vec::new();
        for e in b.entities.iter() {
            if let EntityKind::Insert(i) = &e.kind
                && depth < aad_doc::MAX_BLOCK_DEPTH
            {
                want.push((i.block.clone(), depth + 1));
            }
            es.push(serde_json::to_value(&**e).map_err(|x| bad("sheet.template.save", x.to_string()))?);
        }
        blocks.push(json!({ "name": b.name, "base": [b.base.x, b.base.y, b.base.z], "description": b.description, "entities": es }));
    }
    Ok(
        json!({ "format": "aad-sheet-template", "version": 1, "page": serde_json::to_value(&l.page).unwrap_or(Value::Null), "entities": entities, "blocks": blocks }),
    )
}

fn run_tpl_save(s: &mut Session, p: &Value) -> Result<Value> {
    let name = check_name("sheet.template.save", str_param(p, "name").unwrap_or(""))?;
    let layout = layout_of(s, "sheet.template.save", p)?;
    let tpl = capture(s.doc()?, &layout)?;
    let counts = json!({ "entities": tpl["entities"].as_array().map_or(0, Vec::len), "blocks": tpl["blocks"].as_array().map_or(0, Vec::len) });
    match str_param(p, "scope").unwrap_or("drawing") {
        "drawing" => {
            let mut all = match project::get(s.doc()?, "sheetTemplates") {
                Value::Object(o) => o,
                _ => Map::new(),
            };
            all.insert(name.clone(), tpl);
            project::set(s.doc_mut()?, "sheetTemplates", &Value::Object(all));
        }
        "user" => {
            let dir = dir_of(p).ok_or_else(|| bad("sheet.template.save", "a user template cannot be saved here"))?;
            let path = template_file(&dir, &name);
            let text = serde_json::to_string(&tpl).map_err(|e| bad("sheet.template.save", e.to_string()))?;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| bad("sheet.template.save", format!("{}: {e}", dir.display())))?;
            }
            std::fs::write(&path, text).map_err(|e| bad("sheet.template.save", format!("{}: {e}", path.display())))?;
        }
        other => return Err(bad("sheet.template.save", format!("`scope` must be drawing or user, not `{other}`"))),
    }
    Ok(json!({ "name": name, "entities": counts["entities"], "blocks": counts["blocks"], "message": format!("Saved sheet template `{name}`") }))
}

fn user_templates(dir: Option<std::path::PathBuf>) -> Vec<(String, Value)> {
    let Some(dir) = dir else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for e in rd.flatten().take(200) {
        let path = e.path();
        if path.extension().is_none_or(|x| x != "json") || e.metadata().map_or(true, |m| m.len() > MAX_TEMPLATE_BYTES) {
            continue;
        }
        let Some(name) = path.file_stem().map(|n| n.to_string_lossy().to_string()) else { continue };
        if let Ok(text) = std::fs::read_to_string(&path)
            && let Ok(v) = serde_json::from_str::<Value>(&text)
            && v["format"] == "aad-sheet-template"
        {
            out.push((name, v));
        }
    }
    out
}

fn find_template(s: &Session, name: &str, dir: Option<std::path::PathBuf>) -> Result<Value> {
    if let Some(v) = project::get(s.doc()?, "sheetTemplates").get(name) {
        return Ok(v.clone());
    }
    user_templates(dir)
        .into_iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v)
        .ok_or_else(|| bad("sheet.template.apply", format!("no sheet template named `{name}`")))
}

fn run_tpl_list(s: &mut Session, p: &Value) -> Result<Value> {
    let mut out = Vec::new();
    if let Value::Object(o) = project::get(s.doc()?, "sheetTemplates") {
        for (n, v) in o {
            out.push(json!({ "name": n, "scope": "drawing", "paper": v["page"]["paper"], "entities": v["entities"].as_array().map_or(0, Vec::len) }));
        }
    }
    for (n, v) in user_templates(dir_of(p)) {
        out.push(json!({ "name": n, "scope": "user", "paper": v["page"]["paper"], "entities": v["entities"].as_array().map_or(0, Vec::len) }));
    }
    Ok(json!({ "templates": out }))
}

fn run_tpl_delete(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("sheet.template.delete", "`name` is required"))?.to_string();
    match str_param(p, "scope").unwrap_or("drawing") {
        "drawing" => {
            let mut all = match project::get(s.doc()?, "sheetTemplates") {
                Value::Object(o) => o,
                _ => Map::new(),
            };
            if all.remove(&name).is_none() {
                return Err(bad("sheet.template.delete", format!("no sheet template named `{name}` in this drawing")));
            }
            project::set(s.doc_mut()?, "sheetTemplates", &Value::Object(all));
        }
        "user" => {
            let path = dir_of(p)
                .map(|d| template_file(&d, &name))
                .filter(|f| f.is_file())
                .ok_or_else(|| bad("sheet.template.delete", format!("no user template named `{name}`")))?;
            std::fs::remove_file(&path).map_err(|e| bad("sheet.template.delete", e.to_string()))?;
        }
        other => return Err(bad("sheet.template.delete", format!("`scope` must be drawing or user, not `{other}`"))),
    }
    Ok(json!({ "deleted": name }))
}

fn run_tpl_apply(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("sheet.template.apply", "`name` is required"))?.to_string();
    let layout = layout_of(s, "sheet.template.apply", p)?;
    let tpl = find_template(s, &name, dir_of(p))?;
    let given = string_map(p.get("fields"))?;
    let page: PageSetup =
        serde_json::from_value(tpl["page"].clone()).map_err(|e| bad("sheet.template.apply", format!("the template's page setup is damaged: {e}")))?;
    let ents = tpl["entities"].as_array().ok_or_else(|| bad("sheet.template.apply", "the template has no objects"))?;
    let blocks = tpl["blocks"].as_array().cloned().unwrap_or_default();
    if ents.len() > MAX_TEMPLATE_ENTITIES || blocks.len() > MAX_TEMPLATE_BLOCKS {
        return Err(bad("sheet.template.apply", "the template is too large"));
    }
    // Decode everything first, so a damaged template changes nothing.
    let mut decoded: Vec<Entity> = Vec::new();
    for e in ents {
        let ent: Entity = serde_json::from_value(e.clone()).map_err(|x| bad("sheet.template.apply", format!("a template object is damaged: {x}")))?;
        if !matches!(ent.kind, EntityKind::Viewport(_)) {
            decoded.push(ent);
        }
    }
    let mut defs: Vec<(String, aad_geom::Vec3, String, Vec<Entity>)> = Vec::new();
    for b in &blocks {
        let n = b["name"].as_str().filter(|n| !n.is_empty()).ok_or_else(|| bad("sheet.template.apply", "a template block has no name"))?;
        let base = b["base"]
            .as_array()
            .map(|a| aad_geom::Vec3::new(a.first().and_then(Value::as_f64).unwrap_or(0.0), a.get(1).and_then(Value::as_f64).unwrap_or(0.0), 0.0))
            .unwrap_or(aad_geom::Vec3::ZERO);
        let mut es = Vec::new();
        for e in b["entities"].as_array().into_iter().flatten().take(MAX_TEMPLATE_ENTITIES) {
            es.push(
                serde_json::from_value::<Entity>(e.clone()).map_err(|x| bad("sheet.template.apply", format!("a template block is damaged: {x}")))?,
            );
        }
        defs.push((n.to_string(), base, b["description"].as_str().unwrap_or("").to_string(), es));
    }

    // What an earlier template put on this layout goes; nothing else does.
    let owned_key = "sheetOwned";
    let mut owned = match project::get(s.doc()?, owned_key) {
        Value::Object(o) => o,
        _ => Map::new(),
    };
    let old: Vec<String> =
        owned.get(&layout).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    let d = s.doc_mut()?;
    let space = Space::Paper(layout.clone());
    let mut replaced = 0;
    for h in old {
        if let Some(hd) = aad_doc::Handle::parse_hex(&h)
            && d.space(&space).is_some_and(|st| st.get(hd).is_some())
        {
            d.remove_entity(hd);
            replaced += 1;
        }
    }
    if let Some(l) = d.layouts.iter_mut().find(|l| l.name == layout) {
        l.page = page;
    }
    for (n, base, desc, es) in defs {
        if d.block(&n).is_some() {
            continue;
        }
        let mut b = Block::new(&n);
        b.base = base;
        b.description = desc;
        for mut e in es {
            e.handle = d.new_handle();
            b.entities.push(e);
        }
        d.blocks.insert(n, Arc::new(b));
    }
    let mut placed = Vec::new();
    for e in decoded {
        let h = d.add(&space, e.common, e.kind)?;
        placed.push(json!(h.hex()));
    }
    let n = placed.len();
    owned.insert(layout.clone(), Value::Array(placed));
    project::set(d, owned_key, &Value::Object(owned));

    // Fill the title fields from the project, then from what the caller gave.
    let mut fields = default_fields(s.doc()?, &layout);
    fields.extend(read_fields(s.doc()?, &layout).into_iter().filter(|(_, v)| !v.is_empty()));
    fields.extend(given);
    write_fields(s.doc_mut()?, &layout, &fields);
    Ok(json!({ "placed": n, "replaced": replaced, "layout": layout, "message": format!("Applied sheet template `{name}` to {layout}") }))
}

// ---------------- viewports ----------------

fn viewport_json(h: aad_doc::Handle, v: &aad_doc::Viewport) -> Value {
    let scale = if v.view_height > 1e-12 { v.height / v.view_height } else { 0.0 };
    let (x0, y0) = (v.center.x - v.width / 2.0, v.center.y - v.height / 2.0);
    json!({
        "handle": h.hex(), "rect": [x0, y0, x0 + v.width, y0 + v.height], "width": v.width, "height": v.height,
        "scale": scale, "scaleText": scale_text(scale), "locked": v.locked, "frozen": v.frozen_layers,
        "viewCenter": [v.view_center.x, v.view_center.y], "viewHeight": v.view_height,
    })
}

fn run_vp_list(s: &mut Session, p: &Value) -> Result<Value> {
    let layout = layout_of(s, "viewport.list", p)?;
    let d = s.doc()?;
    let l = d.layouts.iter().find(|l| l.name == layout).ok_or_else(|| bad("viewport.list", "no such layout"))?;
    let vps: Vec<Value> = l
        .entities
        .iter()
        .filter_map(|e| if let EntityKind::Viewport(v) = &e.kind { (v.id != 1).then(|| viewport_json(e.handle, v)) } else { None })
        .collect();
    Ok(json!({ "layout": layout, "viewports": vps }))
}

fn run_vp_add(s: &mut Session, p: &Value) -> Result<Value> {
    let layout = layout_of(s, "viewport.add", p)?;
    let sh = sheet_of(s.doc()?, &layout, "viewport.add")?;
    let (a, b) = match (point_param(p, "p1"), point_param(p, "p2")) {
        (Some(a), Some(b)) => (a, b),
        _ => {
            // Above the title block when there is one, else the printable area.
            let prt: Bounds2 = sh.printable;
            let top = if title_inserts(s.doc()?, &layout).is_empty() { prt.min.y } else { prt.min.y + 56.0 / sh.unit_mm };
            (Vec2::new(prt.min.x, top), prt.max)
        }
    };
    let mut q = json!({ "layout": layout, "count": 1, "p1": [a.x, a.y], "p2": [b.x, b.y] });
    if let Some(o) = q.as_object_mut() {
        o.insert("layout".into(), json!(layout));
    }
    let r = s.execute("mview", &q)?;
    let handle = r["viewports"][0].as_str().ok_or_else(|| bad("viewport.add", "the viewport was not created"))?.to_string();
    let mut set = json!({ "handle": handle });
    if let Some(sc) = p.get("scale") {
        set["scale"] = sc.clone();
    }
    if let Some(l) = p.get("locked") {
        set["locked"] = l.clone();
    }
    if set.as_object().is_some_and(|o| o.len() > 1) {
        s.execute("viewport.set", &set)?;
    }
    // A title block on the sheet shows the scale of the viewport just added.
    if !title_inserts(s.doc()?, &layout).is_empty()
        && let Some(EntityKind::Viewport(v)) = aad_doc::Handle::parse_hex(&handle).and_then(|h| s.doc().ok()?.entity(h).map(|e| e.kind.clone()))
        && v.view_height > 1e-12
    {
        let mut f = BTreeMap::new();
        f.insert("SCALE".to_string(), scale_text(v.height / v.view_height));
        write_fields(s.doc_mut()?, &layout, &f);
    }
    Ok(json!({ "handle": handle }))
}

fn run_vp_remove(s: &mut Session, p: &Value) -> Result<Value> {
    let hex = str_param(p, "handle").ok_or_else(|| bad("viewport.remove", "`handle` is required"))?;
    let h = aad_doc::Handle::parse_hex(hex).ok_or_else(|| bad("viewport.remove", "`handle` is not a handle"))?;
    let d = s.doc_mut()?;
    let is_vp = d.entity(h).is_some_and(|e| matches!(&e.kind, EntityKind::Viewport(v) if v.id != 1));
    if !is_vp {
        return Err(bad("viewport.remove", "that is not a viewport"));
    }
    d.remove_entity(h);
    Ok(json!({ "removed": hex }))
}
