use std::sync::atomic::AtomicBool;

use serde_json::json;

use crate::nodes::{FORMULA, NUMBER};
use crate::template::add_wall;
use crate::*;

fn wall_graph() -> (Graph, template::WallNodes) {
    let mut g = Graph::new("g1");
    let ids = add_wall(&mut g, "w1", &[[0.0, 0.0], [5.0, 0.0]], false, 3.2, 230.0, None);
    (g, ids)
}

fn num(ev: &Evaluation, node: &str, port: &str) -> f64 {
    ev.nodes.get(node).and_then(|n| n.outputs.get(port)).and_then(|o| o.value.number()).map_or(f64::NAN, |(v, _)| v)
}

fn codes(ev: &Evaluation) -> Vec<Code> {
    ev.diagnostics.iter().map(|d| d.code).collect()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn units_parse_into_canonical_si_and_keep_their_dimension() {
    let q = |s: &str| units::parse_quantity(s).unwrap_or((f64::NAN, Dim::NONE));
    assert!(near(q("230 mm").0, 0.23) && q("230 mm").1 == Dim::LENGTH);
    assert!(near(q("3.2m").0, 3.2));
    assert!(near(q("1 ft").0, 0.3048));
    assert!(near(q("12 m2").0, 12.0) && q("12 m2").1 == Dim::AREA);
    assert_eq!(units::parse_quantity("4"), Some((4.0, Dim::NONE)));
    assert_eq!(units::parse_quantity("4 furlongs"), None);
    assert_eq!(units::parse_quantity("nan m"), None);
    assert_eq!(units::from_si(0.23, Dim::LENGTH, "mm").map(|v| (v * 1e6).round() / 1e6), Some(230.0));
    assert_eq!(units::from_si(0.23, Dim::AREA, "mm"), None, "an area is not shown in millimetres");
}

#[test]
fn formulas_check_dimensions_before_running() {
    let dims = [("h".to_string(), Dim::LENGTH), ("t".to_string(), Dim::LENGTH)].into_iter().collect();
    let ok = |s: &str| formula::parse(s).and_then(|e| formula::infer(&e, &dims));
    assert_eq!(ok("h + 150 mm"), Ok(Dim::LENGTH));
    assert_eq!(ok("h * t"), Ok(Dim::AREA));
    assert_eq!(ok("h * t * 2 m"), Ok(Dim::VOLUME));
    assert_eq!(ok("sqrt(h * t)"), Ok(Dim::LENGTH));
    assert_eq!(ok("h / t"), Ok(Dim::NONE));
    assert_eq!(ok("round(h / 1 mm)"), Ok(Dim::NONE));
    assert!(matches!(ok("h + 2"), Err(formula::FormulaError::UnitMismatch(_))));
    assert!(matches!(ok("sqrt(h)"), Err(formula::FormulaError::UnitMismatch(_))));
    assert!(matches!(ok("round(h)"), Err(formula::FormulaError::UnitMismatch(_))));
    assert!(matches!(ok("system(h)"), Err(formula::FormulaError::Syntax(_))));
    assert!(matches!(ok("h; t"), Err(formula::FormulaError::Syntax(_))));
    assert!(matches!(ok("x + 1"), Err(formula::FormulaError::Syntax(_))));
    let vars = [("h".to_string(), 3.0), ("t".to_string(), 0.0)].into_iter().collect();
    let run = |s: &str| formula::parse(s).and_then(|e| formula::eval(&e, &vars));
    assert_eq!(run("h + 150 mm"), Ok(3.15));
    assert_eq!(run("h / t"), Err(formula::FormulaError::DivisionByZero));
    assert_eq!(run("2 ^ 3"), Ok(8.0));
    assert_eq!(run("-h"), Ok(-3.0));
}

#[test]
fn hostile_formulas_are_refused_without_panicking() {
    for s in [
        "",
        "(".repeat(5000).as_str(),
        "-".repeat(5000).as_str(),
        "1 +",
        "2 ^ 99",
        "2 ^ 1.5",
        "1..2",
        "a b",
        &"1+".repeat(3000),
        "max()",
        "abs(1, 2)",
    ] {
        assert!(formula::parse(s).is_err(), "{s:.40}");
    }
    let mut deep = "(".repeat(60);
    deep.push('1');
    deep.push_str(&")".repeat(60));
    assert!(formula::parse(&deep).is_ok(), "moderate nesting is fine");
    assert!(!formula::valid_input_name("mm"), "a unit cannot name an input");
    assert!(!formula::valid_input_name("sqrt"));
    assert!(formula::valid_input_name("height_1"));
}

#[test]
fn a_wall_graph_evaluates_in_dependency_order_with_canonical_units() {
    let (g, ids) = wall_graph();
    let mut cache = Cache::default();
    let ev = evaluate(&g, &Context::default(), &mut cache, None);
    assert_eq!(ev.status, Status::Ok, "{:?}", ev.diagnostics);
    let pos = |id: &str| ev.order.iter().position(|x| x == id).unwrap_or(usize::MAX);
    assert!(pos(&ids.path) < pos(&ids.wall) && pos(&ids.height) < pos(&ids.wall) && pos(&ids.wall) < pos(&ids.measure));
    assert!(near(num(&ev, &ids.wall, "length"), 5.0));
    assert!(near(num(&ev, &ids.wall, "height"), 3.2));
    assert!(near(num(&ev, &ids.wall, "thickness"), 0.23));
    assert!(near(num(&ev, &ids.measure, "grossFaceArea"), 16.0));
    assert!(near(num(&ev, &ids.measure, "grossVolume"), 16.0 * 0.23));
    let thickness = &ev.nodes[&ids.wall].outputs["thickness"];
    assert_eq!(thickness.display, "230 mm");
    assert_eq!(thickness.source, Source::Graph { node_id: ids.thickness.clone(), port_id: "value".into() });
    assert_eq!(ev.nodes[&ids.measure].outputs["grossFaceArea"].display, "16 m2");
    let walls = ev.walls();
    assert_eq!(walls.len(), 1);
    assert_eq!(walls[0].instance_id, "w1");
    assert_eq!(walls[0].component_type, "osas.wall");
    assert_eq!(walls[0].schema_version, 1);
    let mut up = ev.nodes[&ids.measure].upstream.clone();
    up.sort();
    let mut want = vec![ids.path.clone(), ids.height.clone(), ids.thickness.clone(), ids.wall.clone()];
    want.sort();
    assert_eq!(up, want, "provenance: the measurement traces back to every input");
}

#[test]
fn changing_one_parameter_recomputes_only_its_dependents() {
    let (mut g, ids) = wall_graph();
    let mut cache = Cache::default();
    let first = evaluate(&g, &Context::default(), &mut cache, None);
    assert_eq!(first.recomputed.len(), 5);
    assert!(first.reused.is_empty());
    let again = evaluate(&g, &Context::default(), &mut cache, None);
    assert!(again.recomputed.is_empty(), "nothing changed: {:?}", again.recomputed);
    assert_eq!(again.input_hash, first.input_hash);
    if let Some(n) = g.node_mut(&ids.height) {
        n.config["value"] = json!(4.0);
    }
    let ev = evaluate(&g, &Context::default(), &mut cache, None);
    let mut re = ev.recomputed.clone();
    re.sort();
    let mut want = vec![ids.height.clone(), ids.wall.clone(), ids.measure.clone()];
    want.sort();
    assert_eq!(re, want);
    assert!(ev.reused.contains(&ids.path) && ev.reused.contains(&ids.thickness));
    assert!(near(num(&ev, &ids.measure, "grossFaceArea"), 20.0));
    assert_ne!(ev.input_hash, first.input_hash);
}

#[test]
fn a_changed_project_level_invalidates_the_walls_that_read_it() {
    let mut g = Graph::new("g");
    let ids = add_wall(&mut g, "w", &[[0.0, 0.0], [4.0, 0.0]], false, 3.0, 230.0, None);
    g.edges.retain(|e| e.target_port_id != "height");
    if let Some(n) = g.node_mut(&ids.wall) {
        n.config["level"] = json!("Ground");
    }
    let mut ctx = Context::default();
    ctx.level_heights.insert("Ground".into(), 2.85);
    let mut cache = Cache::default();
    let ev = evaluate(&g, &ctx, &mut cache, None);
    assert!(near(num(&ev, &ids.wall, "height"), 2.85));
    assert_eq!(ev.nodes[&ids.wall].outputs["height"].source, Source::Project { rule: "level `Ground` height".into() });
    ctx.level_heights.insert("Ground".into(), 3.1);
    let ev = evaluate(&g, &ctx, &mut cache, None);
    assert!(ev.recomputed.contains(&ids.wall), "the level changed, so the cached wall is not reused");
    assert!(near(num(&ev, &ids.wall, "height"), 3.1));
}

#[test]
fn wall_parameters_follow_the_precedence_order_and_locks_report_conflicts() {
    let mut g = Graph::new("g");
    let ids = add_wall(&mut g, "w", &[[0.0, 0.0], [4.0, 0.0]], false, 3.0, 300.0, None);
    // Override below a connection: the connection wins.
    if let Some(n) = g.node_mut(&ids.wall) {
        n.config["overrides"] = json!({ "thickness": 200, "finish": "plaster.external" });
    }
    let ev = evaluate(&g, &Context::default(), &mut Cache::default(), None);
    assert!(near(num(&ev, &ids.wall, "thickness"), 0.3));
    assert_eq!(ev.nodes[&ids.wall].outputs["finish"].source, Source::Override);
    // Locked: the override stays and the connection is a visible conflict.
    if let Some(n) = g.node_mut(&ids.wall) {
        n.config["locked"] = json!(["thickness"]);
    }
    let ev = evaluate(&g, &Context::default(), &mut Cache::default(), None);
    assert_eq!(ev.status, Status::Ok);
    assert!(near(num(&ev, &ids.wall, "thickness"), 0.2));
    assert_eq!(ev.nodes[&ids.wall].outputs["thickness"].source, Source::Override);
    assert!(ev.diagnostics.iter().any(|d| d.code == Code::Conflict && d.port_id.as_deref() == Some("thickness")));
    // No connection, no override: the schema default.
    g.edges.retain(|e| e.target_port_id != "thickness");
    if let Some(n) = g.node_mut(&ids.wall) {
        n.config["overrides"] = json!({});
        n.config["locked"] = json!([]);
    }
    let ev = evaluate(&g, &Context::default(), &mut Cache::default(), None);
    assert!(near(num(&ev, &ids.wall, "thickness"), 0.23));
    assert_eq!(ev.nodes[&ids.wall].outputs["thickness"].source, Source::Default);
}

#[test]
fn a_cycle_is_refused_with_the_nodes_around_it() {
    let mut g = Graph::new("g");
    for (id, other) in [("a", "b"), ("b", "a")] {
        g.nodes.push(Node {
            node_id: id.into(),
            node_type: FORMULA.into(),
            node_version: 1,
            config: json!({ "expr": "x + 1 m", "inputs": [{ "name": "x", "dimension": "length" }] }),
            state: NodeState::Active,
        });
        g.edges.push(Edge {
            edge_id: format!("e{id}"),
            source_node_id: other.into(),
            source_port_id: "result".into(),
            target_node_id: id.into(),
            target_port_id: "x".into(),
        });
    }
    let ev = evaluate(&g, &Context::default(), &mut Cache::default(), None);
    assert_eq!(ev.status, Status::Failed);
    let cycle = ev.diagnostics.iter().find(|d| d.code == Code::Cycle);
    let path = cycle.map(|d| d.path.clone()).unwrap_or_default();
    assert_eq!(path.len(), 3, "{path:?}");
    assert_eq!(path.first(), path.last());
    assert!(ev.nodes.is_empty(), "nothing is evaluated");
}

#[test]
fn connections_are_type_and_unit_checked() {
    let (mut g, ids) = wall_graph();
    // An area into a length input.
    g.nodes.push(Node {
        node_id: "area".into(),
        node_type: NUMBER.into(),
        node_version: 1,
        config: json!({ "value": 4, "unit": "m2" }),
        state: NodeState::Active,
    });
    g.edges.retain(|e| e.target_port_id != "height");
    g.edges.push(Edge {
        edge_id: "bad".into(),
        source_node_id: "area".into(),
        source_port_id: "value".into(),
        target_node_id: ids.wall.clone(),
        target_port_id: "height".into(),
    });
    let ev = evaluate(&g, &Context::default(), &mut Cache::default(), None);
    assert!(codes(&ev).contains(&Code::UnitMismatch));
    assert_eq!(ev.nodes[&ids.wall].status, NodeStatus::Failed);
    assert_eq!(ev.nodes[&ids.measure].status, NodeStatus::Blocked, "a dependent of a failed node shows no value");
    assert!(ev.nodes[&ids.measure].outputs.is_empty());
    // A path into a length input.
    g.edges.retain(|e| e.edge_id != "bad");
    g.edges.push(Edge {
        edge_id: "bad2".into(),
        source_node_id: ids.path.clone(),
        source_port_id: "path".into(),
        target_node_id: ids.wall.clone(),
        target_port_id: "height".into(),
    });
    let ev = evaluate(&g, &Context::default(), &mut Cache::default(), None);
    assert!(codes(&ev).contains(&Code::InvalidConnection));
}

#[test]
fn missing_inputs_ranges_and_unknown_versions_are_reported_on_their_node() {
    let (mut g, ids) = wall_graph();
    g.edges.retain(|e| e.target_port_id != "path");
    let ev = evaluate(&g, &Context::default(), &mut Cache::default(), None);
    assert!(ev.diagnostics.iter().any(|d| d.code == Code::MissingInput && d.node_id.as_deref() == Some(ids.wall.as_str())));

    let (mut g, ids) = wall_graph();
    if let Some(n) = g.node_mut(&ids.height) {
        n.config["value"] = json!(9.0);
    }
    let ev = evaluate(&g, &Context::default(), &mut Cache::default(), None);
    let d = ev.diagnostics.iter().find(|d| d.code == Code::OutOfRange);
    assert_eq!(d.and_then(|d| d.node_id.clone()), Some(ids.height.clone()));
    assert!(d.is_some_and(|d| d.message.contains("2.4 m to 6 m")), "{d:?}");

    let (mut g, ids) = wall_graph();
    if let Some(n) = g.node_mut(&ids.wall) {
        n.config["schemaVersion"] = json!(2);
    }
    let ev = evaluate(&g, &Context::default(), &mut Cache::default(), None);
    assert!(ev.diagnostics.iter().any(|d| d.code == Code::OutdatedSchema));

    let (mut g, ids) = wall_graph();
    if let Some(n) = g.node_mut(&ids.measure) {
        n.node_version = 7;
    }
    let ev = evaluate(&g, &Context::default(), &mut Cache::default(), None);
    assert!(ev.diagnostics.iter().any(|d| d.code == Code::OutdatedSchema && d.node_id.as_deref() == Some(ids.measure.as_str())));
}

#[test]
fn a_formula_drives_a_wall_and_division_by_zero_is_its_own_error() {
    let (mut g, ids) = wall_graph();
    g.nodes.push(Node {
        node_id: "ftf".into(),
        node_type: NUMBER.into(),
        node_version: 1,
        config: json!({ "value": 3.3, "unit": "m" }),
        state: NodeState::Active,
    });
    g.nodes.push(Node {
        node_id: "h".into(),
        node_type: FORMULA.into(),
        node_version: 1,
        config: json!({ "expr": "ftf - 150 mm", "inputs": [{ "name": "ftf", "dimension": "length" }] }),
        state: NodeState::Active,
    });
    g.edges.retain(|e| e.target_port_id != "height");
    g.edges.push(Edge {
        edge_id: "e1".into(),
        source_node_id: "ftf".into(),
        source_port_id: "value".into(),
        target_node_id: "h".into(),
        target_port_id: "ftf".into(),
    });
    g.edges.push(Edge {
        edge_id: "e2".into(),
        source_node_id: "h".into(),
        source_port_id: "result".into(),
        target_node_id: ids.wall.clone(),
        target_port_id: "height".into(),
    });
    let ev = evaluate(&g, &Context::default(), &mut Cache::default(), None);
    assert_eq!(ev.status, Status::Ok, "{:?}", ev.diagnostics);
    assert!(near(num(&ev, &ids.wall, "height"), 3.15));
    let (up, down) = dependencies(&g, "h");
    assert_eq!(up, vec!["ftf".to_string()]);
    assert!(down.contains(&ids.wall) && down.contains(&ids.measure));

    if let Some(n) = g.node_mut("h") {
        n.config["expr"] = json!("ftf / (ftf - ftf) * 1 m");
    }
    let ev = evaluate(&g, &Context::default(), &mut Cache::default(), None);
    assert!(codes(&ev).contains(&Code::DivisionByZero));
    assert_eq!(ev.nodes[&ids.wall].status, NodeStatus::Blocked);
}

#[test]
fn a_cancelled_evaluation_returns_no_partial_results() {
    let (g, _) = wall_graph();
    let cancel = AtomicBool::new(true);
    let mut cache = Cache::default();
    let ev = evaluate(&g, &Context::default(), &mut cache, Some(&cancel));
    assert_eq!(ev.status, Status::Cancelled);
    assert!(ev.nodes.is_empty() && ev.walls().is_empty());
}

#[test]
fn graphs_round_trip_and_old_formats_migrate_or_are_refused() {
    let (g, ids) = wall_graph();
    let text = g.to_json().to_string();
    let back = Graph::from_json(&serde_json::from_str(&text).unwrap_or_default());
    assert_eq!(back.as_ref(), Ok(&g));
    let a = evaluate(&g, &Context::default(), &mut Cache::default(), None);
    let b = evaluate(back.as_ref().unwrap_or(&g), &Context::default(), &mut Cache::default(), None);
    assert_eq!(a.input_hash, b.input_hash, "the same graph gives the same results after reloading");

    let v0 = json!({
        "schemaVersion": 0, "graphId": "old",
        "nodes": [{ "nodeId": "n", "nodeType": "param.toggle", "nodeVersion": 1, "config": { "value": true }, "position": [10.0, 20.0] }],
    });
    let m = Graph::from_json(&v0);
    assert!(m.as_ref().is_ok_and(|g| g.schema_version == 1 && g.editor.positions.get("n") == Some(&[10.0, 20.0])), "{m:?}");
    assert!(Graph::from_json(&json!({ "schemaVersion": 9, "graphId": "x" })).is_err());
    assert!(Graph::from_json(&json!({ "graphId": "x" })).is_err());
    assert!(Graph::from_json(&json!({ "schemaVersion": 1, "graphId": "bad id!" })).is_err());
    let mut dup = g.to_json();
    if let Some(nodes) = dup["nodes"].as_array_mut()
        && let Some(first) = nodes.first().cloned()
    {
        nodes.push(first);
    }
    assert!(Graph::from_json(&dup).is_err(), "a node id used twice is refused");
    let _ = ids;
}

#[test]
fn removing_a_node_removes_its_edges_and_blocks_nothing_else() {
    let (mut g, ids) = wall_graph();
    let gone = g.remove_node(&ids.thickness);
    assert_eq!(gone, vec!["w1.e.thickness".to_string()]);
    let ev = evaluate(&g, &Context::default(), &mut Cache::default(), None);
    assert_eq!(ev.status, Status::Ok);
    assert!(near(num(&ev, &ids.wall, "thickness"), 0.23), "back to the schema default");
}

#[test]
fn hostile_graphs_never_panic() {
    let junk = [
        json!(null),
        json!([]),
        json!({ "schemaVersion": 1 }),
        json!({ "schemaVersion": 1, "graphId": "g", "nodes": "x" }),
        json!({ "schemaVersion": 1, "graphId": "g", "nodes": [{ "nodeId": "a", "nodeType": "geom.path", "nodeVersion": 1, "config": { "points": [[0, 0], ["x", 1]] } }] }),
        json!({ "schemaVersion": 1, "graphId": "g", "unitsContext": { "drawingUnitM": -1, "displayLength": "mm" } }),
        json!({ "schemaVersion": 1, "graphId": "g", "nodes": [{ "nodeId": "a", "nodeType": "param.number", "nodeVersion": 1, "config": { "value": 1e308, "unit": "km" } }] }),
        json!({ "schemaVersion": 1, "graphId": "g", "nodes": [{ "nodeId": "a", "nodeType": "math.formula", "nodeVersion": 1, "config": { "expr": "a", "inputs": [{ "name": "a" }, { "name": "a" }] } }] }),
        json!({ "schemaVersion": 1, "graphId": "g", "edges": [{ "edgeId": "e", "sourceNodeId": "q", "sourcePortId": "p", "targetNodeId": "q", "targetPortId": "p" }] }),
        json!({ "schemaVersion": 1, "graphId": "g", "nodes": [{ "nodeId": "a", "nodeType": "component.wall", "nodeVersion": 1, "config": { "instanceId": "", "locked": ["nope"] } }] }),
    ];
    for j in junk {
        if let Ok(g) = Graph::from_json(&j) {
            let ev = evaluate(&g, &Context::default(), &mut Cache::default(), None);
            assert_ne!(ev.status, Status::Cancelled);
        }
    }
    let mut big = Graph::new("big");
    for i in 0..(MAX_NODES + 1) {
        big.nodes.push(Node {
            node_id: format!("n{i}"),
            node_type: "param.toggle".into(),
            node_version: 1,
            config: json!({ "value": true }),
            state: NodeState::Active,
        });
    }
    assert!(Graph::from_json(&big.to_json()).is_err());
    let ev = evaluate(&big, &Context::default(), &mut Cache::default(), None);
    assert!(codes(&ev).contains(&Code::Limit));
}

#[test]
fn the_wall_schema_is_versioned_and_lists_what_each_parameter_affects() {
    let t = component_type("osas.wall", 1);
    assert!(t.as_ref().is_some_and(|t| t.param("thickness").is_some_and(|p| p.affects.geometry)));
    assert!(t.as_ref().is_some_and(|t| t.param("finish").is_some_and(|p| !p.affects.geometry && p.affects.specification)));
    assert!(component_type("osas.wall", 2).is_none());
    let lib = library();
    assert!(lib.iter().all(|e| {
        let n = Node {
            node_id: "x".into(),
            node_type: e.node_type.into(),
            node_version: e.node_version,
            config: e.config.clone(),
            state: NodeState::Active,
        };
        nodes::spec(&n).is_ok()
    }));
}
