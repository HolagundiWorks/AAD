//! Ready-made graphs.

use serde_json::{Value as Json, json};

use crate::graph::{Edge, Graph, Node, NodeState};
use crate::nodes::{MEASURE_WALL, NUMBER, PATH, WALL};
use crate::schema::WALL_TYPE;

/// The node ids [`add_wall`] made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WallNodes {
    pub path: String,
    pub height: String,
    pub thickness: String,
    pub wall: String,
    pub measure: String,
}

fn node(id: String, node_type: &str, config: Json) -> Node {
    Node { node_id: id, node_type: node_type.into(), node_version: 1, config, state: NodeState::Active }
}

fn edge(id: String, s: &str, sp: &str, t: &str, tp: &str) -> Edge {
    Edge { edge_id: id, source_node_id: s.into(), source_port_id: sp.into(), target_node_id: t.into(), target_port_id: tp.into() }
}

/// Add a wall to `g`: a path, a height slider, a thickness box, the wall component and its
/// measurements, connected. `prefix` makes the ids (`<prefix>.path`, `<prefix>.wall`, …) and is
/// the wall's instance id. `points` are in metres; `source` names the drawing entity the path
/// follows, when it follows one.
pub fn add_wall(g: &mut Graph, prefix: &str, points: &[[f64; 2]], closed: bool, height_m: f64, thickness_mm: f64, source: Option<Json>) -> WallNodes {
    let id = |s: &str| format!("{prefix}.{s}");
    let ids = WallNodes { path: id("path"), height: id("height"), thickness: id("thickness"), wall: id("wall"), measure: id("measure") };
    let mut path_cfg = json!({ "points": points, "closed": closed });
    if let Some(src) = source {
        path_cfg["source"] = src;
    }
    g.nodes.push(node(ids.path.clone(), PATH, path_cfg));
    g.nodes.push(node(
        ids.height.clone(),
        NUMBER,
        json!({ "value": height_m, "unit": "m", "min": 2.4, "max": 6.0, "step": 0.05, "control": "slider" }),
    ));
    g.nodes.push(node(
        ids.thickness.clone(),
        NUMBER,
        json!({ "value": thickness_mm, "unit": "mm", "min": 75, "max": 600, "step": 5, "control": "number" }),
    ));
    g.nodes.push(node(
        ids.wall.clone(),
        WALL,
        json!({ "componentType": WALL_TYPE, "schemaVersion": 1, "instanceId": prefix, "overrides": {}, "locked": [] }),
    ));
    g.nodes.push(node(ids.measure.clone(), MEASURE_WALL, json!({})));
    g.edges.push(edge(id("e.path"), &ids.path, "path", &ids.wall, "path"));
    g.edges.push(edge(id("e.height"), &ids.height, "value", &ids.wall, "height"));
    g.edges.push(edge(id("e.thickness"), &ids.thickness, "value", &ids.wall, "thickness"));
    g.edges.push(edge(id("e.measure"), &ids.wall, "wall", &ids.measure, "wall"));
    for (i, n) in [&ids.path, &ids.height, &ids.thickness, &ids.wall, &ids.measure].iter().enumerate() {
        let (x, y) = match i {
            0..=2 => (0.0, i as f64 * 120.0),
            3 => (280.0, 120.0),
            _ => (540.0, 120.0),
        };
        g.editor.positions.insert((*n).clone(), [x, y]);
    }
    ids
}
