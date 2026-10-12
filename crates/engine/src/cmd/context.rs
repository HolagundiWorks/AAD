//! Live, read-only context for the assistant: what a `/reference` typed in the command bar means.
//!
//! `/drawing`, `/selection`, `/layers`, `/rooms`, `/floor`, `/schedule` and `/project` are resolved
//! here from the open document and its project data, never from fixed text. Resolution only reads:
//! it takes `&Session`, changes nothing, never switches the drawing or the floor, and sees only the
//! active drawing, its active floor and its project data. A provider that has nothing to read says
//! so with a reason (`Unavailable`); a provider that read a list and found it empty returns an empty
//! list: missing data and empty data are different answers. Every result is bounded in size.

use serde_json::{Map, Value, json};

use crate::Session;
use aad_doc::{Drawing, EntityKind, Space};

/// A reference the command bar offers.
#[derive(Clone, Copy, Debug)]
pub struct Provider {
    pub name: &'static str,
    pub summary: &'static str,
}

/// Only providers that return real data are listed.
pub const PROVIDERS: &[Provider] = &[
    Provider { name: "drawing", summary: "The open drawing: units, space, object counts, extents, layouts" },
    Provider { name: "selection", summary: "The selected objects and their properties" },
    Provider { name: "layers", summary: "Layers with visibility, lock state and colour; the active drawing space" },
    Provider { name: "rooms", summary: "Room labels made by the ROOM command: name, size, area" },
    Provider { name: "floor", summary: "The current floor and the floors of this drawing" },
    Provider { name: "schedule", summary: "The door and window schedule" },
    Provider { name: "project", summary: "Project details of this drawing" },
];

/// Why a reference gave nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefError {
    /// Not one of [`PROVIDERS`].
    Unknown(String),
    /// A known reference with nothing to read in this drawing.
    Unavailable { name: String, reason: String },
}

impl std::fmt::Display for RefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefError::Unknown(n) => {
                let all: Vec<String> = PROVIDERS.iter().map(|p| format!("/{}", p.name)).collect();
                write!(f, "/{n} is not a reference (try {})", all.join(", "))
            }
            RefError::Unavailable { name, reason } => write!(f, "/{name}: {reason}"),
        }
    }
}

/// Most list items one reference returns.
pub const MAX_ITEMS: usize = 40;
/// Longest serialized reference, in characters; more is cut and marked `truncated`.
pub const MAX_CHARS: usize = 3000;

/// The references in `text`: words that start with `/` (at the start or after a space) followed by
/// letters. A `/` inside a word (a path, a fraction) is not a reference. In order, without repeats.
pub fn references_in(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in text.split_whitespace() {
        if let Some(rest) = w.strip_prefix('/') {
            let name: String = rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_ascii_lowercase();
            if !name.is_empty() && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

fn unavailable(name: &str, reason: &str) -> RefError {
    RefError::Unavailable { name: name.into(), reason: reason.into() }
}

fn bounded(mut v: Value) -> Value {
    if v.to_string().chars().count() <= MAX_CHARS {
        return v;
    }
    // Drop list items from the back until it fits, and say so.
    if let Some(o) = v.as_object_mut() {
        let keys: Vec<String> = o.iter().filter(|(_, x)| x.is_array()).map(|(k, _)| k.clone()).collect();
        for k in keys {
            while Value::Object(o.clone()).to_string().chars().count() > MAX_CHARS {
                match o.get_mut(&k).and_then(Value::as_array_mut) {
                    Some(a) if a.len() > 1 => {
                        a.pop();
                    }
                    _ => break,
                }
            }
        }
        o.insert("truncated".into(), json!(true));
    }
    v
}

/// Resolve one reference (`name` without the slash, any case).
pub fn resolve(s: &Session, name: &str) -> Result<Value, RefError> {
    let n = name.to_ascii_lowercase();
    let d = s.doc().map_err(|_| unavailable(&n, "no drawing is open"))?;
    let v = match n.as_str() {
        "drawing" => drawing(s, d),
        "selection" => selection(s, d),
        "layers" => layers(d),
        "rooms" => rooms(d)?,
        "floor" => floor(d)?,
        "schedule" => schedule(d)?,
        "project" => project(d)?,
        _ => return Err(RefError::Unknown(n)),
    };
    Ok(bounded(v))
}

/// What a request's references resolved to.
#[derive(Clone, Debug, Default)]
pub struct Resolved {
    /// `name -> data` for every reference that resolved.
    pub context: Map<String, Value>,
    /// The references that resolved, in order.
    pub used: Vec<String>,
    /// One message per reference that did not.
    pub problems: Vec<String>,
}

/// Resolve every reference in `text`.
pub fn resolve_text(s: &Session, text: &str) -> Resolved {
    let mut r = Resolved::default();
    for name in references_in(text) {
        match resolve(s, &name) {
            Ok(v) => {
                r.used.push(name.clone());
                r.context.insert(name, v);
            }
            Err(e) => r.problems.push(e.to_string()),
        }
    }
    r
}

fn drawing(s: &Session, d: &Drawing) -> Value {
    let space = s.space();
    let ext = d.extents(&space);
    let st = s.state().ok();
    let mut counts: std::collections::BTreeMap<&'static str, usize> = std::collections::BTreeMap::new();
    let (space_name, total) = match d.space(&space) {
        Some(store) => {
            for e in store.iter() {
                *counts.entry(e.kind.type_name()).or_default() += 1;
            }
            (
                match &space {
                    Space::Model => "Model".to_string(),
                    Space::Paper(n) => n.clone(),
                },
                store.len(),
            )
        }
        None => ("Model".to_string(), 0),
    };
    json!({
        "title": st.map(|x| x.title.clone()),
        "path": st.and_then(|x| x.path.clone()),
        "unit": d.header.i64("INSUNITS", 0),
        "space": space_name,
        "objectsInSpace": total,
        "objectsByType": counts,
        "extents": if ext.is_empty() { Value::Null } else { json!({"min": [ext.min.x, ext.min.y], "max": [ext.max.x, ext.max.y]}) },
        "layerCount": d.layers.len(),
        "currentLayer": d.header.str("CLAYER", "0"),
        "layouts": d.layouts.iter().map(|l| l.name.clone()).collect::<Vec<_>>(),
        "blocks": d.blocks.len(),
    })
}

fn selection(s: &Session, d: &Drawing) -> Value {
    let sel = s.selection();
    let items: Vec<Value> = sel
        .iter()
        .take(MAX_ITEMS)
        .filter_map(|h| d.entity(*h))
        .map(|e| {
            let b = aad_doc::entity_bounds(d, e, 0);
            json!({
                "handle": e.handle.hex(), "type": e.kind.type_name(), "layer": e.common.layer,
                "bounds": if b.is_empty() { Value::Null } else { json!({"min": [b.min.x, b.min.y], "max": [b.max.x, b.max.y]}) },
                "text": match &e.kind { EntityKind::Text(t) => Some(t.value.clone()), _ => None },
            })
        })
        .collect();
    json!({"count": sel.len(), "shown": items.len(), "objects": items})
}

fn layers(d: &Drawing) -> Value {
    let items: Vec<Value> = d
        .layers
        .iter()
        .take(MAX_ITEMS * 3)
        .map(|l| json!({"name": l.name, "visible": l.on, "frozen": l.frozen, "locked": l.locked, "color": l.color.name()}))
        .collect();
    json!({"count": d.layers.len(), "current": d.header.str("CLAYER", "0"), "layers": items, "drawingSpace": super::space::active_summary(d)})
}

fn rooms(d: &Drawing) -> Result<Value, RefError> {
    const LABELS: &str = "ROOM-LABELS";
    if d.layer(LABELS).is_none() {
        return Err(unavailable("rooms", "no room labels in this drawing (the ROOM command creates them on layer ROOM-LABELS)"));
    }
    // The ROOM command writes each label as three consecutive texts: name, size, "Area: ...".
    let texts: Vec<&aad_doc::Text> = d
        .model
        .iter()
        .filter(|e| e.common.layer.eq_ignore_ascii_case(LABELS))
        .filter_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t) } else { None })
        .collect();
    let mut rooms = Vec::new();
    for c in texts.chunks(3) {
        match c {
            [name, size, area] if area.value.starts_with("Area:") => {
                rooms.push(json!({"name": name.value, "size": size.value, "area": area.value.trim_start_matches("Area:").trim()}));
            }
            other => rooms.push(json!({"name": other.first().map(|t| t.value.clone()), "note": "label not in the usual three-line form"})),
        }
    }
    let total = rooms.len();
    rooms.truncate(MAX_ITEMS);
    Ok(json!({"count": total, "shown": rooms.len(), "rooms": rooms, "source": "room labels (layer ROOM-LABELS), not geometric room detection"}))
}

fn floor(d: &Drawing) -> Result<Value, RefError> {
    let Value::Array(list) = super::project::get(d, "floors") else {
        return Err(unavailable("floor", "no floors are defined for this drawing (Architecture > Project > Floors)"));
    };
    let current = super::project::active_floor(d).map(|(_, n)| n);
    let floors: Vec<Value> = list.iter().take(MAX_ITEMS).cloned().collect();
    Ok(json!({"current": current, "floors": floors}))
}

fn schedule(d: &Drawing) -> Result<Value, RefError> {
    let Value::Object(o) = super::project::get(d, "schedule") else {
        return Err(unavailable("schedule", "this drawing has no door and window schedule data"));
    };
    let all = o.get("openings").and_then(Value::as_array).cloned().unwrap_or_default();
    let total = all.len();
    let items: Vec<Value> = all.into_iter().take(MAX_ITEMS).collect();
    Ok(json!({"count": total, "shown": items.len(), "openings": items}))
}

fn project(d: &Drawing) -> Result<Value, RefError> {
    let Value::Object(details) = super::project::get(d, "details") else {
        return Err(unavailable("project", "no project details are set for this drawing (Architecture > Project > Project Details)"));
    };
    let floors = match super::project::get(d, "floors") {
        Value::Array(a) => json!(a.len()),
        _ => Value::Null,
    };
    Ok(json!({"details": details, "floorCount": floors}))
}
