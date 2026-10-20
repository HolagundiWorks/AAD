//! SELECTSIMILAR and FILTER: grow a selection from properties (reuse QSELECT matching).

use aad_doc::Handle;
use serde_json::{Value, json};

use super::qselect::{Op, compare, properties};
use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("selectsimilar", "Select Similar", run_similar)
            .menu(&["Edit", "Select Similar"])
            .alias(&["selsimilar"])
            .params(
                "{handles?: seed (default: selection), properties?: [\"layer\",\"type\",\"color\",\"linetype\",\"lineweight\"] (default those five), \
                 mode?: new|append (default append)} → selected handles",
            )
            .noundo(),
        CommandSpec::new("filter", "Filter Selection", run_filter)
            .menu(&["Edit", "Filter..."])
            .alias(&["filterselect"])
            .params(
                "{filters: [{type?, property?, operator?: \"=\"|\"!=\"|\">\"|\"<\"|\"*\", value?, layer?, color?}, ...], \
                 mode?: new|append|exclude, applyTo?: drawing|selection} → selected handles (AND of filters)",
            )
            .noundo(),
    ]
}

const DEFAULT_PROPS: &[&str] = &["layer", "type", "color", "linetype", "lineweight"];
const MAX_FILTERS: usize = 64;
const MAX_SCAN: usize = 2_000_000;

fn run_similar(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "selectsimilar";
    let seeds = targets(s, p)?;
    if seeds.is_empty() {
        return Err(bad(CMD, "select one or more objects first"));
    }
    let props: Vec<String> = p
        .get("properties")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).take(32).map(str::to_string).collect())
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_PROPS.iter().map(|s| (*s).to_string()).collect());
    let mode = str_param(p, "mode").unwrap_or("append").to_ascii_lowercase();
    if mode != "new" && mode != "append" {
        return Err(bad(CMD, "`mode` must be new or append"));
    }
    let d = s.doc()?;
    let mut signatures: Vec<Vec<(String, Value)>> = Vec::new();
    for h in &seeds {
        let Some(e) = d.entity(*h) else { continue };
        let all = properties(d, e);
        let sig: Vec<(String, Value)> = props
            .iter()
            .filter_map(|name| all.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(k, v)| (k.clone(), v.clone())))
            .collect();
        if !sig.is_empty() {
            signatures.push(sig);
        }
    }
    if signatures.is_empty() {
        return Err(bad(CMD, "the selection has no matching properties"));
    }
    let space = s.space();
    let mut matched = Vec::new();
    for e in d.space(&space).into_iter().flat_map(|st| st.iter()).take(MAX_SCAN) {
        let all = properties(d, e);
        let ok = signatures.iter().any(|sig| sig.iter().all(|(k, want)| all.iter().any(|(ak, av)| ak.eq_ignore_ascii_case(k) && compare(av, Op::Eq, want))));
        if ok {
            matched.push(e.handle);
        }
    }
    let sel = if mode == "append" {
        let mut cur = s.selection();
        for h in matched {
            if !cur.contains(&h) {
                cur.push(h);
            }
        }
        cur
    } else {
        matched
    };
    let n = sel.len();
    s.set_selection(sel.clone());
    Ok(json!({ "selected": n, "handles": sel.iter().map(|h| h.hex()).collect::<Vec<_>>(), "properties": props }))
}

fn run_filter(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "filter";
    let filters = p.get("filters").and_then(Value::as_array).ok_or_else(|| bad(CMD, "`filters` (array) is required"))?;
    if filters.is_empty() || filters.len() > MAX_FILTERS {
        return Err(bad(CMD, format!("`filters` needs 1 to {MAX_FILTERS} entries")))
    }
    let apply = str_param(p, "applyTo").unwrap_or("drawing").to_ascii_lowercase();
    let mode = str_param(p, "mode").unwrap_or("new").to_ascii_lowercase();
    let space = s.space();
    let mut cands: Vec<Handle> = if apply.starts_with("sel") {
        s.selection()
    } else {
        s.doc()?.space(&space).map(|st| st.iter().take(MAX_SCAN).map(|e| e.handle).collect()).unwrap_or_default()
    };
    let d = s.doc()?;
    for f in filters {
        let ty = f.get("type").and_then(Value::as_str);
        let op = match f.get("operator").and_then(Value::as_str) {
            Some(o) => Some(Op::parse(o).ok_or_else(|| bad(CMD, format!("unknown operator `{o}`")))?),
            None => None,
        };
        let prop = f.get("property").and_then(Value::as_str);
        let value = f.get("value").cloned();
        let layer = f.get("layer").and_then(Value::as_str);
        let color = f.get("color");
        cands.retain(|h| {
            let Some(e) = d.entity(*h) else { return false };
            if let Some(t) = ty
                && !e.kind.type_name().eq_ignore_ascii_case(t)
            {
                return false;
            }
            if let Some(l) = layer
                && !e.common.layer.eq_ignore_ascii_case(l)
            {
                return false;
            }
            if let Some(c) = color
                && !compare(&Value::String(e.common.color.name()), Op::Eq, c)
            {
                return false;
            }
            if let Some(pr) = prop {
                let v = value.clone().unwrap_or(Value::String("*".into()));
                let Some(pv) = properties(d, e).into_iter().find(|(k, _)| k.eq_ignore_ascii_case(pr)).map(|(_, v)| v) else {
                    return false;
                };
                return compare(&pv, op.unwrap_or(Op::Eq), &v);
            }
            true
        });
    }
    let sel = match mode.as_str() {
        "append" => {
            let mut cur = s.selection();
            for h in cands {
                if !cur.contains(&h) {
                    cur.push(h);
                }
            }
            cur
        }
        "exclude" => s.selection().into_iter().filter(|h| !cands.contains(h)).collect(),
        _ => cands,
    };
    let n = sel.len();
    s.set_selection(sel.clone());
    Ok(json!({ "selected": n, "handles": sel.iter().map(|h| h.hex()).collect::<Vec<_>>() }))
}
