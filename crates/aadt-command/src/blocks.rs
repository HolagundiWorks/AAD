//! Parametric-lite block commands: BPARAM, BVIS, BSETVIS, BFLIP, BATT, BPARAMLIST.

use aadt_model::{BlockParamKind, Geometry, BLOCK_FLIP_ATTR, BLOCK_VIS_ATTR};

use crate::{parse_entity_id, Command, Context, Registry};

pub fn register_blocks(r: &mut Registry) {
    r.register(Box::new(BparamCmd));
    r.register(Box::new(BparamListCmd));
    r.register(Box::new(BvisCmd));
    r.register(Box::new(BsetVisCmd));
    r.register(Box::new(BflipCmd));
    r.register(Box::new(BattCmd));
}

fn update_block<F>(ctx: &mut Context, name: &str, label: &str, f: F) -> Result<(), String>
where
    F: FnOnce(&mut aadt_model::Block),
{
    ctx.doc.transact(label, |d| {
        let mut block = d
            .block(name)
            .cloned()
            .ok_or_else(|| format!("block {name:?} not found"))?;
        f(&mut block);
        d.add_block(block);
        Ok(())
    })
}

struct BparamCmd;
impl Command for BparamCmd {
    fn name(&self) -> &str {
        "BPARAM"
    }
    fn help(&self) -> &str {
        "BPARAM block key [default] [string|number] — add/update a block parameter"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let block = args.first().ok_or("BPARAM: missing block name")?.clone();
        let key = args.get(1).ok_or("BPARAM: missing key")?.clone();
        let default = args.get(2).cloned().unwrap_or_default();
        let kind = args
            .get(3)
            .map(|s| BlockParamKind::parse(s))
            .unwrap_or(BlockParamKind::String);
        update_block(ctx, &block, "BPARAM", |b| {
            b.set_param(key.clone(), default.clone(), kind)
        })?;
        Ok(format!(
            "BPARAM: {block}.{key} = {default:?} ({})",
            kind.as_str()
        ))
    }
}

struct BparamListCmd;
impl Command for BparamListCmd {
    fn name(&self) -> &str {
        "BPARAMLIST"
    }
    fn help(&self) -> &str {
        "BPARAMLIST [block] — list block parameters (all blocks if omitted)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let filter = args.first().cloned();
        let mut lines = Vec::new();
        for b in ctx.doc.blocks() {
            if let Some(ref f) = filter {
                if !b.name.eq_ignore_ascii_case(f) {
                    continue;
                }
            }
            if b.params.is_empty() && b.visibilities.len() <= 1 {
                lines.push(format!("{}: (no params)", b.name));
            } else {
                for p in &b.params {
                    lines.push(format!(
                        "{}.{} = {:?} ({})",
                        b.name,
                        p.name,
                        p.default,
                        p.kind.as_str()
                    ));
                }
                for v in &b.visibilities {
                    lines.push(format!(
                        "{}.VISIBILITY[{}] hide {:?}",
                        b.name, v.name, v.hidden
                    ));
                }
            }
        }
        if lines.is_empty() {
            Ok("(no blocks)".into())
        } else {
            Ok(lines.join("\n"))
        }
    }
}

struct BvisCmd;
impl Command for BvisCmd {
    fn name(&self) -> &str {
        "BVIS"
    }
    fn help(&self) -> &str {
        "BVIS block state [hide i j…] — define visibility state (entity indices to hide)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let block = args.first().ok_or("BVIS: missing block name")?.clone();
        let state = args.get(1).ok_or("BVIS: missing state name")?.clone();
        let mut hidden = Vec::new();
        for a in args.iter().skip(2) {
            if a.eq_ignore_ascii_case("hide") {
                continue;
            }
            let i: usize = a.parse().map_err(|_| format!("BVIS: bad index {a:?}"))?;
            hidden.push(i);
        }
        let n = hidden.len();
        update_block(ctx, &block, "BVIS", |b| {
            b.set_visibility(state.clone(), hidden)
        })?;
        Ok(format!("BVIS: {block} state {state:?} hides {n} ent(s)"))
    }
}

struct BsetVisCmd;
impl Command for BsetVisCmd {
    fn name(&self) -> &str {
        "BSETVIS"
    }
    fn help(&self) -> &str {
        "BSETVIS id state — set VISIBILITY attribute on an INSERT"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_entity_id(args.first().ok_or("BSETVIS: missing id")?, "BSETVIS")?;
        let state = args.get(1).ok_or("BSETVIS: missing state")?.clone();
        let e = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("BSETVIS: no entity {}", id.0))?;
        let Geometry::Insert(ins) = &e.geometry else {
            return Err("BSETVIS: select an INSERT".into());
        };
        if let Some(block) = ctx.doc.block(&ins.name) {
            if !block
                .visibilities
                .iter()
                .any(|v| v.name.eq_ignore_ascii_case(&state))
            {
                return Err(format!(
                    "BSETVIS: state {state:?} not defined on block {}",
                    ins.name
                ));
            }
        }
        ctx.doc.transact("BSETVIS", |d| {
            d.set_attribute(id, BLOCK_VIS_ATTR, state.clone());
        });
        Ok(format!("BSETVIS: entity {} VISIBILITY={state:?}", id.0))
    }
}

struct BflipCmd;
impl Command for BflipCmd {
    fn name(&self) -> &str {
        "BFLIP"
    }
    fn help(&self) -> &str {
        "BFLIP id — toggle AADT-FLIP on an INSERT (mirror about local Y)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_entity_id(args.first().ok_or("BFLIP: missing id")?, "BFLIP")?;
        let e = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("BFLIP: no entity {}", id.0))?;
        let Geometry::Insert(_) = &e.geometry else {
            return Err("BFLIP: select an INSERT".into());
        };
        let cur = e
            .attribute(BLOCK_FLIP_ATTR)
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let next = if cur { "0" } else { "1" };
        ctx.doc.transact("BFLIP", |d| {
            d.set_attribute(id, BLOCK_FLIP_ATTR, next);
        });
        Ok(format!("BFLIP: entity {} AADT-FLIP={next}", id.0))
    }
}

struct BattCmd;
impl Command for BattCmd {
    fn name(&self) -> &str {
        "BATT"
    }
    fn help(&self) -> &str {
        "BATT id — list attributes on an INSERT (params + VISIBILITY)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_entity_id(args.first().ok_or("BATT: missing id")?, "BATT")?;
        let e = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("BATT: no entity {}", id.0))?;
        if e.attributes.is_empty() {
            return Ok(format!("BATT {}: (no attributes)", id.0));
        }
        let mut keys: Vec<_> = e.attributes.keys().cloned().collect();
        keys.sort();
        let lines: Vec<_> = keys
            .iter()
            .map(|k| format!("  {k} = {:?}", e.attributes.get(k).unwrap()))
            .collect();
        Ok(format!("BATT {}:\n{}", id.0, lines.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Interpreter;
    use aadt_model::BLOCK_VIS_ATTR;

    #[test]
    fn bparam_seeds_insert_attrs() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "LINE 0 0 1 0").unwrap();
        i.eval_line(&mut ctx, "SELECT 0").unwrap();
        i.eval_line(&mut ctx, "BLOCK WIN 0 0").unwrap();
        i.eval_line(&mut ctx, "BPARAM WIN WNAME W1").unwrap();
        i.eval_line(&mut ctx, "INSERT WIN 5 5").unwrap();
        let e = ctx.doc.get(aadt_model::EntityId(1)).unwrap();
        assert_eq!(e.attribute("WNAME"), Some("W1"));
        assert_eq!(e.attribute(BLOCK_VIS_ATTR), Some("Standard"));
    }

    #[test]
    fn bvis_and_bsetvis() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "LINE 0 0 1 0").unwrap();
        i.eval_line(&mut ctx, "LINE 0 0 0 1").unwrap();
        i.eval_line(&mut ctx, "SELECT 0 1").unwrap();
        i.eval_line(&mut ctx, "BLOCK DOOR 0 0").unwrap();
        i.eval_line(&mut ctx, "BVIS DOOR Swing hide 1").unwrap();
        i.eval_line(&mut ctx, "INSERT DOOR 2 2").unwrap();
        // INSERT is entity 2 after two lines (0,1) — lines remain, insert is next id
        let insert_id = ctx
            .doc
            .entities()
            .iter()
            .find(|e| matches!(e.geometry, aadt_model::Geometry::Insert(_)))
            .unwrap()
            .id;
        i.eval_line(&mut ctx, &format!("BSETVIS {} Swing", insert_id.0))
            .unwrap();
        assert_eq!(
            ctx.doc.get(insert_id).unwrap().attribute(BLOCK_VIS_ATTR),
            Some("Swing")
        );
        let block = ctx.doc.block("DOOR").unwrap();
        assert_eq!(block.hidden_for("Swing"), &[1usize]);
    }

    #[test]
    fn bparam_is_undoable() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "LINE 0 0 1 0").unwrap();
        i.eval_line(&mut ctx, "SELECT 0").unwrap();
        i.eval_line(&mut ctx, "BLOCK WIN 0 0").unwrap();
        assert!(ctx.doc.block("WIN").unwrap().params.is_empty());
        i.eval_line(&mut ctx, "BPARAM WIN WNAME W1").unwrap();
        assert_eq!(ctx.doc.block("WIN").unwrap().params.len(), 1);
        i.eval_line(&mut ctx, "UNDO").unwrap();
        assert!(
            ctx.doc.block("WIN").unwrap().params.is_empty(),
            "UNDO must restore block definition"
        );
    }
}
