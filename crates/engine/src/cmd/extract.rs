//! EXPORT (format umbrella), EATTEXT / DATAEXTRACTION, DWGCOMPARE, and agent-facing
//! VALIDATE / INSPECT / QUERY / PROPOSEEDIT / PREVIEWEDIT.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use aad_doc::{Drawing, EntityKind, Handle};
use serde_json::{Value, json};

use super::file::{base64_encode, io};
use super::*;
use crate::{Result, Session};

const MAX_ROWS: usize = 50_000;
const MAX_COMPARE: usize = 200_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("export", "Export...", run_export)
            .menu(&["File", "Export..."])
            .params(
                "{format?: dxf|dwg|pdf|png|svg|csv (default from path, else dxf), path? (else returns base64 `data`), \
                 layout?, paper?, landscape?, fit?, lineweights?} → {path?|data, bytes, format}",
            )
            .noundo(),
        CommandSpec::new("eattext", "Extract Attributes", run_eattext)
            .menu(&["Tools", "Attribute Extraction..."])
            .alias(&["eat"])
            .params(
                "{handles? (INSERT), block?, path? (write CSV), format?: csv|json (default csv)} → \
                 {rows, columns, csv?|data?, path?, count}",
            )
            .noundo(),
        CommandSpec::new("dataextraction", "Data Extraction", run_dataextraction)
            .menu(&["Tools", "Data Extraction..."])
            .alias(&["dataextract"])
            .params(
                "{type?, layer?, handles?, path?, format?: csv|json (default csv), \
                 includeGeometry?: bool} → {rows, columns, csv?|data?, path?, count}",
            )
            .noundo(),
        CommandSpec::new("dwgcompare", "Compare Drawings", run_dwgcompare)
            .menu(&["Tools", "Drawing Compare..."])
            .alias(&["compare"])
            .params(
                "{path} | {data: base64, name?} | {index: open drawing} → \
                 {matched, onlyHere, onlyThere, typeDiffs, message}",
            )
            .noundo(),
        CommandSpec::new("validate", "Validate Drawing", run_validate)
            .menu(&["Tools", "Validate"])
            .params("{fix?: bool} → {ok, errors, fixed?, issues, counts, message}")
            .noundo(),
        CommandSpec::new("inspect", "Inspect", run_inspect_alias)
            .params("{entities?: bool, limit?: n} (same as drawing.inspect)")
            .noundo(),
        CommandSpec::new("query", "Query", run_query_alias)
            .params(
                "{type?, layer?, window?: [[x,y],[x,y]], crossing?: bool, limit?, offset?} \
                 (same as entities) | {window: [x0,y0,x1,y1], ...} (same as spatial.query)",
            )
            .noundo(),
        CommandSpec::new("proposeedit", "Propose Edit", run_proposeedit)
            .params("{command, params?} → {proposed, undoable, label, message} (does not run the command)")
            .noundo(),
        CommandSpec::new("previewedit", "Preview Edit", run_previewedit)
            .params(
                "{command, params?} → {result, beforeEntities, afterEntities, reverted, message} \
                 (runs then restores the drawing when the command is undoable)",
            )
            .noundo(),
    ]
}

fn fmt_of(path: Option<&str>, explicit: Option<&str>) -> String {
    if let Some(f) = explicit.filter(|s| !s.is_empty()) {
        return f.to_ascii_lowercase();
    }
    path.and_then(|p| std::path::Path::new(p).extension())
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_else(|| "dxf".into())
}

fn write_export_bytes(cmd: &str, path: Option<&str>, name: &str, bytes: &[u8]) -> Result<Value> {
    if let Some(path) = path {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(parent) = std::path::Path::new(path).parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent).map_err(|e| bad(cmd, format!("{path}: {e}")))?;
            }
            std::fs::write(path, bytes).map_err(|e| bad(cmd, format!("{path}: {e}")))?;
            return Ok(json!({ "path": path, "bytes": bytes.len(), "format": fmt_of(Some(path), None) }));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let deliver = file::deliver().ok_or_else(|| bad(cmd, "file delivery is not available; omit `path` for base64"))?;
            deliver(name, "application/octet-stream", bytes).map_err(|e| bad(cmd, e))?;
            return Ok(json!({ "download": name, "bytes": bytes.len(), "format": fmt_of(Some(name), None) }));
        }
    }
    Ok(json!({ "data": base64_encode(bytes), "bytes": bytes.len(), "format": fmt_of(Some(name), None) }))
}

fn run_export(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "export";
    let path = str_param(p, "path");
    let format = fmt_of(path, str_param(p, "format"));
    match format.as_str() {
        "csv" => {
            let table = attribute_table(s, p, None)?;
            let csv = table_to_csv(&table.columns, &table.rows);
            let bytes = csv.into_bytes();
            let name = path.unwrap_or("extract.csv");
            write_export_bytes(CMD, path, name, &bytes)
        }
        "pdf" => {
            let mut plot_p = p.clone();
            if let Some(o) = plot_p.as_object_mut() {
                o.remove("format");
            }
            s.execute("exportpdf", &plot_p)
        }
        "dxf" | "dwg" | "png" | "svg" => {
            let hooks = io().ok_or_else(|| bad(CMD, "file formats are not available in this build"))?;
            let name = path.unwrap_or(match format.as_str() {
                "dwg" => "drawing.dwg",
                "png" => "drawing.png",
                "svg" => "drawing.svg",
                _ => "drawing.dxf",
            });
            let bytes = (hooks.write)(&super::xref::without_overlays(s.doc()?), name).map_err(|e| bad(CMD, e))?;
            let mut r = write_export_bytes(CMD, path, name, &bytes)?;
            if matches!(format.as_str(), "dxf" | "dwg") {
                let lost = file::not_saved(s.doc()?);
                if let Some(m) = file::not_saved_message(&lost) {
                    r["message"] = json!(m);
                    r["notSaved"] = json!(lost);
                }
            }
            Ok(r)
        }
        other => Err(bad(CMD, format!("unknown format `{other}` (dxf, dwg, pdf, png, svg, csv)"))),
    }
}

struct Table {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
}

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

fn table_to_csv(columns: &[String], rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    out.push_str(&columns.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
    out.push('\n');
    for row in rows.iter().take(MAX_ROWS) {
        let line: Vec<String> = columns.iter().enumerate().map(|(i, _)| csv_cell(row.get(i).map(String::as_str).unwrap_or(""))).collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

fn table_result(cmd: &str, p: &Value, table: Table) -> Result<Value> {
    let format = str_param(p, "format").unwrap_or("csv").to_ascii_lowercase();
    let path = str_param(p, "path");
    let count = table.rows.len();
    let mut out = json!({ "columns": table.columns, "count": count, "rows": count.min(MAX_ROWS) });
    match format.as_str() {
        "json" => {
            let objs: Vec<Value> = table
                .rows
                .iter()
                .take(MAX_ROWS)
                .map(|row| {
                    let mut o = serde_json::Map::new();
                    for (i, col) in table.columns.iter().enumerate() {
                        o.insert(col.clone(), json!(row.get(i).cloned().unwrap_or_default()));
                    }
                    Value::Object(o)
                })
                .collect();
            out["data"] = Value::Array(objs);
            if let Some(path) = path {
                let bytes = serde_json::to_vec_pretty(&out["data"]).unwrap_or_default();
                let written = write_export_bytes(cmd, Some(path), path, &bytes)?;
                if let Some(o) = out.as_object_mut() {
                    for (k, v) in written.as_object().into_iter().flatten() {
                        o.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        _ => {
            let csv = table_to_csv(&table.columns, &table.rows);
            out["csv"] = json!(csv);
            if let Some(path) = path {
                let written = write_export_bytes(cmd, Some(path), path, csv.as_bytes())?;
                if let Some(o) = out.as_object_mut() {
                    for (k, v) in written.as_object().into_iter().flatten() {
                        o.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }
    Ok(out)
}

fn attribute_table(s: &Session, p: &Value, only_handles: Option<&[Handle]>) -> Result<Table> {
    let block_filter = str_param(p, "block").map(|b| b.to_ascii_lowercase());
    let d = s.doc()?;
    let mut tags: BTreeSet<String> = BTreeSet::new();
    let mut raw: Vec<(String, String, Handle, BTreeMap<String, String>)> = Vec::new();
    let space = s.space();
    let store = d.space(&space);
    let iter = store.into_iter().flat_map(|st| st.iter());
    for e in iter {
        if let Some(hs) = only_handles
            && !hs.is_empty()
            && !hs.contains(&e.handle)
        {
            continue;
        }
        let EntityKind::Insert(ins) = &e.kind else { continue };
        if block_filter.as_ref().is_some_and(|b| !ins.block.eq_ignore_ascii_case(b)) {
            continue;
        }
        if ins.attribs.is_empty() {
            continue;
        }
        let mut vals = BTreeMap::new();
        for a in &ins.attribs {
            tags.insert(a.tag.clone());
            vals.insert(a.tag.clone(), a.text.value.clone());
        }
        raw.push((ins.block.clone(), e.common.layer.clone(), e.handle, vals));
        if raw.len() >= MAX_ROWS {
            break;
        }
    }
    let mut columns = vec!["HANDLE".into(), "BLOCK".into(), "LAYER".into()];
    columns.extend(tags.iter().cloned());
    let mut rows = Vec::with_capacity(raw.len());
    for (block, layer, h, vals) in raw {
        let mut row = vec![h.hex(), block, layer];
        for t in &tags {
            row.push(vals.get(t).cloned().unwrap_or_default());
        }
        rows.push(row);
    }
    Ok(Table { columns, rows })
}

fn run_eattext(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "eattext";
    let hs = targets(s, p)?;
    let only = if hs.is_empty() { None } else { Some(hs.as_slice()) };
    let table = attribute_table(s, p, only)?;
    table_result(CMD, p, table)
}

fn entity_row(d: &Drawing, e: &aad_doc::Entity, include_geometry: bool) -> (Vec<String>, BTreeMap<String, String>) {
    let mut cols = BTreeMap::new();
    cols.insert("HANDLE".into(), e.handle.hex());
    cols.insert("TYPE".into(), e.kind.type_name().to_string());
    cols.insert("LAYER".into(), e.common.layer.clone());
    cols.insert("COLOR".into(), e.common.color.name());
    cols.insert("LINETYPE".into(), e.common.linetype.clone());
    match &e.kind {
        EntityKind::Insert(i) => {
            cols.insert("BLOCK".into(), i.block.clone());
            for a in &i.attribs {
                cols.insert(format!("ATTR:{}", a.tag), a.text.value.clone());
            }
        }
        EntityKind::Text(t) => {
            cols.insert("TEXT".into(), t.value.clone());
        }
        EntityKind::MText(t) => {
            cols.insert("TEXT".into(), t.contents.clone());
        }
        _ => {}
    }
    if include_geometry {
        let b = aad_doc::entity_bounds(d, e, 0);
        if !b.is_empty() {
            cols.insert("XMIN".into(), format!("{:.6}", b.min.x));
            cols.insert("YMIN".into(), format!("{:.6}", b.min.y));
            cols.insert("XMAX".into(), format!("{:.6}", b.max.x));
            cols.insert("YMAX".into(), format!("{:.6}", b.max.y));
        }
    }
    let order = cols.keys().cloned().collect();
    (order, cols)
}

fn run_dataextraction(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "dataextraction";
    let ty = str_param(p, "type").map(|t| t.to_ascii_lowercase());
    let layer = str_param(p, "layer").map(|l| l.to_ascii_lowercase());
    let include_geometry = bool_or(p, "includeGeometry", false);
    let hs = targets(s, p)?;
    let filter_hs = !hs.is_empty();
    let d = s.doc()?;
    let space = s.space();
    let mut all_cols: BTreeSet<String> = BTreeSet::new();
    let mut rows_map: Vec<BTreeMap<String, String>> = Vec::new();
    let store = d.space(&space);
    for e in store.into_iter().flat_map(|st| st.iter()) {
        if filter_hs && !hs.contains(&e.handle) {
            continue;
        }
        if ty.as_ref().is_some_and(|t| e.kind.type_name().to_ascii_lowercase() != *t && e.kind.dxf_name().to_ascii_lowercase() != *t) {
            continue;
        }
        if layer.as_ref().is_some_and(|l| e.common.layer.to_ascii_lowercase() != *l) {
            continue;
        }
        let (order, cols) = entity_row(d, e, include_geometry);
        for c in order {
            all_cols.insert(c);
        }
        rows_map.push(cols);
        if rows_map.len() >= MAX_ROWS {
            break;
        }
    }
    // Stable column order: fixed first, then the rest alphabetically.
    let mut columns = vec!["HANDLE".into(), "TYPE".into(), "LAYER".into(), "COLOR".into(), "LINETYPE".into()];
    for c in &all_cols {
        if !columns.iter().any(|x| x == c) {
            columns.push(c.clone());
        }
    }
    let rows: Vec<Vec<String>> = rows_map.iter().map(|m| columns.iter().map(|c| m.get(c).cloned().unwrap_or_default()).collect()).collect();
    table_result(CMD, p, Table { columns, rows })
}

fn fingerprint(d: &Drawing, e: &aad_doc::Entity) -> String {
    let b = aad_doc::entity_bounds(d, e, 0);
    let (x0, y0, x1, y1) = if b.is_empty() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        ((b.min.x * 1000.0).round() / 1000.0, (b.min.y * 1000.0).round() / 1000.0, (b.max.x * 1000.0).round() / 1000.0, (b.max.y * 1000.0).round() / 1000.0)
    };
    format!("{}|{}|{x0}|{y0}|{x1}|{y1}", e.kind.type_name(), e.common.layer)
}

fn collect_prints(d: &Drawing) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    let mut n = 0usize;
    for e in d.model.iter().chain(d.layouts.iter().flat_map(|l| l.entities.iter())) {
        *m.entry(fingerprint(d, e)).or_insert(0) += 1;
        n += 1;
        if n >= MAX_COMPARE {
            break;
        }
    }
    m
}

fn type_counts(d: &Drawing) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for e in d.model.iter().chain(d.layouts.iter().flat_map(|l| l.entities.iter())).take(MAX_COMPARE) {
        *m.entry(e.kind.type_name().to_string()).or_insert(0) += 1;
    }
    m
}

fn load_other(s: &Session, p: &Value) -> Result<Drawing> {
    const CMD: &str = "dwgcompare";
    if let Some(i) = p.get("index").and_then(Value::as_u64).map(|n| n as usize) {
        let doc = s.docs.get(i).ok_or_else(|| bad(CMD, "no such open drawing"))?;
        return Ok((*doc.doc).clone());
    }
    let hooks = io().ok_or_else(|| bad(CMD, "file formats are not available in this build"))?;
    match (str_param(p, "path"), str_param(p, "data")) {
        (Some(path), _) => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let bytes = std::fs::read(path).map_err(|e| bad(CMD, format!("{path}: {e}")))?;
                (hooks.read)(&bytes, path).map_err(|e| bad(CMD, e))
            }
            #[cfg(target_arch = "wasm32")]
            {
                let _ = path;
                Err(bad(CMD, "paths are not available on the web; pass `data` or `index`"))
            }
        }
        (None, Some(data)) => {
            let bytes = file::base64_decode(data).ok_or_else(|| bad(CMD, "invalid base64"))?;
            let name = str_param(p, "name").unwrap_or("other.dxf");
            (hooks.read)(&bytes, name).map_err(|e| bad(CMD, e))
        }
        _ => Err(bad(CMD, "`path`, `data`, or `index` is required")),
    }
}

fn run_dwgcompare(s: &mut Session, p: &Value) -> Result<Value> {
    let here = s.doc()?;
    let there = load_other(s, p)?;
    let a = collect_prints(here);
    let b = collect_prints(&there);
    let mut matched = 0usize;
    let mut only_here = 0usize;
    let mut only_there = 0usize;
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    for k in keys {
        let ca = a.get(k).copied().unwrap_or(0);
        let cb = b.get(k).copied().unwrap_or(0);
        matched = matched.saturating_add(ca.min(cb));
        only_here = only_here.saturating_add(ca.saturating_sub(cb));
        only_there = only_there.saturating_add(cb.saturating_sub(ca));
    }
    let ta = type_counts(here);
    let tb = type_counts(&there);
    let mut type_diffs = Vec::new();
    let types: BTreeSet<&String> = ta.keys().chain(tb.keys()).collect();
    for t in types {
        let ca = ta.get(t).copied().unwrap_or(0);
        let cb = tb.get(t).copied().unwrap_or(0);
        if ca != cb {
            type_diffs.push(json!({ "type": t, "here": ca, "there": cb }));
        }
    }
    let same = only_here == 0 && only_there == 0;
    Ok(json!({
        "matched": matched,
        "onlyHere": only_here,
        "onlyThere": only_there,
        "typeDiffs": type_diffs,
        "identical": same,
        "message": if same {
            "Drawings match (by type, layer and bounds fingerprint).".to_string()
        } else {
            format!("{only_here} only in this drawing, {only_there} only in the other, {matched} matched.")
        }
    }))
}

fn run_validate(s: &mut Session, p: &Value) -> Result<Value> {
    let fix = bool_or(p, "fix", false);
    let audit = s.execute("audit", &json!({ "fix": fix }))?;
    let d = s.doc()?;
    let mut counts = BTreeMap::new();
    for e in d.model.iter().take(MAX_COMPARE) {
        *counts.entry(e.kind.type_name().to_string()).or_insert(0usize) += 1;
    }
    let errors = audit.get("errors").and_then(Value::as_u64).unwrap_or(0);
    let ok = errors == 0;
    Ok(json!({
        "ok": ok,
        "errors": errors,
        "fixed": audit.get("fixed").cloned().unwrap_or(json!(false)),
        "issues": audit.get("issues").cloned().unwrap_or(json!([])),
        "counts": counts,
        "entityCount": d.entity_count(),
        "message": audit.get("message").cloned().unwrap_or_else(|| json!(if ok { "Drawing is valid." } else { "Drawing has issues." })),
    }))
}

fn run_inspect_alias(s: &mut Session, p: &Value) -> Result<Value> {
    s.execute("drawing.inspect", p)
}

fn run_query_alias(s: &mut Session, p: &Value) -> Result<Value> {
    // Spatial form uses a flat `[x0,y0,x1,y1]` window; entity query uses two corner points.
    if p.get("window").and_then(Value::as_array).is_some_and(|a| a.len() == 4 && a.iter().all(|v| v.as_f64().is_some())) {
        return s.execute("spatial.query", p);
    }
    s.execute("entities", p)
}

fn run_proposeedit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "proposeedit";
    let _ = s.doc()?;
    let name = str_param(p, "command").ok_or_else(|| bad(CMD, "`command` is required"))?;
    let id = resolve_alias(name);
    let spec = find_command(&id).ok_or_else(|| bad(CMD, format!("unknown command `{name}`")))?;
    let params = p.get("params").cloned().unwrap_or_else(|| json!({}));
    if !params.is_object() && !params.is_null() {
        return Err(bad(CMD, "`params` must be an object"));
    }
    Ok(json!({
        "proposed": { "command": spec.id, "params": params },
        "undoable": spec.undoable,
        "label": spec.label,
        "message": format!("Proposed {} ({}); call previewedit or engine.execute to run.", spec.label, spec.id),
    }))
}

fn run_previewedit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "previewedit";
    let name = str_param(p, "command").ok_or_else(|| bad(CMD, "`command` is required"))?;
    let id = resolve_alias(name);
    let spec = find_command(&id).ok_or_else(|| bad(CMD, format!("unknown command `{name}`")))?;
    if matches!(spec.id, "previewedit" | "proposeedit" | "u" | "undo" | "redo") {
        return Err(bad(CMD, format!("cannot preview `{}`", spec.id)));
    }
    let params = p.get("params").cloned().unwrap_or_else(|| json!({}));
    let st = s.state()?;
    let before_doc = Arc::clone(&st.doc);
    let before_sel = st.selection.clone();
    let before_undo = st.undo.len();
    let before_redo = st.redo.len();
    let before_rev = st.revision;
    let before_n = before_doc.entity_count();
    let result = s.execute(spec.id, &params);
    let after_n = s.doc().map(|d| d.entity_count()).unwrap_or(before_n);
    // Always restore: preview must not leave the drawing changed.
    if let Ok(st) = s.state_mut() {
        st.doc = before_doc;
        st.selection = before_sel;
        st.undo.truncate(before_undo);
        st.redo.truncate(before_redo);
        st.revision = before_rev;
    }
    let result = result?;
    Ok(json!({
        "result": result,
        "beforeEntities": before_n,
        "afterEntities": after_n,
        "reverted": true,
        "command": spec.id,
        "message": format!("Preview of {} restored; drawing unchanged.", spec.label),
    }))
}
