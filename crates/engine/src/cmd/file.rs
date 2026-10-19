//! File menu: new, close, switching drawings. Open/save of DWG/DXF go through the io layer,
//! registered by the host (desktop/CLI) via [`crate::cmd::file::set_io`]. On the web, where there is
//! no file system, saved files reach the user through the host's [`crate::cmd::file::set_deliver`].

use std::sync::OnceLock;

use aad_doc::Drawing;
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

/// Plot a space to PDF bytes with JSON options (`{paper?, landscape?, fit?, lineweights?, …}`).
pub type PlotHook = fn(&Drawing, &aad_doc::Space, &Value) -> std::result::Result<Vec<u8>, String>;

/// File format hooks (installed by the app so the engine stays I/O-agnostic and wasm-safe).
pub struct IoHooks {
    pub read: fn(&[u8], &str) -> std::result::Result<Drawing, String>,
    pub write: fn(&Drawing, &str) -> std::result::Result<Vec<u8>, String>,
    /// PLOT / EXPORTPDF (optional: builds without a plotter report "not available").
    pub plot: Option<PlotHook>,
}

static IO: OnceLock<IoHooks> = OnceLock::new();

pub fn set_io(h: IoHooks) {
    let _ = IO.set(h);
}
pub fn io() -> Option<&'static IoHooks> {
    IO.get()
}

/// Hand finished file bytes to the user where there is no file system (the browser: a download).
/// Arguments: file name, MIME type, bytes. `Ok` only once the delivery was actually started.
pub type DeliverHook = fn(&str, &str, &[u8]) -> std::result::Result<(), String>;

static DELIVER: OnceLock<DeliverHook> = OnceLock::new();

/// Install the host's file delivery (the web app: a browser download). Saving uses it on wasm;
/// anything else that produces a file on the web (plots) can use [`deliver`] too.
pub fn set_deliver(h: DeliverHook) {
    let _ = DELIVER.set(h);
}
pub fn deliver() -> Option<DeliverHook> {
    DELIVER.get().copied()
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("new", "New Drawing...", run_new)
            .menu(&["File", "New Drawing..."])
            .key("Cmd+N")
            .alias(&["qnew"])
            .params("{metric?: bool}")
            .enabled(always)
            .noundo(),
        CommandSpec::new("open", "Open...", run_open)
            .menu(&["File", "Open..."])
            .key("Cmd+O")
            .params("{path} | {data: base64, name}")
            .enabled(always)
            .noundo(),
        CommandSpec::new("close", "Close", run_close).menu(&["File", "Close"]).key("Cmd+W").params("{index?}").noundo(),
        CommandSpec::new("drawing.revision", "Drawing Revision", run_revision)
            .params("{path? (default: the active drawing's file)} → {status: current|changed|unrecorded, sha256, bytes, recorded?} (compares the file on disk with the revision its last save recorded in <file>.aadrev.json)")
            .noundo(),
        CommandSpec::new("closeall", "Close All", run_closeall).menu(&["File", "Close All"]).enabled(always).noundo(),
        CommandSpec::new("qsave", "Save", run_qsave_reporting).menu(&["File", "Save"]).key("Cmd+S").alias(&["save"]).params("{path?}").noundo(),
        CommandSpec::new("saveas", "Save As...", run_saveas_reporting)
            .menu(&["File", "Save As..."])
            .key("Cmd+Shift+S")
            .params("{path, format?: dxf|dwg}")
            .noundo(),
        CommandSpec::new("document.switch", "Switch Drawing", run_switch).params("{index}").enabled(always).noundo(),
        CommandSpec::new("document.bytes", "Drawing as bytes", run_bytes).params("{format?: dxf} → {data: base64}").noundo(),
    ]
}

fn run_new(s: &mut Session, p: &Value) -> Result<Value> {
    let i = s.new_drawing(bool_or(p, "metric", false));
    Ok(json!({ "index": i, "title": s.docs.get(i).map(|d| d.title.clone()) }))
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

pub(crate) fn open_bytes(s: &mut Session, bytes: &[u8], name: &str, path: Option<String>) -> Result<usize> {
    let hooks = io().ok_or_else(|| bad("open", "file formats are not available in this build"))?;
    let d = (hooks.read)(bytes, name).map_err(|e| bad("open", e))?;
    import_drawing(s, d, name, path)
}

/// A drawing read from a file and flattened to 2D, ready to become a document. Everything slow
/// (the copy kept as the saved state, the flatten) is done when it is made, so a host can do it
/// on a worker thread and only [`finish_open`] runs on the thread that owns the session.
pub struct Prepared {
    drawing: Drawing,
    original: std::sync::Arc<Drawing>,
    report: crate::flatten::FlattenReport,
    units_unset: bool,
}

/// Flatten `d` to 2D. The flattening is applied to a working copy; the drawing as read stays the
/// "saved" state and one undo step, so the import can be reverted and the file on disk is only
/// replaced by an explicit save.
pub fn prepare(mut d: Drawing) -> Prepared {
    let original = std::sync::Arc::new(d.clone());
    let report = crate::flatten::flatten_drawing(&mut d);
    let units_unset = crate::annot::drawing_unit(&d).is_none();
    Prepared { drawing: d, original, report, units_unset }
}

/// Read, parse and flatten a drawing file without a session.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_drawing_file(path: &str) -> std::result::Result<Prepared, String> {
    let hooks = io().ok_or_else(|| "file formats are not available in this build".to_string())?;
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    (hooks.read)(&bytes, path).map(prepare)
}

/// Make a drawing read by [`read_drawing_file`] the active document, as `open` with a path does.
pub fn finish_open(s: &mut Session, p: Prepared, path: &str) -> Result<usize> {
    finish_import(s, p, path, Some(path.to_string()))
}

/// Open a drawing read from a file, flattening it to 2D first.
pub(crate) fn import_drawing(s: &mut Session, d: Drawing, name: &str, path: Option<String>) -> Result<usize> {
    finish_import(s, prepare(d), name, path)
}

fn finish_import(s: &mut Session, p: Prepared, name: &str, path: Option<String>) -> Result<usize> {
    let Prepared { drawing, original, report, units_unset } = p;
    if units_unset {
        s.echo(
            "The drawing's units are not set or not a length unit AAD knows. Choose them in the Units popup (status bar); nothing was assumed."
                .to_string(),
        );
    }
    let i = s.open_drawing(drawing, &file_name(name), path);
    if report.flattened > 0 {
        let st = s.state_mut()?;
        st.saved = original.clone();
        st.undo.push(crate::Snapshot { label: "Flatten on import".into(), doc: original, selection: Vec::new() });
    }
    if report.has_findings() {
        s.echo(report.message());
    }
    s.last_flatten = Some(report);
    Ok(i)
}

fn run_open(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(_path) = str_param(p, "path") {
        #[cfg(not(target_arch = "wasm32"))]
        let path = _path;
        #[cfg(not(target_arch = "wasm32"))]
        {
            let bytes = std::fs::read(path).map_err(|e| bad("open", format!("{path}: {e}")))?;
            let i = open_bytes(s, &bytes, path, Some(path.to_string()))?;
            return Ok(
                json!({ "index": i, "entities": s.docs.get(i).map(|d| d.doc.entity_count()), "flatten": s.last_flatten.as_ref().map(|r| r.to_json()) }),
            );
        }
        #[cfg(target_arch = "wasm32")]
        return Err(bad("open", "paths are not available on the web; pass `data`"));
    }
    let data = str_param(p, "data").ok_or_else(|| bad("open", "`path` or `data` (base64) is required"))?;
    let bytes = base64_decode(data).ok_or_else(|| bad("open", "invalid base64"))?;
    let name = str_param(p, "name").unwrap_or("Drawing.dxf");
    let i = open_bytes(s, &bytes, name, None)?;
    Ok(json!({ "index": i, "flatten": s.last_flatten.as_ref().map(|r| r.to_json()) }))
}

fn run_revision(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        Err(bad("drawing.revision", "files are not available on the web"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = match str_param(p, "path") {
            Some(path) => path.to_string(),
            None => s.state()?.path.clone().ok_or_else(|| bad("drawing.revision", "the drawing has no file yet; save it or pass `path`"))?,
        };
        crate::revision::check(&path).map_err(|e| bad("drawing.revision", e))
    }
}

fn run_close(s: &mut Session, p: &Value) -> Result<Value> {
    let i = p.get("index").and_then(Value::as_u64).map(|i| i as usize).unwrap_or(s.active);
    if i >= s.docs.len() {
        return Err(bad("close", "no such drawing"));
    }
    s.cancel();
    s.docs.remove(i);
    if i < s.active {
        // An earlier drawing went away: the active one shifted down by one.
        s.active -= 1;
    }
    if s.active >= s.docs.len() {
        s.active = s.docs.len().saturating_sub(1);
    }
    ok()
}

fn run_closeall(s: &mut Session, _p: &Value) -> Result<Value> {
    s.cancel();
    s.docs.clear();
    s.active = 0;
    ok()
}

pub(crate) fn save_to(s: &mut Session, path: &str) -> Result<usize> {
    let hooks = io().ok_or_else(|| bad("save", "file formats are not available in this build"))?;
    let bytes = (hooks.write)(&super::xref::without_overlays(s.doc()?), path).map_err(|e| bad("save", e))?;
    let n = commit_save(s, path, &bytes, store)?;
    #[cfg(not(target_arch = "wasm32"))]
    if is_drawing_file(path) {
        let entities = s.doc().map(|d| d.entity_count()).unwrap_or(0);
        if let Err(e) = crate::revision::record(path, &bytes, entities) {
            s.echo(format!("Drawing revision not recorded: {e}"));
        }
    }
    Ok(n)
}

/// Put saved bytes where the user gets them: natively the file at `path` (written atomically); on
/// the web, which has no file system, a browser download named after `path` via [`deliver`].
fn store(path: &str, bytes: &[u8]) -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Keep the file being replaced as `<path>.bak` the first time it is overwritten, so the
        // original is never lost to a save (or to a repair that is saved over it).
        let backup = format!("{path}.bak");
        if std::path::Path::new(path).is_file() && !std::path::Path::new(&backup).exists() {
            std::fs::copy(path, &backup).map_err(|e| bad("save", format!("could not back up {path} to {backup}: {e}")))?;
        }
        let tmp = format!("{path}.aad-tmp");
        std::fs::write(&tmp, bytes).map_err(|e| bad("save", format!("{path}: {e}")))?;
        std::fs::rename(&tmp, path).map_err(|e| bad("save", format!("{path}: {e}")))?;
        Ok(())
    }
    #[cfg(target_arch = "wasm32")]
    {
        let name = file_name(path);
        let deliver = deliver().ok_or_else(|| bad("save", "saving files is not available in this build; use document.bytes"))?;
        deliver(&name, "application/octet-stream", bytes).map_err(|e| bad("save", format!("{name}: {e}")))
    }
}

/// Store the bytes, then mark the drawing saved under `path`. If storing fails the drawing keeps
/// its unsaved changes, file name and title. On the web a started download counts as saved: the
/// browser owns the file from there (we can't see whether the user keeps it), and the name stays so
/// a later QSAVE downloads the drawing again under it. An image or PDF export (SVG, PNG, PDF) only
/// stores the file: the drawing keeps its name, title and unsaved changes, so closing still asks.
fn commit_save(s: &mut Session, path: &str, bytes: &[u8], store: impl FnOnce(&str, &[u8]) -> Result<()>) -> Result<usize> {
    s.state()?;
    store(path, bytes)?;
    if !is_drawing_file(path) {
        return Ok(bytes.len());
    }
    let st = s.state_mut()?;
    st.saved = st.doc.clone();
    st.path = Some(path.to_string());
    st.title = file_name(path);
    Ok(bytes.len())
}

/// The save result; on the web it also names the file the browser downloaded.
fn saved(path: &str, bytes: usize) -> Value {
    let mut r = json!({ "path": path, "bytes": bytes });
    if cfg!(target_arch = "wasm32") {
        r["download"] = json!(file_name(path));
    }
    r
}

fn run_save(s: &mut Session, p: &Value) -> Result<Value> {
    let path = match str_param(p, "path") {
        Some(p) => p.to_string(),
        None => s.state()?.path.clone().ok_or_else(|| bad("qsave", "drawing has no file name yet; use saveas {path}"))?,
    };
    let n = save_to(s, &path)?;
    Ok(saved(&path, n))
}

fn run_saveas(s: &mut Session, p: &Value) -> Result<Value> {
    let path = str_param(p, "path").ok_or_else(|| bad("saveas", "`path` is required"))?.to_string();
    let n = save_to(s, &path)?;
    Ok(saved(&path, n))
}

fn run_switch(s: &mut Session, p: &Value) -> Result<Value> {
    let i = p.get("index").and_then(Value::as_u64).ok_or_else(|| bad("document.switch", "`index` is required"))? as usize;
    if i >= s.docs.len() {
        return Err(bad("document.switch", "no such drawing"));
    }
    s.cancel();
    s.active = i;
    ok()
}

fn run_bytes(s: &mut Session, p: &Value) -> Result<Value> {
    let hooks = io().ok_or_else(|| bad("document.bytes", "file formats are not available in this build"))?;
    let fmt = str_param(p, "format").unwrap_or("dxf");
    let lost = not_saved(s.doc()?);
    let bytes = (hooks.write)(&super::xref::without_overlays(s.doc()?), &format!("drawing.{fmt}")).map_err(|e| bad("document.bytes", e))?;
    let mut r = json!({ "data": base64_encode(&bytes), "bytes": bytes.len() });
    if is_drawing_file(&format!("drawing.{fmt}")) {
        report_not_saved(s, &mut r, &lost);
    }
    Ok(r)
}

/// Entities a DXF or DWG file doesn't keep: those of object types AAD doesn't model
/// (MLINE, 3DSOLID, MULTILEADER… read from other programs' files and kept in the drawing, but
/// not written), counted by DXF type across model space, layouts and block definitions.
pub fn not_saved(d: &Drawing) -> std::collections::BTreeMap<String, usize> {
    let mut out: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let stores = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities));
    for e in stores.flat_map(|s| s.iter()) {
        if let aad_doc::EntityKind::Unknown(u) = &e.kind {
            let n = out.entry(u.dxf_type.clone()).or_insert(0);
            *n = n.saturating_add(1);
        }
    }
    out
}

/// The command-line warning for [`not_saved`] entities, or `None` when the file keeps everything.
pub fn not_saved_message(lost: &std::collections::BTreeMap<String, usize>) -> Option<String> {
    if lost.is_empty() {
        return None;
    }
    let list: Vec<String> = lost.iter().map(|(t, n)| format!("{n} {t}")).collect();
    Some(format!("Not saved (AAD can't write these object types yet): {}", list.join(", ")))
}

/// Whether `path` is saved as a drawing (DXF, DWG) rather than exported as an image.
fn is_drawing_file(path: &str) -> bool {
    let ext = std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "dxf" | "dwg" | "")
}

/// Say on the command line what the file couldn't keep, and list it in the result's `notSaved`.
fn report_not_saved(s: &mut Session, r: &mut Value, lost: &std::collections::BTreeMap<String, usize>) {
    let Some(msg) = not_saved_message(lost) else { return };
    s.echo(msg);
    if let Some(o) = r.as_object_mut() {
        o.insert("notSaved".into(), json!(lost));
    }
}

/// A save split so the slow part (writing the format and the file) can run on a worker thread:
/// [`begin_save`] on the session's thread, [`SaveJob::run`] anywhere, [`finish_save`] back on the
/// session's thread. The job holds the drawing as it was when the save began, so edits made
/// meanwhile stay unsaved.
#[cfg(not(target_arch = "wasm32"))]
pub struct SaveJob {
    doc: std::sync::Arc<Drawing>,
    index: usize,
    path: String,
    lost: std::collections::BTreeMap<String, usize>,
}

/// Plan a save to `path`, or to the drawing's own file when `path` is `None`.
#[cfg(not(target_arch = "wasm32"))]
pub fn begin_save(s: &Session, path: Option<&str>) -> Result<SaveJob> {
    let path = match path {
        Some(p) => p.to_string(),
        None => s.state()?.path.clone().ok_or_else(|| bad("qsave", "drawing has no file name yet; use saveas {path}"))?,
    };
    let state = s.state()?;
    Ok(SaveJob { doc: state.doc.clone(), index: s.active, lost: not_saved(&state.doc), path })
}

#[cfg(not(target_arch = "wasm32"))]
impl SaveJob {
    /// Write the file; returns the byte count.
    pub fn run(&self) -> std::result::Result<usize, String> {
        let hooks = io().ok_or_else(|| "file formats are not available in this build".to_string())?;
        let bytes = (hooks.write)(&super::xref::without_overlays(&self.doc), &self.path)?;
        store(&self.path, &bytes).map_err(|e| e.to_string())?;
        if is_drawing_file(&self.path) {
            // Advisory: a revision that cannot be written never fails the save.
            let _ = crate::revision::record(&self.path, &bytes, self.doc.entity_count());
        }
        Ok(bytes.len())
    }
}

/// Mark the drawing saved under the job's path and report what the file could not keep.
#[cfg(not(target_arch = "wasm32"))]
pub fn finish_save(s: &mut Session, job: &SaveJob, bytes: usize) -> Result<Value> {
    if is_drawing_file(&job.path) {
        let st = s.docs.get_mut(job.index).ok_or_else(|| bad("save", "the drawing was closed while it was being saved"))?;
        st.saved = job.doc.clone();
        st.path = Some(job.path.clone());
        st.title = file_name(&job.path);
    }
    let mut r = saved(&job.path, bytes);
    after_save(s, &mut r, &job.lost);
    Ok(r)
}

/// Save, then report the entities the file couldn't keep (nothing is dropped silently).
fn save_reporting(s: &mut Session, p: &Value, run: fn(&mut Session, &Value) -> Result<Value>) -> Result<Value> {
    let lost = s.doc().map(not_saved).unwrap_or_default();
    let mut r = run(s, p)?;
    after_save(s, &mut r, &lost);
    Ok(r)
}

/// The reports that follow a saved drawing file: what the file couldn't keep, and the ShilpiDB sidecar.
fn after_save(s: &mut Session, r: &mut Value, lost: &std::collections::BTreeMap<String, usize>) {
    if r.get("path").and_then(Value::as_str).is_some_and(is_drawing_file) {
        report_not_saved(s, r, lost);
        if let Some(path) = r.get("path").and_then(Value::as_str)
            && let Err(e) = super::vdb::write_sidecar_if_enabled(s, std::path::Path::new(path))
        {
            s.echo(format!("ShilpiDB sidecar: {e}"));
        }
    }
}

fn run_qsave_reporting(s: &mut Session, p: &Value) -> Result<Value> {
    save_reporting(s, p, run_save)
}

fn run_saveas_reporting(s: &mut Session, p: &Value) -> Result<Value> {
    save_reporting(s, p, run_saveas)
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk.first().copied().unwrap_or(0), chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(B64[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };
        buf = (buf << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    fn active_title(s: &Session) -> String {
        s.state().unwrap().title.clone()
    }

    #[test]
    fn closing_earlier_inactive_drawing_keeps_active_drawing() {
        let mut s = Session::new();
        s.execute("new", &json!({})).unwrap();
        s.execute("new", &json!({})).unwrap();
        s.execute("document.switch", &json!({"index": 1})).unwrap();
        assert_eq!(active_title(&s), "Drawing2");
        s.execute("close", &json!({"index": 0})).unwrap();
        assert_eq!(active_title(&s), "Drawing2");
        assert_eq!(s.docs.len(), 2);
    }

    #[test]
    fn closing_later_inactive_drawing_keeps_active_drawing() {
        let mut s = Session::new();
        s.execute("new", &json!({})).unwrap();
        s.execute("new", &json!({})).unwrap();
        s.execute("document.switch", &json!({"index": 1})).unwrap();
        s.execute("close", &json!({"index": 2})).unwrap();
        assert_eq!(active_title(&s), "Drawing2");
    }
}

#[cfg(test)]
mod pr193_tests {
    use super::*;

    #[test]
    fn failed_store_keeps_unsaved_changes_and_delivered_save_clears_them() {
        let mut s = Session::new();
        s.cmdline("circle 4,4 1").unwrap();
        let st = s.state().unwrap();
        let title = st.title.clone();
        assert!(st.is_dirty() && st.path.is_none());

        // The file route is unavailable or fails (the web without a download): no success, still dirty.
        let failed = commit_save(&mut s, "a/Owned.dxf", b"0\nEOF\n", |_, _| Err(bad("save", "no download")));
        assert!(failed.is_err());
        let st = s.state().unwrap();
        assert!(st.is_dirty(), "a save that delivered nothing must keep the unsaved state");
        assert_eq!((st.path.as_deref(), st.title.as_str()), (None, title.as_str()));

        // The file route took the bytes (written, or a download started): saved under that name.
        let mut got = None;
        let n = commit_save(&mut s, "a/Owned.dxf", b"0\nEOF\n", |p, b| {
            got = Some((p.to_string(), b.len()));
            Ok(())
        })
        .unwrap();
        assert_eq!((n, got), (6, Some(("a/Owned.dxf".to_string(), 6))));
        let st = s.state().unwrap();
        assert!(!st.is_dirty());
        assert_eq!((st.path.as_deref(), st.title.as_str()), (Some("a/Owned.dxf"), "Owned.dxf"));
        assert_eq!(saved("a/Owned.dxf", n)["bytes"], 6);
    }
}
