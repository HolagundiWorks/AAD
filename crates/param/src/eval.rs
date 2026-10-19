//! Validation and evaluation: types and connections are checked, cycles are refused with the
//! nodes around them, nodes run in dependency order, and a node whose inputs and configuration
//! are unchanged reuses its cached outputs.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::diag::{Code, Diagnostic};
use crate::graph::{Graph, NodeState};
use crate::nodes::{self, Inputs, NodeSpec, Source};
use crate::types::{DataType, Value, WallValue};
use crate::units::{self, Dim};
use crate::{Context, canonical, fnv64};

/// A checked graph, ready to run.
#[derive(Clone, Debug)]
pub struct Prepared {
    /// Node indices in dependency order.
    pub order: Vec<usize>,
    pub specs: Vec<Option<NodeSpec>>,
    /// For each node, its connected inputs: port → (source node index, source port).
    pub incoming: Vec<BTreeMap<String, (usize, String)>>,
    /// Nodes that failed validation.
    pub failed: BTreeSet<usize>,
}

/// Check `g`. `None` when the graph cannot run at all (a cycle or a structural error); otherwise
/// the nodes that failed are listed in [`Prepared::failed`] and their dependents are blocked.
pub fn validate(g: &Graph) -> (Option<Prepared>, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    if let Err(e) = g.check_structure() {
        diags.push(Diagnostic::error(Code::Limit, None, None, e.to_string()));
        return (None, diags);
    }
    let index: HashMap<&str, usize> = g.nodes.iter().enumerate().map(|(i, n)| (n.node_id.as_str(), i)).collect();
    let mut failed = BTreeSet::new();
    let specs: Vec<Option<NodeSpec>> = g
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| match nodes::spec(n) {
            Ok(s) => Some(s),
            Err(d) => {
                diags.push(d);
                failed.insert(i);
                None
            }
        })
        .collect();
    let mut incoming: Vec<BTreeMap<String, (usize, String)>> = vec![BTreeMap::new(); g.nodes.len()];
    let mut deps: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); g.nodes.len()];
    for e in &g.edges {
        let (Some(&s), Some(&t)) = (index.get(e.source_node_id.as_str()), index.get(e.target_node_id.as_str())) else {
            diags.push(Diagnostic::error(
                Code::InvalidConnection,
                Some(&e.target_node_id),
                Some(&e.target_port_id),
                format!("edge `{}` joins a node that does not exist", e.edge_id),
            ));
            continue;
        };
        deps[t].insert(s);
        let (Some(ss), Some(ts)) = (&specs[s], &specs[t]) else { continue };
        let Some(out) = ss.output(&e.source_port_id) else {
            diags.push(Diagnostic::error(
                Code::InvalidConnection,
                Some(&e.source_node_id),
                Some(&e.source_port_id),
                format!("`{}` has no output `{}`", e.source_node_id, e.source_port_id),
            ));
            failed.insert(t);
            continue;
        };
        let Some(inp) = ts.input(&e.target_port_id) else {
            diags.push(Diagnostic::error(
                Code::InvalidConnection,
                Some(&e.target_node_id),
                Some(&e.target_port_id),
                format!("`{}` has no input `{}`", e.target_node_id, e.target_port_id),
            ));
            failed.insert(t);
            continue;
        };
        if incoming[t].contains_key(&e.target_port_id) {
            diags.push(Diagnostic::error(
                Code::InvalidConnection,
                Some(&e.target_node_id),
                Some(&e.target_port_id),
                format!("input `{}` already has a connection", e.target_port_id),
            ));
            failed.insert(t);
            continue;
        }
        if out.data_type != inp.data_type {
            let code = match (&out.data_type, &inp.data_type) {
                (DataType::Number { .. }, DataType::Number { .. }) => Code::UnitMismatch,
                _ => Code::InvalidConnection,
            };
            diags.push(Diagnostic::error(
                code,
                Some(&e.target_node_id),
                Some(&e.target_port_id),
                format!("`{}` gives a {} but `{}` takes a {}", e.source_port_id, out.data_type.name(), e.target_port_id, inp.data_type.name()),
            ));
            failed.insert(t);
            continue;
        }
        incoming[t].insert(e.target_port_id.clone(), (s, e.source_port_id.clone()));
    }
    for (i, spec) in specs.iter().enumerate() {
        let Some(spec) = spec else { continue };
        for p in spec.inputs.iter().filter(|p| p.required) {
            if !incoming[i].contains_key(&p.id) {
                diags.push(Diagnostic::error(
                    Code::MissingInput,
                    Some(&g.nodes[i].node_id),
                    Some(&p.id),
                    format!("required input `{}` is not connected", p.id),
                ));
                failed.insert(i);
            }
        }
    }
    match topo_order(&deps) {
        Ok(order) => (Some(Prepared { order, specs, incoming, failed }), diags),
        Err(cycle) => {
            let path: Vec<String> = cycle.iter().map(|&i| g.nodes[i].node_id.clone()).collect();
            let mut d =
                Diagnostic::error(Code::Cycle, path.first().map(String::as_str), None, format!("the graph has a cycle: {}", path.join(" → ")));
            d.path = path;
            diags.push(d);
            (None, diags)
        }
    }
}

/// Kahn's order (ties by index, so it is deterministic), or the nodes around a cycle.
fn topo_order(deps: &[BTreeSet<usize>]) -> Result<Vec<usize>, Vec<usize>> {
    let n = deps.len();
    let mut users: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut pending: Vec<usize> = deps.iter().map(BTreeSet::len).collect();
    for (t, ds) in deps.iter().enumerate() {
        for &s in ds {
            users[s].push(t);
        }
    }
    let mut ready: BTreeSet<usize> = (0..n).filter(|&i| pending[i] == 0).collect();
    let mut order = Vec::with_capacity(n);
    while let Some(i) = ready.pop_first() {
        order.push(i);
        for &u in &users[i] {
            pending[u] = pending[u].saturating_sub(1);
            if pending[u] == 0 {
                ready.insert(u);
            }
        }
    }
    if order.len() == n {
        return Ok(order);
    }
    // Walk back along dependencies from a node left over until one repeats.
    let left: BTreeSet<usize> = (0..n).filter(|i| pending[*i] > 0).collect();
    let Some(&start) = left.first() else { return Err(vec![]) };
    let mut seen: Vec<usize> = vec![start];
    let mut cur = start;
    for _ in 0..=n {
        let Some(&next) = deps[cur].iter().find(|d| left.contains(d)) else { break };
        if let Some(pos) = seen.iter().position(|&x| x == next) {
            let mut cycle: Vec<usize> = seen[pos..].to_vec();
            cycle.reverse();
            if let Some(&first) = cycle.first() {
                cycle.push(first);
            }
            return Err(cycle);
        }
        seen.push(next);
        cur = next;
    }
    Err(left.into_iter().collect())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeStatus {
    Ok,
    Failed,
    /// An upstream node failed or is disabled: no current value.
    Blocked,
    Disabled,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    pub value: Value,
    pub source: Source,
    /// The value in display units, such as `3.2 m` or `14.72 m2`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub display: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeResult {
    pub status: NodeStatus,
    pub outputs: BTreeMap<String, Output>,
    /// Every node this one depends on, directly or not.
    pub upstream: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Ok,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Evaluation {
    pub graph_id: String,
    pub graph_revision: u64,
    /// Fingerprint of everything the results depend on.
    pub input_hash: String,
    pub status: Status,
    pub nodes: BTreeMap<String, NodeResult>,
    pub diagnostics: Vec<Diagnostic>,
    pub order: Vec<String>,
    pub recomputed: Vec<String>,
    pub reused: Vec<String>,
}

impl Evaluation {
    /// The walls the graph produces, staged for the host to commit.
    pub fn walls(&self) -> Vec<WallValue> {
        self.nodes
            .values()
            .filter(|r| r.status == NodeStatus::Ok)
            .filter_map(|r| match r.outputs.get("wall").map(|o| &o.value) {
                Some(Value::Wall { wall }) => Some((**wall).clone()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
struct Entry {
    key: u64,
    outputs: BTreeMap<String, (Value, Source)>,
    warnings: Vec<Diagnostic>,
}

/// Cached node outputs of one graph, keyed by everything a node's result depends on. Disposable:
/// it is never saved.
#[derive(Clone, Debug, Default)]
pub struct Cache {
    graph_id: String,
    entries: HashMap<String, Entry>,
}

impl Cache {
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn display(v: &Value, length_unit: &str) -> String {
    let round = |x: f64| (x * 1e6).round() / 1e6;
    match v {
        Value::Number { value, dim } => {
            let u = if *dim == Dim::LENGTH { length_unit } else { dim.display_unit() };
            match units::from_si(*value, *dim, u) {
                Some(x) => format!("{} {u}", round(x)).trim().to_string(),
                None => round(*value).to_string(),
            }
        }
        Value::Boolean { value } => value.to_string(),
        Value::Enum { value } | Value::Text { value } => value.clone(),
        Value::Integer { value } => value.to_string(),
        _ => String::new(),
    }
}

/// Run `g`. Results that cannot be current are never shown: a failed node has no outputs and
/// its dependents are blocked. A cancelled evaluation returns no results at all.
pub fn evaluate(g: &Graph, ctx: &Context, cache: &mut Cache, cancel: Option<&AtomicBool>) -> Evaluation {
    let mut ev = Evaluation {
        graph_id: g.graph_id.clone(),
        graph_revision: g.graph_revision,
        input_hash: String::new(),
        status: Status::Ok,
        nodes: BTreeMap::new(),
        diagnostics: Vec::new(),
        order: Vec::new(),
        recomputed: Vec::new(),
        reused: Vec::new(),
    };
    if cache.graph_id != g.graph_id {
        cache.entries.clear();
        cache.graph_id = g.graph_id.clone();
    }
    let (prepared, diags) = validate(g);
    ev.diagnostics = diags;
    let Some(p) = prepared else {
        ev.status = Status::Failed;
        return ev;
    };
    let ctx_hash = fnv64(canonical(&serde_json::to_value(ctx).unwrap_or_default()).as_bytes());
    let length_unit = if units::unit(&g.units_context.display_length).is_some_and(|(d, _)| d == Dim::LENGTH) {
        g.units_context.display_length.as_str()
    } else {
        "m"
    };
    let mut produced: Vec<Option<BTreeMap<String, (Value, Source)>>> = vec![None; g.nodes.len()];
    let mut upstream: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); g.nodes.len()];
    let mut all_keys = Vec::with_capacity(p.order.len());
    for &i in &p.order {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Evaluation {
                status: Status::Cancelled,
                nodes: BTreeMap::new(),
                diagnostics: vec![Diagnostic::error(Code::Cancelled, None, None, "the evaluation was cancelled")],
                recomputed: Vec::new(),
                reused: Vec::new(),
                ..ev
            };
        }
        let node = &g.nodes[i];
        ev.order.push(node.node_id.clone());
        for (s, _) in p.incoming[i].values() {
            let up = upstream[*s].clone();
            upstream[i].extend(up);
            upstream[i].insert(*s);
        }
        let ups: Vec<String> = upstream[i].iter().map(|&u| g.nodes[u].node_id.clone()).collect();
        let finish = |status: NodeStatus, outs: Option<&BTreeMap<String, (Value, Source)>>, ev: &mut Evaluation| {
            let outputs = outs
                .map(|o| {
                    o.iter()
                        .map(|(k, (v, s))| (k.clone(), Output { display: display(v, length_unit), value: v.clone(), source: s.clone() }))
                        .collect()
                })
                .unwrap_or_default();
            ev.nodes.insert(node.node_id.clone(), NodeResult { status, outputs, upstream: ups.clone() });
        };
        if node.state == NodeState::Disabled {
            finish(NodeStatus::Disabled, None, &mut ev);
            continue;
        }
        if p.failed.contains(&i) {
            cache.entries.remove(&node.node_id);
            finish(NodeStatus::Failed, None, &mut ev);
            continue;
        }
        let mut inputs: Inputs = BTreeMap::new();
        let mut blocked = None;
        for (port, (s, sport)) in &p.incoming[i] {
            match produced[*s].as_ref().and_then(|o| o.get(sport)) {
                Some((v, _)) => {
                    inputs.insert(port.clone(), (v.clone(), Source::Graph { node_id: g.nodes[*s].node_id.clone(), port_id: sport.clone() }));
                }
                None => blocked = Some(g.nodes[*s].node_id.clone()),
            }
        }
        if let Some(up) = blocked {
            ev.diagnostics.push(Diagnostic::warning(Code::Blocked, Some(&node.node_id), None, format!("no current value: `{up}` has none")));
            finish(NodeStatus::Blocked, None, &mut ev);
            continue;
        }
        let Some(spec) = &p.specs[i] else {
            finish(NodeStatus::Failed, None, &mut ev);
            continue;
        };
        let mut key_text = format!("{}@{}|{}", node.node_type, node.node_version, canonical(&node.config));
        for (port, (v, _)) in &inputs {
            key_text.push('|');
            key_text.push_str(port);
            key_text.push('=');
            key_text.push_str(&canonical(&serde_json::to_value(v).unwrap_or_default()));
        }
        if spec.reads_context {
            key_text.push_str(&format!("|ctx={ctx_hash:x}"));
        }
        let key = fnv64(key_text.as_bytes());
        all_keys.push(key);
        let hit = cache.entries.get(&node.node_id).filter(|e| e.key == key).cloned();
        let entry = match hit {
            Some(e) => {
                ev.reused.push(node.node_id.clone());
                e
            }
            None => match nodes::evaluate(node, ctx, &inputs) {
                Ok(out) => {
                    ev.recomputed.push(node.node_id.clone());
                    let e = Entry { key, outputs: out.outputs, warnings: out.warnings };
                    cache.entries.insert(node.node_id.clone(), e.clone());
                    e
                }
                Err(d) => {
                    ev.recomputed.push(node.node_id.clone());
                    cache.entries.remove(&node.node_id);
                    ev.diagnostics.push(d);
                    finish(NodeStatus::Failed, None, &mut ev);
                    continue;
                }
            },
        };
        ev.diagnostics.extend(entry.warnings.iter().cloned());
        finish(NodeStatus::Ok, Some(&entry.outputs), &mut ev);
        produced[i] = Some(entry.outputs);
    }
    let live: BTreeSet<&str> = g.nodes.iter().map(|n| n.node_id.as_str()).collect();
    cache.entries.retain(|k, _| live.contains(k.as_str()));
    let mut h = format!("{}#{}#{ctx_hash:x}", g.graph_id, g.graph_revision);
    for k in all_keys {
        h.push_str(&format!("{k:x},"));
    }
    ev.input_hash = format!("{:016x}", fnv64(h.as_bytes()));
    if ev.diagnostics.iter().any(Diagnostic::is_error) {
        ev.status = Status::Failed;
    }
    ev
}

/// The nodes `id` depends on and the nodes that depend on it, for highlighting.
pub fn dependencies(g: &Graph, id: &str) -> (Vec<String>, Vec<String>) {
    let walk = |forward: bool| {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut stack = vec![id.to_string()];
        while let Some(cur) = stack.pop() {
            for e in &g.edges {
                let (from, to) = if forward { (&e.source_node_id, &e.target_node_id) } else { (&e.target_node_id, &e.source_node_id) };
                if *from == cur && to != id && seen.insert(to.clone()) {
                    stack.push(to.clone());
                }
            }
        }
        seen.into_iter().collect::<Vec<_>>()
    };
    (walk(false), walk(true))
}
