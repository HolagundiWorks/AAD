//! ETRANSMIT (pack drawing + dependencies) and ADCENTER (browse reusable content).

use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

const MAX_FILES: usize = 500;
const MAX_LIST: usize = 10_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("etransmit", "eTransmit", run_etransmit)
            .menu(&["File", "eTransmit..."])
            .alias(&["et"])
            .params(
                "{path?: output folder (default: <drawing>-etransmit beside the drawing), includeXrefs?: bool (default true), \
                 includeImages?: bool (default true)} → {folder, files: [{role, path}]}",
            )
            .noundo(),
        CommandSpec::new("adcenter", "Design Center", run_adcenter)
            .menu(&["Tools", "Design Center"])
            .alias(&["adc", "dc"])
            .params(
                "{path?: drawing to browse (default: this drawing), table?: blocks|layers|styles|dimstyles|all (default all)} → \
                 {path, blocks, layers, styles, dimstyles}",
            )
            .noundo(),
    ]
}

fn run_etransmit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "etransmit";
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        return Err(bad(CMD, "eTransmit is not available in this build"));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::path::{Path, PathBuf};
        let st = s.state()?;
        let drawing = st.path.as_ref().ok_or_else(|| bad(CMD, "save the drawing first"))?;
        let src = Path::new(drawing);
        let parent = src.parent().unwrap_or_else(|| Path::new("."));
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("drawing");
        let out = str_param(p, "path").map(PathBuf::from).unwrap_or_else(|| parent.join(format!("{stem}-etransmit")));
        std::fs::create_dir_all(&out).map_err(|e| bad(CMD, format!("{}: {e}", out.display())))?;
        let mut files = Vec::new();
        let dest = out.join(src.file_name().unwrap_or_default());
        std::fs::copy(src, &dest).map_err(|e| bad(CMD, format!("copy drawing: {e}")))?;
        files.push(json!({ "role": "drawing", "path": dest.display().to_string() }));
        if bool_or(p, "includeXrefs", true) {
            for b in s.doc()?.blocks.values().take(MAX_FILES) {
                let Some(path) = b.xref_path.as_deref().filter(|s| !s.is_empty()) else { continue };
                let from = Path::new(path);
                if !from.is_file() {
                    // Resolve beside the drawing.
                    let beside = parent.join(path);
                    if beside.is_file() {
                        let name = beside.file_name().unwrap_or_default();
                        let to = out.join(name);
                        if std::fs::copy(&beside, &to).is_ok() {
                            files.push(json!({ "role": "xref", "name": b.name, "path": to.display().to_string() }));
                        }
                    }
                    continue;
                }
                let name = from.file_name().unwrap_or_default();
                let to = out.join(name);
                if std::fs::copy(from, &to).is_ok() {
                    files.push(json!({ "role": "xref", "name": b.name, "path": to.display().to_string() }));
                }
                if files.len() >= MAX_FILES {
                    break;
                }
            }
        }
        if bool_or(p, "includeImages", true) {
            let d = s.doc()?;
            for e in d.model.iter().chain(d.layouts.iter().flat_map(|l| l.entities.iter())).take(MAX_LIST) {
                if let aad_doc::EntityKind::Image(img) = &e.kind {
                    let from = Path::new(&img.path);
                    if from.is_file() {
                        let name = from.file_name().unwrap_or_default();
                        let to = out.join(name);
                        if std::fs::copy(from, &to).is_ok() {
                            files.push(json!({ "role": "image", "path": to.display().to_string() }));
                        }
                    }
                }
                if files.len() >= MAX_FILES {
                    break;
                }
            }
        }
        Ok(json!({ "folder": out.display().to_string(), "files": files, "count": files.len() }))
    }
}

fn run_adcenter(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "adcenter";
    let table = str_param(p, "table").unwrap_or("all").to_ascii_lowercase();
    #[cfg(not(target_arch = "wasm32"))]
    let (path, doc) = if let Some(path) = str_param(p, "path") {
        let bytes = std::fs::read(path).map_err(|e| bad(CMD, format!("{path}: {e}")))?;
        if bytes.len() > 200_000_000 {
            return Err(bad(CMD, "file too large"));
        }
        let io = super::file::io().ok_or_else(|| bad(CMD, "file formats are not available in this build"))?;
        let d = (io.read)(&bytes, path).map_err(|e| bad(CMD, e.to_string()))?;
        (path.to_string(), d)
    } else {
        ("(current)".into(), s.doc()?.clone())
    };
    #[cfg(target_arch = "wasm32")]
    let (path, doc) = {
        if str_param(p, "path").is_some() {
            return Err(bad(CMD, "browsing another file is not available in this build"));
        }
        ("(current)".into(), s.doc()?.clone())
    };
    let want = |k: &str| table == "all" || table == k;
    let mut out = json!({ "path": path });
    if want("blocks") {
        let blocks: Vec<Value> = doc
            .blocks
            .values()
            .filter(|b| !b.anonymous)
            .take(MAX_LIST)
            .map(|b| json!({ "name": b.name, "entities": b.entities.len(), "xref": b.xref_path, "description": b.description }))
            .collect();
        out["blocks"] = Value::Array(blocks);
    }
    if want("layers") {
        let layers: Vec<Value> = doc.layers.iter().take(MAX_LIST).map(|l| json!({ "name": l.name, "on": l.on, "frozen": l.frozen, "locked": l.locked, "color": l.color.name() })).collect();
        out["layers"] = Value::Array(layers);
    }
    if want("styles") {
        let styles: Vec<Value> = doc.text_styles.iter().take(MAX_LIST).map(|t| json!({ "name": t.name, "font": t.font, "height": t.height })).collect();
        out["styles"] = Value::Array(styles);
    }
    if want("dimstyles") {
        let styles: Vec<Value> = doc.dim_styles.iter().take(MAX_LIST).map(|t| json!({ "name": t.name })).collect();
        out["dimstyles"] = Value::Array(styles);
    }
    Ok(out)
}
