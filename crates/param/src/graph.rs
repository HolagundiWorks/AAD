//! The persistent graph: nodes, edges and editor metadata. Execution order comes from the edges;
//! positions, groups and labels are editor metadata and are kept apart in [`Editor`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::{Error, MAX_EDGES, MAX_GROUPS, MAX_ID_LEN, MAX_NODES};

/// The graph format this crate writes. Older versions are migrated by [`Graph::from_json`];
/// newer ones are refused.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Graph {
    pub schema_version: u32,
    pub graph_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Incremented by the host on every stored edit.
    #[serde(default)]
    pub graph_revision: u64,
    #[serde(default)]
    pub units_context: UnitsContext,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub editor: Editor,
}

/// How lengths in the drawing relate to the graph's canonical metres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnitsContext {
    /// Metres in one drawing unit (0.001 for a millimetre drawing).
    pub drawing_unit_m: f64,
    /// The unit lengths are shown in.
    pub display_length: String,
}

impl Default for UnitsContext {
    fn default() -> Self {
        UnitsContext { drawing_unit_m: 0.001, display_length: "mm".into() }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeState {
    #[default]
    Active,
    /// Not evaluated; its dependents report a missing input.
    Disabled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Node {
    pub node_id: String,
    pub node_type: String,
    pub node_version: u32,
    #[serde(default)]
    pub config: Json,
    #[serde(default)]
    pub state: NodeState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edge {
    pub edge_id: String,
    pub source_node_id: String,
    pub source_port_id: String,
    pub target_node_id: String,
    pub target_port_id: String,
}

/// Editor metadata: never read by the evaluator.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Editor {
    #[serde(default)]
    pub positions: BTreeMap<String, [f64; 2]>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub groups: Vec<Group>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Group {
    pub group_id: String,
    pub label: String,
    pub nodes: Vec<String>,
}

/// Letters, digits, `-`, `_` and `.`, 1 to [`MAX_ID_LEN`] characters.
pub fn valid_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= MAX_ID_LEN && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

impl Graph {
    pub fn new(graph_id: &str) -> Self {
        Graph {
            schema_version: SCHEMA_VERSION,
            graph_id: graph_id.to_string(),
            project_id: None,
            graph_revision: 0,
            units_context: UnitsContext::default(),
            nodes: Vec::new(),
            edges: Vec::new(),
            editor: Editor::default(),
        }
    }

    /// Read a stored graph. Saved graphs are untrusted: the format version is checked, older
    /// versions are migrated, and the structure is checked against the limits.
    pub fn from_json(v: &Json) -> Result<Graph, Error> {
        let version = v.get("schemaVersion").and_then(Json::as_u64).ok_or_else(|| Error::Format("`schemaVersion` is missing".into()))?;
        let migrated = match version {
            0 => migrate_v0(v)?,
            1 => v.clone(),
            n => return Err(Error::Format(format!("graph format {n} is newer than this program reads ({SCHEMA_VERSION})"))),
        };
        let g: Graph = serde_json::from_value(migrated).map_err(|e| Error::Format(e.to_string()))?;
        g.check_structure()?;
        Ok(g)
    }

    pub fn to_json(&self) -> Json {
        serde_json::to_value(self).unwrap_or(Json::Null)
    }

    /// Identifier shape, uniqueness and size limits. Types and connections are checked by
    /// [`crate::validate`].
    pub fn check_structure(&self) -> Result<(), Error> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(Error::Format(format!("graph format {} is not {SCHEMA_VERSION}", self.schema_version)));
        }
        if !valid_id(&self.graph_id) {
            return Err(Error::Format("`graphId` must be 1-64 letters, digits, `-`, `_` or `.`".into()));
        }
        if self.nodes.len() > MAX_NODES || self.edges.len() > MAX_EDGES || self.editor.groups.len() > MAX_GROUPS {
            return Err(Error::Limit(format!("a graph holds at most {MAX_NODES} nodes, {MAX_EDGES} edges and {MAX_GROUPS} groups")));
        }
        let du = self.units_context.drawing_unit_m;
        if !(du.is_finite() && du > 0.0) {
            return Err(Error::Format("`unitsContext.drawingUnitM` must be positive".into()));
        }
        let mut seen = std::collections::HashSet::new();
        for n in &self.nodes {
            if !valid_id(&n.node_id) {
                return Err(Error::Format(format!("node id `{}` is not a valid id", n.node_id)));
            }
            if !seen.insert(n.node_id.as_str()) {
                return Err(Error::Format(format!("node id `{}` is used twice", n.node_id)));
            }
        }
        let mut edges = std::collections::HashSet::new();
        for e in &self.edges {
            if !valid_id(&e.edge_id) || !edges.insert(e.edge_id.as_str()) {
                return Err(Error::Format(format!("edge id `{}` is not valid or is used twice", e.edge_id)));
            }
        }
        Ok(())
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.node_id == id)
    }
    pub fn node_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.node_id == id)
    }

    /// Remove a node and every edge touching it; returns the removed edge ids.
    pub fn remove_node(&mut self, id: &str) -> Vec<String> {
        self.nodes.retain(|n| n.node_id != id);
        let (gone, kept): (Vec<Edge>, Vec<Edge>) =
            std::mem::take(&mut self.edges).into_iter().partition(|e| e.source_node_id == id || e.target_node_id == id);
        self.edges = kept;
        self.editor.positions.remove(id);
        self.editor.labels.remove(id);
        for g in &mut self.editor.groups {
            g.nodes.retain(|n| n != id);
        }
        gone.into_iter().map(|e| e.edge_id).collect()
    }
}

/// Format 0 (pre-release) kept node positions on the node as `position`; format 1 keeps them in
/// `editor.positions`.
fn migrate_v0(v: &Json) -> Result<Json, Error> {
    let mut out = v.clone();
    let Some(obj) = out.as_object_mut() else { return Err(Error::Format("a graph is a JSON object".into())) };
    obj.insert("schemaVersion".into(), Json::from(1));
    let mut positions = serde_json::Map::new();
    if let Some(nodes) = obj.get_mut("nodes").and_then(Json::as_array_mut) {
        for n in nodes {
            if let Some(no) = n.as_object_mut()
                && let Some(p) = no.remove("position")
                && let Some(id) = no.get("nodeId").and_then(Json::as_str)
            {
                positions.insert(id.to_string(), p);
            }
        }
    }
    let editor = obj.entry("editor").or_insert_with(|| Json::Object(serde_json::Map::new()));
    if let Some(e) = editor.as_object_mut() {
        e.insert("positions".into(), Json::Object(positions));
    }
    Ok(out)
}
