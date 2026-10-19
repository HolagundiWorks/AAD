//! `takeoff.record`: the take-off as ShilpiDB records, for O-CoreOp to read.
//!
//! The contract is `TAKEOFF-SHILPIDB.md` in the O-CoreOp repository, version 1 (revised 2026-10-19). One database
//! holds many drawings, so every record id is `(kind << 56) | (slot << 32) | n`: the kind (1 header, 3 wall,
//! 4 opening, 5 column, 6 lintel, 7 room, 8 slab, 9 annotated-PDF chunk), the drawing's slot and the item's
//! number. The slot comes from the drawing id (`slot_for`), is probed past a slot another drawing holds, and is
//! kept in the drawing so it never changes. The payload of a row is the bridge row exactly as `bridge.export`
//! writes it; the box is in millimetres. Only measured facts are recorded.
//!
//! The writer's protocol has no transactions, so the header is the completion marker: it is written with
//! `"state": "recording"`, then the rows, then the stale rows of this slot are deleted, then the annotated PDF,
//! then the header again with `"state": "complete"`, then the database is saved. A reader refuses a drawing that
//! is still `recording`.
//!
//! By default the take-off goes to the shared `shilpid` (`127.0.0.1:7420`, started when nothing answers), where
//! O-CoreOp reads it within seconds. `server` names another one; `path` or `file: true` writes a `.vdb` file
//! instead (`file: true` puts it beside the drawing).

use serde_json::{Map, Value, json};

use super::bridge::{self, Collected, Item, KIND_COLUMN, KIND_LINTEL, KIND_OPENING, KIND_ROOM, KIND_SLAB, KIND_WALL};
use super::*;
use crate::{Result, Session};

const FORMAT: &str = "hcw-takeoff-shilpi";
const KIND_HEADER: u8 = 1;
/// An annotated-PDF chunk.
pub(crate) const KIND_PDF: u8 = 9;
/// The kinds this version writes. Records of any other kind belong to a later version and are left alone.
const OWN_KINDS: [u8; 8] = [KIND_HEADER, KIND_WALL, KIND_OPENING, KIND_COLUMN, KIND_LINTEL, KIND_ROOM, KIND_SLAB, KIND_PDF];
/// Rows one recording writes; a larger take-off is cut and the result says so.
const MAX_ROWS: usize = 500_000;
/// Largest PDF chunk, and the most a stored PDF may hold (contract section 6).
const CHUNK: usize = 512 * 1024;
const MAX_PDF: usize = 64 * 1024 * 1024;
/// The shared server's address and where it keeps its data.
pub(crate) const SHARED_ADDR: &str = "127.0.0.1:7420";
#[cfg(not(target_arch = "wasm32"))]
const GATEWAY_ADDR: &str = "127.0.0.1:7421";
#[cfg(not(target_arch = "wasm32"))]
const BATCH: usize = 500;
/// The project-store key that keeps the drawing's slot.
const SLOT_KEY: &str = "takeoffSlot";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("takeoff.record", "Record Take-off in ShilpiDB", run_record)
            .menu(&["Architecture", "Take-off", "Record in ShilpiDB"])
            .params("{server?: \"host:port\" of a shilpid (default: the shared one on 127.0.0.1:7420, started if nothing answers), path?: .vdb file written instead of a server, file?: true for <drawing>.takeoff.vdb beside the saved drawing, annotate?: draw and store the annotated PDF (default true), paper?: \"A3\", level?, bearing?} → {server | path, headerId, slot, state, counts, written, added, changed?, unchanged?, deleted, pdf?, skipped, warnings}")
            .noundo(),
    ]
}

// ---------------- ids ----------------

fn fnv32(bytes: &[u8]) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for b in bytes {
        h ^= u32::from(*b);
        h = h.wrapping_mul(16_777_619);
    }
    h
}

/// The slot a drawing id starts from: 32-bit FNV-1a folded to 24 bits, never 0.
pub(crate) fn slot_for(drawing_id: &str) -> u32 {
    let h = fnv32(drawing_id.as_bytes());
    let slot = (h ^ (h >> 24)) & 0x00FF_FFFF;
    if slot == 0 { 1 } else { slot }
}

/// The slot after `slot`: past the last one it wraps to 1.
fn next_slot(slot: u32) -> u32 {
    if slot >= 0x00FF_FFFF { 1 } else { slot + 1 }
}

/// The record id of item number `n` of `kind` in `slot`.
pub(crate) fn record_id(kind: u8, slot: u32, n: u32) -> u64 {
    (u64::from(kind) << 56) | (u64::from(slot & 0x00FF_FFFF) << 32) | u64::from(n)
}

pub(crate) fn header_id(slot: u32) -> u64 {
    record_id(KIND_HEADER, slot, 0)
}

pub(crate) fn kind_of(id: u64) -> u8 {
    (id >> 56) as u8
}

pub(crate) fn slot_of(id: u64) -> u32 {
    ((id >> 32) & 0x00FF_FFFF) as u32
}

/// An item's number inside its slot: the entity handle when it fits in 32 bits, else the FNV-1a of its hex text.
fn item_number(handle: u64) -> u32 {
    u32::try_from(handle).unwrap_or_else(|_| fnv32(aad_doc::Handle(handle).hex().as_bytes()))
}

fn kind_name(kind: u8) -> &'static str {
    match kind {
        KIND_WALL => "walls",
        KIND_OPENING => "openings",
        KIND_COLUMN => "columns",
        KIND_LINTEL => "lintels",
        KIND_ROOM => "rooms",
        KIND_SLAB => "slabs",
        _ => "other",
    }
}

// ---------------- records ----------------

/// A record as it is written: id, box in millimetres, payload bytes.
#[derive(Clone)]
pub(crate) struct Rec {
    pub id: u64,
    pub bbox: [f64; 4],
    pub payload: Vec<u8>,
}

/// The annotated PDF as it travels: the chunks and the header's `annotated_pdf` entry.
pub(crate) struct Pdf {
    pub chunks: Vec<Rec>,
    pub meta: Value,
}

/// Split a PDF into chunk records (kind 9, `n` from 0, zero box) and describe it.
pub(crate) fn chunk_pdf(bytes: &[u8], slot: u32, pages: u32, generated: &str) -> Result<Pdf> {
    if bytes.len() > MAX_PDF {
        return Err(bad("takeoff.record", format!("the annotated drawing is {} bytes; the most that is stored is {MAX_PDF}", bytes.len())));
    }
    let mut chunks = Vec::new();
    for (i, part) in bytes.chunks(CHUNK).enumerate() {
        let n = u32::try_from(i).map_err(|_| bad("takeoff.record", "too many chunks"))?;
        chunks.push(Rec { id: record_id(KIND_PDF, slot, n), bbox: [0.0; 4], payload: part.to_vec() });
    }
    let meta =
        json!({ "sha256": crate::revision::sha256_hex(bytes), "bytes": bytes.len(), "chunks": chunks.len(), "pages": pages, "generated": generated });
    Ok(Pdf { chunks, meta })
}

/// The header payload: the bridge file's `source`, `project`, `levels`, `beam_depths_mm` and `opening_schedule`, the
/// state, the AORMS project the drawing belongs to and, once stored, the annotated PDF.
pub(crate) fn header(root: &Map<String, Value>, project_office_id: Option<&str>, complete: bool, pdf: Option<&Value>) -> Value {
    let mut source = Map::new();
    source.insert("app".into(), json!("O-Draft"));
    if let Some(src) = root.get("source") {
        for key in ["drawing", "drawing_id", "exported"] {
            if let Some(v) = src.get(key) {
                source.insert(key.into(), v.clone());
            }
        }
        if let Some(sha) = src.get("revision").and_then(|r| r.get("sha256")) {
            source.insert("revision_sha256".into(), sha.clone());
        }
    }
    if let Some(id) = project_office_id.filter(|s| !s.is_empty()) {
        source.insert("project_office_id".into(), json!(id));
    }
    let mut h = Map::new();
    h.insert("format".into(), json!(FORMAT));
    h.insert("version".into(), json!(1));
    h.insert("units".into(), json!("mm"));
    h.insert("state".into(), json!(if complete { "complete" } else { "recording" }));
    h.insert("source".into(), Value::Object(source));
    for key in ["project", "levels", "beam_depths_mm", "opening_schedule"] {
        if let Some(v) = root.get(key) {
            h.insert(key.into(), v.clone());
        }
    }
    if let (true, Some(p)) = (complete, pdf) {
        h.insert("annotated_pdf".into(), p.clone());
    }
    Value::Object(h)
}

/// What a recording would write: the rows of one slot, and what it left out.
pub(crate) struct Built {
    pub rows: Vec<Rec>,
    pub counts: std::collections::BTreeMap<&'static str, usize>,
    pub skipped: Vec<String>,
}

pub(crate) fn build(c: &Collected, slot: u32) -> Result<Built> {
    let mut warnings: Vec<String> = Vec::new();
    let mut skipped = Vec::new();
    let mut counts = std::collections::BTreeMap::new();
    let mut rows: Vec<Rec> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut cut = false;
    for Item { kind, n, row, bbox } in &c.items {
        if rows.len() >= MAX_ROWS {
            cut = true;
            break;
        }
        let id = record_id(*kind, slot, item_number(*n));
        if !seen.insert(id) {
            let name = row.get("ref").and_then(Value::as_str).unwrap_or("?");
            skipped.push(format!("{} {name}: another row has the same number", kind_name(*kind)));
            continue;
        }
        let payload = serde_json::to_vec(row).map_err(|e| bad("takeoff.record", e.to_string()))?;
        *counts.entry(kind_name(*kind)).or_insert(0usize) += 1;
        rows.push(Rec { id, bbox: *bbox, payload });
    }
    if cut {
        warnings.push(format!("more than {MAX_ROWS} rows: the rest were not recorded"));
    }
    rows.sort_by_key(|r| r.id);
    Ok(Built { rows, counts, skipped })
}

// ---------------- the database ----------------

/// A ShilpiDB database, a file held in memory or a running server.
trait Sink {
    fn ids(&mut self) -> Result<Vec<u64>>;
    fn payload(&mut self, id: u64) -> Result<Option<Vec<u8>>>;
    fn put(&mut self, recs: &[Rec]) -> Result<()>;
    fn delete(&mut self, id: u64) -> Result<bool>;
    fn save(&mut self) -> Result<()>;
}

fn vdb_bbox(b: [f64; 4]) -> aad_vdb::Bbox {
    aad_vdb::Bbox::from_corners(b[0], b[1], b[2], b[3])
}

#[cfg(not(target_arch = "wasm32"))]
struct FileSink {
    path: std::path::PathBuf,
    records: std::collections::BTreeMap<u64, aad_vdb::VdbRecord>,
}

#[cfg(not(target_arch = "wasm32"))]
impl FileSink {
    /// Open a take-off database file. An existing file must be a take-off database (some record is a header) or be
    /// empty, so an entity store is never overwritten by mistake.
    fn open(path: &std::path::Path) -> Result<FileSink> {
        let mut records = std::collections::BTreeMap::new();
        if path.exists() {
            let store = aad_vdb::load_path(path).map_err(|e| bad("takeoff.record", format!("{}: {e}", path.display())))?;
            if !store.is_empty() && !store.records().iter().any(|r| kind_of(r.id) == KIND_HEADER) {
                return Err(bad(
                    "takeoff.record",
                    format!("{} is not a take-off database (it has no header record); choose another file", path.display()),
                ));
            }
            records.extend(store.records().iter().map(|r| (r.id, r.clone())));
        }
        Ok(FileSink { path: path.to_path_buf(), records })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Sink for FileSink {
    fn ids(&mut self) -> Result<Vec<u64>> {
        Ok(self.records.keys().copied().collect())
    }
    fn payload(&mut self, id: u64) -> Result<Option<Vec<u8>>> {
        Ok(self.records.get(&id).map(|r| r.payload.clone()))
    }
    fn put(&mut self, recs: &[Rec]) -> Result<()> {
        for r in recs {
            self.records.insert(r.id, aad_vdb::VdbRecord { id: r.id, bbox: vdb_bbox(r.bbox), payload: r.payload.clone() });
        }
        Ok(())
    }
    fn delete(&mut self, id: u64) -> Result<bool> {
        Ok(self.records.remove(&id).is_some())
    }
    fn save(&mut self) -> Result<()> {
        let store = aad_vdb::VdbStore::from_records(self.records.values().cloned().collect()).map_err(|e| bad("takeoff.record", e.to_string()))?;
        aad_vdb::save_path(&self.path, &store).map_err(|e| bad("takeoff.record", format!("{}: {e}", self.path.display())))
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct ServerSink {
    addr: String,
    client: aad_vdb::VdbClient,
}

#[cfg(not(target_arch = "wasm32"))]
impl ServerSink {
    fn fail(&self, e: aad_vdb::ClientError) -> crate::EngineError {
        bad("takeoff.record", format!("{}: {e}", self.addr))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Sink for ServerSink {
    fn ids(&mut self) -> Result<Vec<u64>> {
        self.client.list().map_err(|e| bad("takeoff.record", format!("{}: {e}", self.addr)))
    }
    fn payload(&mut self, id: u64) -> Result<Option<Vec<u8>>> {
        self.client.get(id).map(|r| r.map(|r| r.payload)).map_err(|e| self.fail(e))
    }
    fn put(&mut self, recs: &[Rec]) -> Result<()> {
        for chunk in recs.chunks(BATCH) {
            let batch = chunk.iter().map(|r| (r.id, vdb_bbox(r.bbox), r.payload.clone())).collect();
            self.client.insert_many(batch).map_err(|e| bad("takeoff.record", format!("{}: {e}", self.addr)))?;
        }
        Ok(())
    }
    fn delete(&mut self, id: u64) -> Result<bool> {
        self.client.delete(id).map_err(|e| bad("takeoff.record", format!("{}: {e}", self.addr)))
    }
    fn save(&mut self) -> Result<()> {
        self.client.save().map_err(|e| bad("takeoff.record", format!("{}: {e}", self.addr)))
    }
}

/// The drawing id in a header payload, if it parses as a take-off header.
fn drawing_id_of(payload: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(payload).ok()?;
    v.get("source")?.get("drawing_id")?.as_str().map(str::to_string)
}

/// The slot this drawing records into: the slot kept in the drawing when it is still ours, else the first free one
/// from `slot_for`, probing past a slot another drawing holds.
fn find_slot(sink: &mut dyn Sink, drawing_id: &str, kept: Option<u32>) -> Result<u32> {
    let mut slot = kept.filter(|s| (1..=0x00FF_FFFF).contains(s)).unwrap_or_else(|| slot_for(drawing_id));
    for _ in 0..=0x00FF_FFFFu32 {
        match sink.payload(header_id(slot))? {
            None => return Ok(slot),
            Some(p) if drawing_id_of(&p).as_deref() == Some(drawing_id) => return Ok(slot),
            Some(_) => slot = next_slot(slot),
        }
    }
    Err(bad("takeoff.record", "every slot in the database is taken"))
}

/// What a recording did.
struct Outcome {
    slot: u32,
    added: usize,
    changed: Option<usize>,
    unchanged: Option<usize>,
    deleted: usize,
    written: usize,
}

/// The writer's protocol (contract section 4.4) against any database.
fn write_slot(
    sink: &mut dyn Sink,
    c: &Collected,
    built: &Built,
    office_id: Option<&str>,
    kept: Option<u32>,
    pdf: Option<(&[u8], u32, String)>,
    compare: bool,
) -> Result<Outcome> {
    let mut slot = find_slot(sink, &c.drawing_id, kept)?;
    let root = &c.root;
    // Claim the slot: write the header, read it back, and take the next slot if another writer won it.
    let mut claims = 0;
    loop {
        let rec = |slot: u32| -> Result<Rec> {
            let payload = serde_json::to_vec(&header(root, office_id, false, None)).map_err(|e| bad("takeoff.record", e.to_string()))?;
            Ok(Rec { id: header_id(slot), bbox: [0.0; 4], payload })
        };
        sink.put(&[rec(slot)?])?;
        match sink.payload(header_id(slot))? {
            Some(p) if drawing_id_of(&p).as_deref() == Some(c.drawing_id.as_str()) => break,
            _ => {
                claims += 1;
                if claims > 64 {
                    return Err(bad("takeoff.record", "could not claim a slot"));
                }
                slot = next_slot(slot);
            }
        }
    }
    // The built rows were numbered for the slot found before the claim; renumber them if it moved.
    let rows: Vec<Rec> = if built.rows.first().is_none_or(|r| slot_of(r.id) == slot) {
        built.rows.clone()
    } else {
        built.rows.iter().map(|r| Rec { id: (r.id & !(0x00FF_FFFFu64 << 32)) | (u64::from(slot) << 32), ..r.clone() }).collect()
    };
    let existing: std::collections::HashSet<u64> = sink.ids()?.into_iter().filter(|id| slot_of(*id) == slot).collect();
    let (mut changed, mut unchanged) = (0usize, 0usize);
    if compare {
        for r in &rows {
            if existing.contains(&r.id) {
                if sink.payload(r.id)?.as_deref() == Some(r.payload.as_slice()) {
                    unchanged += 1;
                } else {
                    changed += 1;
                }
            }
        }
    }
    let added = rows.iter().filter(|r| !existing.contains(&r.id)).count();
    sink.put(&rows)?;
    let new_ids: std::collections::HashSet<u64> = rows.iter().map(|r| r.id).collect();
    let mut deleted = 0usize;
    for id in &existing {
        let k = kind_of(*id);
        if *id != header_id(slot) && OWN_KINDS.contains(&k) && k != KIND_PDF && !new_ids.contains(id) && sink.delete(*id)? {
            deleted += 1;
        }
    }
    // The annotated PDF: the old chunks go, the new ones are written.
    for id in &existing {
        if kind_of(*id) == KIND_PDF {
            sink.delete(*id)?;
        }
    }
    let mut meta = None;
    if let Some((bytes, pages, generated)) = pdf {
        let p = chunk_pdf(bytes, slot, pages, &generated)?;
        sink.put(&p.chunks)?;
        meta = Some(p.meta);
    }
    let done = serde_json::to_vec(&header(root, office_id, true, meta.as_ref())).map_err(|e| bad("takeoff.record", e.to_string()))?;
    sink.put(&[Rec { id: header_id(slot), bbox: [0.0; 4], payload: done }])?;
    sink.save()?;
    Ok(Outcome { slot, added, changed: compare.then_some(changed), unchanged: compare.then_some(unchanged), deleted, written: rows.len() + 1 })
}

// ---------------- the shared server ----------------

/// Find a ShilpiDB program: `SHILPIDB_BIN` (a folder), `%LocalAppData%\ShilpiDB\bin`, then `PATH`.
#[cfg(not(target_arch = "wasm32"))]
fn find_program(name: &str) -> Option<std::path::PathBuf> {
    let exe = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(d) = std::env::var_os("SHILPIDB_BIN") {
        dirs.push(d.into());
    }
    if let Some(d) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(std::path::Path::new(&d).join("ShilpiDB").join("bin"));
    }
    if let Some(p) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&p));
    }
    dirs.into_iter().map(|d| d.join(&exe)).find(|p| p.is_file())
}

#[cfg(not(target_arch = "wasm32"))]
fn answers(addr: &str) -> bool {
    use std::net::ToSocketAddrs;
    addr.to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .is_some_and(|a| std::net::TcpStream::connect_timeout(&a, std::time::Duration::from_millis(400)).is_ok())
}

/// Make sure the shared server answers: when nothing does, start `shilpid` and `shilpi-http` (they bind fixed
/// ports, so a second starter by O-CoreOp fails harmlessly and the one that is up is used).
#[cfg(not(target_arch = "wasm32"))]
fn ensure_shared_server() -> Result<()> {
    if answers(SHARED_ADDR) {
        return Ok(());
    }
    let shilpid = find_program("shilpid").ok_or_else(|| {
        bad(
            "takeoff.record",
            "no ShilpiDB server answers on 127.0.0.1:7420 and shilpid was not found (set SHILPIDB_BIN to its folder, or pass `server` or `path`)",
        )
    })?;
    let base = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let dir = base.join("OSAS").join("takeoff");
    std::fs::create_dir_all(&dir).map_err(|e| bad("takeoff.record", format!("{}: {e}", dir.display())))?;
    let data = dir.join("takeoff.vdb");
    let spawn = |cmd: &mut std::process::Command| {
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // No console window, and not part of this process's group.
            cmd.creation_flags(0x0800_0000 | 0x0000_0200);
        }
        cmd.spawn()
    };
    spawn(std::process::Command::new(&shilpid).args(["--bind", SHARED_ADDR, "--data"]).arg(&data).args(["--autosave-secs", "5"]))
        .map_err(|e| bad("takeoff.record", format!("could not start shilpid: {e}")))?;
    if let Some(http) = find_program("shilpi-http") {
        // The gateway is for O-CoreOp; a failure to start it does not stop the recording.
        let _ = spawn(std::process::Command::new(http).args(["--bind", GATEWAY_ADDR, "--backend", SHARED_ADDR]));
    }
    for _ in 0..50 {
        if answers(SHARED_ADDR) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err(bad("takeoff.record", "shilpid was started but did not answer on 127.0.0.1:7420 within five seconds"))
}

// ---------------- the command ----------------

fn run_record(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        Err(bad("takeoff.record", "ShilpiDB files cannot be written on the web"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let c = bridge::collect(s, p)?;
        let mut warnings = c.warnings.clone();
        let kept = {
            let v = project::get(s.doc()?, SLOT_KEY);
            let ours = v.get("drawingId").and_then(Value::as_str) == Some(c.drawing_id.as_str());
            if ours { v.get("slot").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()) } else { None }
        };
        let office_id = project::get(s.doc()?, "aorms").get("projectOfficeId").and_then(Value::as_str).map(str::to_string);

        // The annotated drawing, when asked for and when this build can plot.
        let want_pdf = p.get("annotate").and_then(Value::as_bool).unwrap_or(true);
        let pdf = if want_pdf {
            match super::takeoffann::build_pdf(s, &c, str_param(p, "paper").unwrap_or("A3")) {
                Ok((bytes, _)) if bytes.len() > MAX_PDF => {
                    warnings.push(format!("the annotated drawing is {} bytes, over the {MAX_PDF} that are stored: not recorded", bytes.len()));
                    None
                }
                Ok((bytes, _)) => {
                    let pages = super::takeoffann::page_count(&bytes);
                    Some((bytes, pages, bridge::iso_time().unwrap_or_default()))
                }
                Err(e) => {
                    warnings.push(format!("no annotated drawing: {e}"));
                    None
                }
            }
        } else {
            None
        };
        let pdf_ref = pdf.as_ref().map(|(b, n, g)| (b.as_slice(), *n, g.clone()));

        let file_path = match (str_param(p, "path"), p.get("file").and_then(Value::as_bool).unwrap_or(false)) {
            (Some(t), _) => Some(std::path::PathBuf::from(t)),
            (None, true) => match s.state()?.path.as_ref() {
                Some(drawing) => Some(std::path::Path::new(drawing).with_extension("takeoff.vdb")),
                None => return Err(bad("takeoff.record", "save the drawing first, or give `path` or `server`")),
            },
            (None, false) => None,
        };
        let (target, mut sink): (Value, Box<dyn Sink>) = match (&file_path, str_param(p, "server")) {
            (Some(path), _) => (json!({ "path": path.display().to_string() }), Box::new(FileSink::open(path)?)),
            (None, server) => {
                let addr = server.unwrap_or(SHARED_ADDR).to_string();
                if server.is_none() {
                    ensure_shared_server()?;
                }
                let client = aad_vdb::VdbClient::connect_with_timeout(addr.as_str(), Some(std::time::Duration::from_secs(10)))
                    .map_err(|e| bad("takeoff.record", format!("{addr}: {e}")))?;
                (json!({ "server": addr }), Box::new(ServerSink { addr, client }))
            }
        };
        let probe_slot = find_slot(sink.as_mut(), &c.drawing_id, kept)?;
        let built = build(&c, probe_slot)?;
        let out = write_slot(sink.as_mut(), &c, &built, office_id.as_deref(), Some(probe_slot), pdf_ref, file_path.is_some())?;
        project::set(s.doc_mut()?, SLOT_KEY, &json!({ "drawingId": c.drawing_id, "slot": out.slot }));

        let mut result = target;
        result["headerId"] = json!(header_id(out.slot));
        result["slot"] = json!(out.slot);
        result["state"] = json!("complete");
        result["counts"] = json!(built.counts);
        result["written"] = json!(out.written);
        result["added"] = json!(out.added);
        result["deleted"] = json!(out.deleted);
        if let Some(n) = out.changed {
            result["changed"] = json!(n);
        }
        if let Some(n) = out.unchanged {
            result["unchanged"] = json!(n);
        }
        if let Some((bytes, pages, _)) = &pdf {
            result["pdf"] = json!({ "bytes": bytes.len(), "pages": pages, "chunks": bytes.chunks(CHUNK).count() });
        }
        result["skipped"] = json!(built.skipped);
        result["warnings"] = json!(warnings);
        let text = built.counts.iter().map(|(k, n)| format!("{n} {k}")).collect::<Vec<_>>().join(", ");
        s.echo(format!("Take-off recorded: {}.", if text.is_empty() { "no measured rows".to_string() } else { text }));
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_follow_the_contract_test_vectors() {
        assert_eq!(slot_for("0b7d9c34-1e6f-4f0b-8a13-9b2c7d5e41aa"), 6_389_047);
        assert_eq!(slot_for("6f1c1b0e-7a52-4d0a-9c1e-2d8a3f4b5c60"), 15_751_439);
        assert!((1..=0x00FF_FFFF).contains(&slot_for("")));
        assert_eq!(next_slot(0x00FF_FFFF), 1);
        assert_eq!(next_slot(5), 6);
    }

    #[test]
    fn ids_carry_kind_slot_and_number() {
        assert_eq!(header_id(0), 1u64 << 56);
        let id = record_id(3, 0x0ABCDE, 0x1A2);
        assert_eq!((kind_of(id), slot_of(id)), (3, 0x0ABCDE));
        assert_eq!(id & 0xFFFF_FFFF, 0x1A2);
        // A slot wider than 24 bits cannot spill into the kind.
        assert_eq!(kind_of(record_id(3, u32::MAX, 7)), 3);
        // A handle wider than 32 bits is hashed, not truncated.
        assert_eq!(item_number(0x1A2), 0x1A2);
        assert_eq!(item_number(0x1_0000_0001), fnv32(b"100000001"));
    }

    #[test]
    fn a_pdf_is_split_into_chunks_that_add_up() {
        let bytes: Vec<u8> = (0..(CHUNK * 2 + 100)).map(|i| (i % 251) as u8).collect();
        let p = chunk_pdf(&bytes, 7, 1, "2026-10-19T00:00:00Z").unwrap();
        assert_eq!(p.chunks.len(), 3);
        assert!(p.chunks.iter().all(|c| c.payload.len() <= CHUNK && kind_of(c.id) == KIND_PDF && slot_of(c.id) == 7 && c.bbox == [0.0; 4]));
        assert_eq!(p.chunks.iter().flat_map(|c| c.payload.clone()).collect::<Vec<u8>>(), bytes);
        assert_eq!(p.meta["bytes"], json!(bytes.len()));
        assert_eq!(p.meta["chunks"], json!(3));
        assert_eq!(p.meta["sha256"], json!(crate::revision::sha256_hex(&bytes)));
        assert!(chunk_pdf(&vec![0u8; MAX_PDF + 1], 7, 1, "").is_err());
    }

    /// A database in memory, for the slot rules.
    #[derive(Default)]
    struct Mem(std::collections::BTreeMap<u64, Vec<u8>>);
    impl Sink for Mem {
        fn ids(&mut self) -> Result<Vec<u64>> {
            Ok(self.0.keys().copied().collect())
        }
        fn payload(&mut self, id: u64) -> Result<Option<Vec<u8>>> {
            Ok(self.0.get(&id).cloned())
        }
        fn put(&mut self, recs: &[Rec]) -> Result<()> {
            for r in recs {
                self.0.insert(r.id, r.payload.clone());
            }
            Ok(())
        }
        fn delete(&mut self, id: u64) -> Result<bool> {
            Ok(self.0.remove(&id).is_some())
        }
        fn save(&mut self) -> Result<()> {
            Ok(())
        }
    }

    fn header_of(drawing_id: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({ "source": { "drawing_id": drawing_id } })).unwrap()
    }

    #[test]
    fn a_taken_slot_is_probed_past_and_an_own_slot_is_reused() {
        let id = "0b7d9c34-1e6f-4f0b-8a13-9b2c7d5e41aa";
        let mut db = Mem::default();
        assert_eq!(find_slot(&mut db, id, None).unwrap(), 6_389_047, "a free slot is the start slot");
        // Another drawing holds it: the next one is used.
        db.0.insert(header_id(6_389_047), header_of("someone-else"));
        assert_eq!(find_slot(&mut db, id, None).unwrap(), 6_389_048);
        // Our own header is found again, also from the slot kept in the drawing.
        db.0.insert(header_id(6_389_048), header_of(id));
        assert_eq!(find_slot(&mut db, id, None).unwrap(), 6_389_048);
        assert_eq!(find_slot(&mut db, id, Some(6_389_048)).unwrap(), 6_389_048);
        // A kept slot that another drawing has since taken is not trusted.
        db.0.insert(header_id(6_389_048), header_of("third"));
        assert_eq!(find_slot(&mut db, id, Some(6_389_048)).unwrap(), 6_389_049);
        // After the last slot comes 1.
        let last = "x";
        let mut full = Mem::default();
        full.0.insert(header_id(0x00FF_FFFF), header_of("other"));
        assert_eq!(find_slot(&mut full, last, Some(0x00FF_FFFF)).unwrap(), 1);
    }
}
