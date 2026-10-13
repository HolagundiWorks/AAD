//! Properties, layers and blocks for the WinUI shell.
//!
//! One JSON document, tagged with the drawing id and revision, so the shell rebuilds the panels
//! only when the drawing changed. Edits go back through engine commands, not through this snapshot.

use aad_color::Color;
use aad_engine::Session;
use aad_engine::doc::{EntityKind, Handle, Lineweight};
use serde_json::{Value, json};

const MAX_LAYERS: usize = 400;
const MAX_BLOCKS: usize = 400;
const MAX_SCAN: usize = 50_000;

/// The Classic panels: current properties, the layer table and block definitions.
pub fn panels_json(session: &Session) -> String {
    let Ok(state) = session.state() else {
        return json!({ "uid": 0, "revision": 0, "selection": 0, "properties": [], "layers": [], "blocks": [] }).to_string();
    };
    let doc = &state.doc;
    let selection = state.selection.len();
    let current = doc.header.str("CLAYER", "0");
    let mut layers_truncated = false;
    let mut layers = Vec::new();
    for layer in doc.layers.iter().take(MAX_LAYERS + 1) {
        if layers.len() == MAX_LAYERS {
            layers_truncated = true;
            break;
        }
        let rgb = layer.color.resolve(Color::Index(7), Color::Index(7));
        layers.push(json!({
            "name": layer.name,
            "on": layer.on,
            "frozen": layer.frozen,
            "locked": layer.locked,
            "color": rgb.hex(),
            "colorName": layer.color.name(),
            "current": layer.name.eq_ignore_ascii_case(&current),
        }));
    }
    let refs = reference_counts(doc);
    let mut blocks_truncated = false;
    let mut blocks: Vec<&aad_engine::doc::Block> = doc.blocks.values().map(|b| b.as_ref()).filter(|b| !b.anonymous).collect();
    blocks.sort_by_key(|block| block.name.to_ascii_lowercase());
    if blocks.len() > MAX_BLOCKS {
        blocks.truncate(MAX_BLOCKS);
        blocks_truncated = true;
    }
    let blocks: Vec<Value> = blocks
        .iter()
        .map(|b| {
            json!({
                "name": b.name,
                "entities": b.entities.len(),
                "references": refs.get(&b.name).copied().unwrap_or(0),
            })
        })
        .collect();
    json!({
        "uid": state.uid,
        "revision": state.revision,
        "selection": selection,
        "properties": properties(doc, &state.selection),
        "layers": layers,
        "layersTruncated": layers_truncated,
        "blocks": blocks,
        "blocksTruncated": blocks_truncated,
    })
    .to_string()
}

fn properties(doc: &aad_engine::doc::Drawing, selection: &[Handle]) -> Vec<Value> {
    let header = &doc.header;
    if selection.is_empty() {
        let color = Color::from_aci(i16::try_from(header.i64("CECOLOR", 256)).unwrap_or(256));
        let weight = Lineweight::from_dxf(i16::try_from(header.i64("CELWEIGHT", -1)).unwrap_or(-1));
        let scale = header.f64("CELTSCALE", 1.0);
        let scale = if scale.is_finite() { format!("{scale:.4}") } else { "0".into() };
        return vec![
            row("Selection", "No selection"),
            row("Color", &color.name()),
            row("Layer", &header.str("CLAYER", "0")),
            row("Linetype", &header.str("CELTYPE", "ByLayer")),
            row("Lineweight", &weight.name()),
            row("Linetype scale", &scale),
        ];
    }
    let count = selection.len();
    let mut kinds: Vec<&str> = Vec::new();
    let mut layer = String::new();
    for handle in selection.iter().copied().take(32) {
        let Some(entity) = doc.entity(handle) else { continue };
        let kind = entity.kind.type_name();
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
        if layer.is_empty() {
            layer = entity.common.layer.clone();
        }
    }
    let kind = match kinds.as_slice() {
        [] => "Unknown",
        [one] => one,
        _ => "Mixed",
    };
    vec![row("Selection", &format!("{count} selected")), row("Type", kind), row("Layer", &layer)]
}

fn row(label: &str, value: &str) -> Value {
    json!({ "label": label, "value": value })
}

fn reference_counts(doc: &aad_engine::doc::Drawing) -> std::collections::BTreeMap<String, u32> {
    let mut counts = std::collections::BTreeMap::new();
    let entities = std::iter::once(&doc.model).chain(doc.layouts.iter().map(|layout| &layout.entities)).flat_map(|space| space.iter());
    for (seen, entity) in entities.enumerate() {
        if seen == MAX_SCAN {
            break;
        }
        if let EntityKind::Insert(insert) = &entity.kind {
            let count = counts.entry(insert.block.clone()).or_insert(0u32);
            *count = count.saturating_add(1);
        }
    }
    counts
}
