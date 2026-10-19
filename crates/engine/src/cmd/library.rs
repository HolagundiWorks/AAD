//! `library.publish` and `library.list`: the component library in its own ShilpiDB database
//! (`docs/component-library-proposal.md`, option B).
//!
//! The library is separate from the take-off database: it is shared by every drawing, so it has no drawing slot.
//! Ids follow the same layout, `(kind << 56) | (hi << 32) | revision`, where `hi` is a 24-bit hash of the
//! definition's id (probed past a hash another definition holds) and `revision` is the definition's version. One
//! definition revision is one immutable record: publishing the same bytes again changes nothing, publishing
//! different bytes under the same id and revision is a conflict and is not written, and a change is a new revision.
//!
//! Kinds: 1 header, 2 component type, 3 material, 4 finish, 5 preset, 6 graph template. The payload is the
//! definition object (UTF-8 JSON) with `format` (`osas-component-type`, `osas-material`, ...) and `version` (the
//! payload format, 1). A definition carries specifications only: no rates, costs or waste factors.
//!
//! The default target is `127.0.0.1:7422` (gateway 7423), started when nothing answers; `server` names another and
//! `path` uses a `.vdb` file.

use serde_json::{Value, json};

use super::takeoffrec::{Rec, Sink, header_id, kind_of, record_id, slot_for};
use super::*;
use crate::{Result, Session};

const FORMAT: &str = "osas-library";
const KIND_COMPONENT_TYPE: u8 = 2;
const KIND_MATERIAL: u8 = 3;
const KIND_FINISH: u8 = 4;
const KIND_PRESET: u8 = 5;
const KIND_TEMPLATE: u8 = 6;
const KINDS: [(&str, u8); 5] = [
    ("component_type", KIND_COMPONENT_TYPE),
    ("material", KIND_MATERIAL),
    ("finish", KIND_FINISH),
    ("preset", KIND_PRESET),
    ("template", KIND_TEMPLATE),
];
/// The most definitions one call publishes, and the largest payload of one.
const MAX_DEFS: usize = 10_000;
const MAX_PAYLOAD: usize = 1024 * 1024;
#[cfg(not(target_arch = "wasm32"))]
const ADDR: &str = "127.0.0.1:7422";
#[cfg(not(target_arch = "wasm32"))]
const GATEWAY: &str = "127.0.0.1:7423";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("library.publish", "Publish to the Component Library", run_publish)
            .menu(&["Architecture", "Parametric", "Publish to Library"])
            .params("{definitions: [{kind: \"component_type\"|\"material\"|\"finish\"|\"preset\"|\"template\", id, revision (component types: schema_version), ...}], server?: \"host:port\" (default: the shared library server on 127.0.0.1:7422, started if nothing answers), path?: .vdb file} → {target, published, unchanged, conflicts, counts}")
            .noundo(),
        CommandSpec::new("library.list", "List the Component Library", run_list)
            .menu(&["Architecture", "Parametric", "Component Library"])
            .params("{kind?: one of the kinds above, server?, path?} → {target, state, definitions: [{kind, id, revision, label, deprecated}], counts}")
            .noundo(),
    ]
}

fn kind_code(name: &str) -> Option<u8> {
    KINDS.iter().find(|k| k.0 == name).map(|k| k.1)
}

fn kind_name(code: u8) -> &'static str {
    KINDS.iter().find(|k| k.1 == code).map_or("other", |k| k.0)
}

fn format_of(kind: &str) -> String {
    format!("osas-{}", kind.replace('_', "-"))
}

/// A validated definition, ready to place.
struct Def {
    kind: u8,
    id: String,
    revision: u32,
    payload: Value,
}

fn valid_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Check one definition of the call and give it its `format` and `version`.
fn parse_def(v: &Value) -> std::result::Result<Def, String> {
    let obj = v.as_object().ok_or("a definition must be an object")?;
    let kind_text = obj.get("kind").and_then(Value::as_str).ok_or("a definition needs a `kind`")?;
    let kind = kind_code(kind_text).ok_or_else(|| format!("unknown kind `{}`", kind_text.chars().take(40).collect::<String>()))?;
    let id = obj
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| valid_id(s))
        .ok_or("a definition needs an `id` of letters, digits, `-`, `_` and `.` (at most 64)")?;
    let rev_key = if kind == KIND_COMPONENT_TYPE { "schema_version" } else { "revision" };
    let revision = obj
        .get(rev_key)
        .or_else(|| obj.get("revision"))
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n >= 1)
        .ok_or_else(|| format!("`{id}` needs `{rev_key}`, a whole number of at least 1"))?;
    let mut payload = obj.clone();
    payload.remove("kind");
    match payload.get("format").and_then(Value::as_str) {
        Some(f) if f != format_of(kind_text) => return Err(format!("`{id}`: format `{f}` does not match kind `{kind_text}`")),
        _ => {
            payload.insert("format".into(), json!(format_of(kind_text)));
        }
    }
    payload.entry("version").or_insert(json!(1));
    if payload.get("version") != Some(&json!(1)) {
        return Err(format!("`{id}`: this version of O-Draft writes payload version 1 only"));
    }
    let payload = Value::Object(payload);
    if serde_json::to_vec(&payload).map_or(true, |b| b.len() > MAX_PAYLOAD) {
        return Err(format!("`{id}`: larger than {MAX_PAYLOAD} bytes"));
    }
    Ok(Def { kind, id: id.to_string(), revision, payload })
}

fn next_hi(hi: u32) -> u32 {
    if hi >= 0x00FF_FFFF { 1 } else { hi + 1 }
}

fn id_in(payload: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(payload).ok()?.get("id")?.as_str().map(str::to_string)
}

/// Where a definition goes: the record id for its `(id, revision)`, probing past a hash another definition holds.
fn place(sink: &mut dyn Sink, d: &Def) -> Result<u64> {
    let mut hi = slot_for(&d.id);
    for _ in 0..=0x00FF_FFFFu32 {
        let rid = record_id(d.kind, hi, d.revision);
        match sink.payload(rid)? {
            None => return Ok(rid),
            Some(p) if id_in(&p).as_deref() == Some(d.id.as_str()) => return Ok(rid),
            Some(_) => hi = next_hi(hi),
        }
    }
    Err(bad("library.publish", "every slot for this kind is taken"))
}

fn header_payload(complete: bool, counts: &std::collections::BTreeMap<&'static str, usize>) -> Value {
    json!({ "format": FORMAT, "version": 1, "state": if complete { "complete" } else { "recording" }, "counts": counts })
}

fn counts_of(ids: &[u64]) -> std::collections::BTreeMap<&'static str, usize> {
    let mut m = std::collections::BTreeMap::new();
    for id in ids {
        let k = kind_of(*id);
        if k != 1 {
            *m.entry(kind_name(k)).or_insert(0usize) += 1;
        }
    }
    m
}

/// What a publication did.
#[derive(Default)]
struct Outcome {
    published: Vec<String>,
    unchanged: Vec<String>,
    conflicts: Vec<Value>,
}

/// Write definitions to a database: header `recording`, each definition (immutable), header `complete`, save.
fn publish(sink: &mut dyn Sink, defs: &[Def]) -> Result<(Outcome, std::collections::BTreeMap<&'static str, usize>)> {
    let none = std::collections::BTreeMap::new();
    let put_header = |sink: &mut dyn Sink, complete: bool, counts: &std::collections::BTreeMap<&'static str, usize>| -> Result<()> {
        let payload = serde_json::to_vec(&header_payload(complete, counts)).map_err(|e| bad("library.publish", e.to_string()))?;
        sink.put(&[Rec { id: header_id(0), bbox: [0.0; 4], payload }])
    };
    put_header(sink, false, &none)?;
    let mut out = Outcome::default();
    for d in defs {
        let rid = place(sink, d)?;
        let label = format!("{}@{}", d.id, d.revision);
        match sink.payload(rid)? {
            Some(old) => {
                if serde_json::from_slice::<Value>(&old).ok().as_ref() == Some(&d.payload) {
                    out.unchanged.push(label);
                } else {
                    out.conflicts.push(json!({ "id": d.id, "revision": d.revision, "reason": "a different definition is already published under this id and revision; publish it as a new revision" }));
                }
            }
            None => {
                let payload = serde_json::to_vec(&d.payload).map_err(|e| bad("library.publish", e.to_string()))?;
                sink.put(&[Rec { id: rid, bbox: [0.0; 4], payload }])?;
                out.published.push(label);
            }
        }
    }
    let counts = counts_of(&sink.ids()?);
    put_header(sink, true, &counts)?;
    sink.save()?;
    Ok((out, counts))
}

/// Open the database a call names: a file, a named server, or the shared library server.
#[cfg(not(target_arch = "wasm32"))]
fn open(p: &Value, cmd: &str) -> Result<(Value, Box<dyn Sink>)> {
    use super::takeoffrec::{FileSink, ServerSink, ensure_server};
    match (str_param(p, "path"), str_param(p, "server")) {
        (Some(path), _) => {
            let path = std::path::PathBuf::from(path);
            Ok((json!({ "path": path.display().to_string() }), Box::new(FileSink::open_as(&path, FORMAT, "component library", cmd)?)))
        }
        (None, server) => {
            let addr = server.unwrap_or(ADDR).to_string();
            if server.is_none() {
                ensure_server(cmd, ADDR, GATEWAY, "library", "library.vdb")?;
            }
            let sink = ServerSink::connect(&addr, cmd)?;
            Ok((json!({ "server": addr }), Box::new(sink)))
        }
    }
}

fn run_publish(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        Err(bad("library.publish", "ShilpiDB cannot be written on the web"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let list = p.get("definitions").and_then(Value::as_array).ok_or_else(|| bad("library.publish", "`definitions` must be a list"))?;
        if list.len() > MAX_DEFS {
            return Err(bad("library.publish", format!("at most {MAX_DEFS} definitions in one call")));
        }
        let mut defs = Vec::new();
        for (i, v) in list.iter().enumerate() {
            defs.push(parse_def(v).map_err(|e| bad("library.publish", format!("definition {}: {e}", i + 1)))?);
        }
        let (mut target, mut sink) = open(p, "library.publish")?;
        let (out, counts) = publish(sink.as_mut(), &defs)?;
        target["published"] = json!(out.published);
        target["unchanged"] = json!(out.unchanged);
        target["conflicts"] = json!(out.conflicts);
        target["counts"] = json!(counts);
        s.echo(format!("Library: {} published, {} unchanged, {} in conflict.", out.published.len(), out.unchanged.len(), out.conflicts.len()));
        Ok(target)
    }
}

fn run_list(_s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = p;
        Err(bad("library.list", "ShilpiDB cannot be read on the web"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let only = match str_param(p, "kind") {
            Some(k) => Some(kind_code(k).ok_or_else(|| bad("library.list", format!("unknown kind `{}`", k.chars().take(40).collect::<String>())))?),
            None => None,
        };
        let (mut target, mut sink) = open(p, "library.list")?;
        let mut ids = sink.ids()?;
        ids.sort_unstable();
        let mut state = Value::Null;
        let mut definitions = Vec::new();
        for id in &ids {
            let k = kind_of(*id);
            let Some(bytes) = sink.payload(*id)? else { continue };
            let Ok(v) = serde_json::from_slice::<Value>(&bytes) else { continue };
            if k == 1 {
                state = v.get("state").cloned().unwrap_or(Value::Null);
            } else if (2..=6).contains(&k) && only.is_none_or(|o| o == k) {
                definitions.push(json!({
                    "kind": kind_name(k), "id": v.get("id"), "revision": id & 0xFFFF_FFFF,
                    "label": v.get("label"), "deprecated": v.get("deprecated").and_then(Value::as_bool).unwrap_or(false),
                }));
            }
        }
        target["state"] = state;
        target["counts"] = json!(counts_of(&ids));
        target["definitions"] = json!(definitions);
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn def(kind: &str, id: &str, revision: u64) -> Def {
        parse_def(&json!({ "kind": kind, "id": id, "revision": revision, "label": id })).unwrap()
    }

    #[test]
    fn definitions_are_checked_before_anything_is_written() {
        assert!(parse_def(&json!({ "kind": "wall", "id": "a", "revision": 1 })).is_err(), "unknown kind");
        assert!(parse_def(&json!({ "kind": "material", "id": "bad id", "revision": 1 })).is_err(), "id shape");
        assert!(parse_def(&json!({ "kind": "material", "id": "a", "revision": 0 })).is_err(), "revision from 1");
        assert!(parse_def(&json!({ "kind": "material", "id": "a" })).is_err(), "revision needed");
        assert!(parse_def(&json!({ "kind": "material", "id": "a", "revision": 1, "format": "osas-finish" })).is_err(), "format must match");
        assert!(parse_def(&json!({ "kind": "material", "id": "a", "revision": 1, "version": 2 })).is_err(), "payload version 1 only");
        assert!(parse_def(&json!(5)).is_err());
        // A component type takes its revision from `schema_version`.
        let d = parse_def(&json!({ "kind": "component_type", "id": "osas.wall", "schema_version": 3 })).unwrap();
        assert_eq!((d.kind, d.revision), (KIND_COMPONENT_TYPE, 3));
        assert_eq!(d.payload["format"], "osas-component-type");
    }

    #[test]
    fn a_published_definition_is_immutable_and_a_new_revision_is_a_new_record() {
        let mut db = Mem::default();
        let (a, _) = publish(&mut db, &[def("material", "brick.full", 1)]).unwrap();
        assert_eq!((a.published.len(), a.unchanged.len(), a.conflicts.len()), (1, 0, 0));
        let (b, _) = publish(&mut db, &[def("material", "brick.full", 1)]).unwrap();
        assert_eq!((b.published.len(), b.unchanged.len()), (0, 1), "the same bytes change nothing");
        // Different content under the same id and revision is refused and the original stays.
        let changed = parse_def(&json!({ "kind": "material", "id": "brick.full", "revision": 1, "label": "other" })).unwrap();
        let (c, _) = publish(&mut db, &[changed]).unwrap();
        assert_eq!((c.published.len(), c.conflicts.len()), (0, 1));
        let stored: Value = serde_json::from_slice(db.0.get(&place_id(&db, "material", "brick.full", 1)).unwrap()).unwrap();
        assert_eq!(stored["label"], "brick.full");
        // Revision 2 sits beside revision 1.
        let (d, counts) = publish(&mut db, &[def("material", "brick.full", 2)]).unwrap();
        assert_eq!(d.published.len(), 1);
        assert_eq!(counts["material"], 2, "two revisions are two records");
    }

    fn place_id(db: &Mem, kind: &str, id: &str, rev: u32) -> u64 {
        let _ = db;
        record_id(kind_code(kind).unwrap(), slot_for(id), rev)
    }

    #[test]
    fn a_hash_another_definition_holds_is_probed_past() {
        let mut db = Mem::default();
        let d = def("finish", "plaster.internal", 1);
        // Another definition sits where this one would go.
        let squat = record_id(KIND_FINISH, slot_for("plaster.internal"), 1);
        db.0.insert(squat, serde_json::to_vec(&json!({ "id": "someone.else" })).unwrap());
        let rid = place(&mut db, &d).unwrap();
        assert_ne!(rid, squat);
        assert_eq!(rid, record_id(KIND_FINISH, slot_for("plaster.internal") + 1, 1));
        // Its own earlier record is found again.
        db.0.insert(rid, serde_json::to_vec(&d.payload).unwrap());
        assert_eq!(place(&mut db, &d).unwrap(), rid);
    }
}
