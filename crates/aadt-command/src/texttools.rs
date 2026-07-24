//! Text utilities from HCW-TOOLS.lsp: FIXTXT, TextIncrement, TXTALIGN, etc.

use aadt_geometry::Point2;
use aadt_model::{Geometry, Text};

use crate::{parse_entity_id, parse_f64, resolve_targets, Command, Context, Registry};

pub fn register_texttools(r: &mut Registry) {
    r.register(Box::new(FixTxtCmd));
    r.register(Box::new(FixTxtHCmd));
    r.register(Box::new(TextIncrementCmd));
    r.register(Box::new(TxtAlignCmd));
    r.register(Box::new(TxtDupCmd));
    r.register(Box::new(TxtAuditCmd));
    r.register(Box::new(TxtExportCmd));
    r.register(Box::new(TxtStyleCmd));
    r.register(Box::new(WinLabelCmd));
    r.register(Box::new(WinLabelHeightCmd));
}

fn text_entities<'a>(
    ctx: &'a Context,
    ids: &[aadt_model::EntityId],
) -> Vec<(aadt_model::EntityId, &'a Text)> {
    ids.iter()
        .filter_map(|id| {
            let e = ctx.doc.get(*id)?;
            match &e.geometry {
                Geometry::Text(t) => Some((*id, t)),
                _ => None,
            }
        })
        .collect()
}

/// FIXTXT id… — uniform height (max) + vertical spacing so edge gap ≈ 0.5×height.
struct FixTxtCmd;
impl Command for FixTxtCmd {
    fn name(&self) -> &str {
        "FIXTXT"
    }
    fn help(&self) -> &str {
        "FIXTXT [id…] — fix overlapping TEXT (uniform height + vertical spacing)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let ids = if args.is_empty() {
            resolve_targets(ctx, "SELECTION", "FIXTXT")?
        } else {
            args.iter()
                .map(|a| parse_entity_id(a, "FIXTXT"))
                .collect::<Result<Vec<_>, _>>()?
        };
        let texts = text_entities(ctx, &ids);
        if texts.is_empty() {
            return Err("FIXTXT: select TEXT objects first".into());
        }
        let max_h = texts
            .iter()
            .map(|(_, t)| t.height)
            .fold(0.0_f64, f64::max)
            .max(1e-6);
        let c2c = max_h * 1.5;
        let mut items: Vec<(aadt_model::EntityId, Point2, f64)> = texts
            .iter()
            .map(|(id, t)| (*id, t.position, t.position.y))
            .collect();
        items.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        let mut prev_cy: Option<f64> = None;
        let mut updates: Vec<(aadt_model::EntityId, Point2, f64)> = Vec::new();
        for (id, pos, mut cy) in items {
            if let Some(prev) = prev_cy {
                let need = prev + c2c;
                if cy < need {
                    let dy = need - cy;
                    cy = need;
                    updates.push((id, Point2::new(pos.x, pos.y + dy), max_h));
                } else {
                    updates.push((id, pos, max_h));
                }
            } else {
                updates.push((id, pos, max_h));
            }
            prev_cy = Some(cy);
        }
        let n = updates.len();
        ctx.doc.transact("FIXTXT", |d| {
            for (id, pos, h) in updates {
                if let Some(e) = d.get(id) {
                    if let Geometry::Text(t) = &e.geometry {
                        let mut t2 = t.clone();
                        t2.height = h;
                        t2.position = pos;
                        d.replace_geometry(id, Geometry::Text(t2));
                    }
                }
            }
        });
        Ok(format!(
            "FIXTXT done. height={max_h:.4}  edge-gap={:.4}  ({n} texts)",
            max_h * 0.5
        ))
    }
}

struct FixTxtHCmd;
impl Command for FixTxtHCmd {
    fn name(&self) -> &str {
        "FIXTXTH"
    }
    fn help(&self) -> &str {
        "FIXTXTH [id…] — horizontal FIXTXT (push right on overlap)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let ids = if args.is_empty() {
            resolve_targets(ctx, "SELECTION", "FIXTXTH")?
        } else {
            args.iter()
                .map(|a| parse_entity_id(a, "FIXTXTH"))
                .collect::<Result<Vec<_>, _>>()?
        };
        let texts = text_entities(ctx, &ids);
        if texts.is_empty() {
            return Err("FIXTXTH: select TEXT objects first".into());
        }
        let max_h = texts
            .iter()
            .map(|(_, t)| t.height)
            .fold(0.0_f64, f64::max)
            .max(1e-6);
        let c2c = max_h * 1.5;
        let mut items: Vec<(aadt_model::EntityId, Point2, f64)> = texts
            .iter()
            .map(|(id, t)| (*id, t.position, t.position.x))
            .collect();
        items.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        let mut prev_cx: Option<f64> = None;
        let mut updates = Vec::new();
        for (id, pos, mut cx) in items {
            if let Some(prev) = prev_cx {
                let need = prev + c2c;
                if cx < need {
                    let dx = need - cx;
                    cx = need;
                    updates.push((id, Point2::new(pos.x + dx, pos.y), max_h));
                } else {
                    updates.push((id, pos, max_h));
                }
            } else {
                updates.push((id, pos, max_h));
            }
            prev_cx = Some(cx);
        }
        let n = updates.len();
        ctx.doc.transact("FIXTXTH", |d| {
            for (id, pos, h) in updates {
                if let Some(e) = d.get(id) {
                    if let Geometry::Text(t) = &e.geometry {
                        let mut t2 = t.clone();
                        t2.height = h;
                        t2.position = pos;
                        d.replace_geometry(id, Geometry::Text(t2));
                    }
                }
            }
        });
        Ok(format!("FIXTXTH done. height={max_h:.4} ({n} texts)"))
    }
}

/// Last run of digits in a string â†’ (start_byte_idx, len_chars) as char indices.
fn last_num_span(s: &str) -> Option<(usize, usize)> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = chars.len();
    while i > 0 && !chars[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i == 0 {
        return None;
    }
    let end = i;
    while i > 0 && chars[i - 1].is_ascii_digit() {
        i -= 1;
    }
    Some((i, end - i))
}

struct TextIncrementCmd;
impl Command for TextIncrementCmd {
    fn name(&self) -> &str {
        "TEXTINCREMENT"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["TXTINC", "TINC"]
    }
    fn help(&self) -> &str {
        "TEXTINCREMENT id x y — copy TEXT with last number incremented (preserves zero pad)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let id = parse_entity_id(args.first().ok_or("TEXTINCREMENT: missing id")?, "TEXTINCREMENT")?;
        let x = parse_f64(args, 1, "TEXTINCREMENT")?;
        let y = parse_f64(args, 2, "TEXTINCREMENT")?;
        let e = ctx
            .doc
            .get(id)
            .ok_or_else(|| format!("TEXTINCREMENT: no entity {}", id.0))?;
        let Geometry::Text(t) = &e.geometry else {
            return Err("TEXTINCREMENT: select a TEXT object".into());
        };
        let layer = e.layer.clone();
        let span = last_num_span(&t.value).ok_or("TEXTINCREMENT: no number found in text")?;
        let chars: Vec<char> = t.value.chars().collect();
        let (start, len) = span;
        let numstr: String = chars[start..start + len].iter().collect();
        let num: i64 = numstr.parse().unwrap_or(0) + 1;
        let mut newnum = num.to_string();
        while newnum.len() < len {
            newnum.insert(0, '0');
        }
        let prefix: String = chars[..start].iter().collect();
        let suffix: String = chars[start + len..].iter().collect();
        let newtxt = format!("{prefix}{newnum}{suffix}");
        let h = t.height;
        let rot = t.rotation;
        ctx.doc.transact("TEXTINCREMENT", |d| {
            d.add_geometry(
                layer,
                Geometry::Text(Text::new(newtxt.clone(), Point2::new(x, y), h, rot)),
            );
        });
        Ok(format!("TEXTINCREMENT: placed '{newtxt}'"))
    }
}

struct TxtAlignCmd;
impl Command for TxtAlignCmd {
    fn name(&self) -> &str {
        "TXTALIGN"
    }
    fn help(&self) -> &str {
        "TXTALIGN Left|Right|CentreX|Top|Middle|Bottom x y [id…] — align TEXT to axis"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        let axis = args
            .first()
            .ok_or("TXTALIGN: missing axis (Left/Right/CentreX/Top/Middle/Bottom)")?
            .to_ascii_lowercase();
        let rx = parse_f64(args, 1, "TXTALIGN")?;
        let ry = parse_f64(args, 2, "TXTALIGN")?;
        let ids = if args.len() > 3 {
            args[3..]
                .iter()
                .map(|a| parse_entity_id(a, "TXTALIGN"))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            resolve_targets(ctx, "SELECTION", "TXTALIGN")?
        };
        let texts = text_entities(ctx, &ids);
        if texts.is_empty() {
            return Err("TXTALIGN: select TEXT objects".into());
        }
        let updates: Vec<_> = texts
            .iter()
            .map(|(id, t)| {
                let p = match axis.as_str() {
                    "left" | "right" | "centrex" | "centerx" => Point2::new(rx, t.position.y),
                    "top" | "middle" | "bottom" => Point2::new(t.position.x, ry),
                    _ => t.position,
                };
                (*id, p)
            })
            .collect();
        let n = updates.len();
        ctx.doc.transact("TXTALIGN", |d| {
            for (id, pos) in updates {
                if let Some(e) = d.get(id) {
                    if let Geometry::Text(t) = &e.geometry {
                        let mut t2 = t.clone();
                        t2.position = pos;
                        d.replace_geometry(id, Geometry::Text(t2));
                    }
                }
            }
        });
        Ok(format!("TXTALIGN: {n} object(s) aligned to {axis}"))
    }
}

struct TxtDupCmd;
impl Command for TxtDupCmd {
    fn name(&self) -> &str {
        "TXTDUP"
    }
    fn help(&self) -> &str {
        "TXTDUP [DELETE] — find duplicate TEXT (same content + position); DELETE removes them"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        use std::collections::HashMap;
        let delete = args
            .first()
            .is_some_and(|a| a.eq_ignore_ascii_case("DELETE") || a.eq_ignore_ascii_case("Y"));
        let mut seen: HashMap<String, aadt_model::EntityId> = HashMap::new();
        let mut dups = Vec::new();
        for e in ctx.doc.entities() {
            if let Geometry::Text(t) = &e.geometry {
                let key = format!(
                    "{}|{:.3},{:.3}",
                    t.value, t.position.x, t.position.y
                );
                if seen.contains_key(&key) {
                    dups.push(e.id);
                } else {
                    seen.insert(key, e.id);
                }
            }
        }
        if dups.is_empty() {
            return Ok("TXTDUP: No duplicate text found. PASS.".into());
        }
        let n = dups.len();
        if delete {
            ctx.doc.transact("TXTDUP", |d| {
                for id in &dups {
                    d.remove_entity(*id);
                }
            });
            Ok(format!("TXTDUP: {n} duplicate(s) deleted"))
        } else {
            Ok(format!(
                "TXTDUP: {n} duplicate(s) found — run TXTDUP DELETE to remove"
            ))
        }
    }
}

struct TxtAuditCmd;
impl Command for TxtAuditCmd {
    fn name(&self) -> &str {
        "TXTAUDIT"
    }
    fn help(&self) -> &str {
        "TXTAUDIT — count TEXT/MTEXT by layer"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        use std::collections::BTreeMap;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut n = 0usize;
        for e in ctx.doc.entities() {
            if matches!(e.geometry, Geometry::Text(_)) {
                *counts.entry(e.layer.clone()).or_default() += 1;
                n += 1;
            }
        }
        let mut lines = vec![format!("TXTAUDIT: {n} text object(s):")];
        for (lay, c) in counts {
            lines.push(format!("  {lay}: {c}"));
        }
        Ok(lines.join("\n"))
    }
}

struct TxtExportCmd;
impl Command for TxtExportCmd {
    fn name(&self) -> &str {
        "TXTEXPORT"
    }
    fn help(&self) -> &str {
        "TXTEXPORT — print TEXT as CSV (Layer,X,Y,Height,Content)"
    }
    fn run(&self, ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        let mut lines = vec!["Layer,X,Y,Height,Content".to_string()];
        for e in ctx.doc.entities() {
            if let Geometry::Text(t) = &e.geometry {
                let content = t.value.replace(',', " ");
                lines.push(format!(
                    "{},{:.3},{:.3},{:.4},{content}",
                    e.layer, t.position.x, t.position.y, t.height
                ));
            }
        }
        Ok(lines.join("\n"))
    }
}

struct TxtStyleCmd;
impl Command for TxtStyleCmd {
    fn name(&self) -> &str {
        "TXTSTYLE"
    }
    fn help(&self) -> &str {
        "TXTSTYLE — text styles are not yet modelled (uses default rendering)"
    }
    fn run(&self, _ctx: &mut Context, _args: &[String]) -> Result<String, String> {
        Ok("TXTSTYLE: AiADT uses a single default text style for now".into())
    }
}

struct WinLabelCmd;
impl Command for WinLabelCmd {
    fn name(&self) -> &str {
        "WINLABEL"
    }
    fn help(&self) -> &str {
        "WINLABEL [AT x y] [id…] — label INSERTs from WNAME (SELECTION if no ids)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if ctx.doc.layer("WIN_LABELS").is_none() {
            ctx.doc.transact("WINLABEL", |d| {
                d.add_layer(aadt_model::Layer::new("WIN_LABELS").with_color(3));
            });
        }
        let mut ids = Vec::new();
        let mut base = None::<Point2>;
        let mut i = 0;
        if args.first().map(|s| s.eq_ignore_ascii_case("AT")).unwrap_or(false) {
            let x = parse_f64(args, 1, "WINLABEL")?;
            let y = parse_f64(args, 2, "WINLABEL")?;
            base = Some(Point2::new(x, y));
            i = 3;
        }
        while i < args.len() {
            ids.push(parse_entity_id(&args[i], "WINLABEL")?);
            i += 1;
        }
        if ids.is_empty() {
            ids = resolve_targets(ctx, "SELECTION", "WINLABEL")?;
        }
        let h = ctx.room.text_height_units.unwrap_or(0.15);
        let origin = base.unwrap_or_else(|| Point2::new(0.0, 0.0));
        let mut labels = Vec::new();
        for (n, id) in ids.iter().enumerate() {
            let e = ctx
                .doc
                .get(*id)
                .ok_or_else(|| format!("WINLABEL: no entity {}", id.0))?;
            let Geometry::Insert(_) = &e.geometry else {
                continue;
            };
            let Some(wname) = e.attribute("WNAME") else {
                continue;
            };
            let short: String = wname.chars().take(2).collect();
            let pt = Point2::new(origin.x, origin.y - n as f64 * h * 2.5);
            labels.push((short, pt, wname.to_string()));
        }
        if labels.is_empty() {
            return Err(
                "WINLABEL: no INSERT with WNAME in selection (set via EATTEDIT / BPARAM)"
                    .into(),
            );
        }
        let n = labels.len();
        ctx.doc.transact("WINLABEL", |d| {
            for (short, pt, _) in &labels {
                d.add_geometry(
                    "WIN_LABELS",
                    Geometry::Text(Text::new(short.clone(), *pt, h, 0.0)),
                );
            }
        });
        Ok(format!(
            "WINLABEL: labelled {n} — {}",
            labels
                .iter()
                .map(|(_, _, w)| w.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

struct WinLabelHeightCmd;
impl Command for WinLabelHeightCmd {
    fn name(&self) -> &str {
        "WINLABELHEIGHT"
    }
    fn help(&self) -> &str {
        "WINLABELHEIGHT [h] — session default height for future WinLabel (stored in room text height)"
    }
    fn run(&self, ctx: &mut Context, args: &[String]) -> Result<String, String> {
        if let Some(raw) = args.first() {
            let h: f64 = raw
                .parse()
                .map_err(|_| "WINLABELHEIGHT: need a number".to_string())?;
            ctx.room.text_height_units = Some(h.max(1e-6));
        }
        Ok(format!(
            "WinLabel height (drawing units): {:.3}",
            ctx.room.text_height_units.unwrap_or(0.15)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Interpreter;

    #[test]
    fn fixtxt_spaces_overlapping() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "TEXT 0 0 1 A").unwrap();
        i.eval_line(&mut ctx, "TEXT 0 0.2 1 B").unwrap();
        i.eval_line(&mut ctx, "SELECT 0 1").unwrap();
        i.eval_line(&mut ctx, "FIXTXT").unwrap();
        let ys: Vec<f64> = ctx
            .doc
            .entities()
            .iter()
            .filter_map(|e| match &e.geometry {
                Geometry::Text(t) => Some(t.position.y),
                _ => None,
            })
            .collect();
        assert!(ys[1] - ys[0] >= 1.4);
    }

    #[test]
    fn textincrement_pads_zeros() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "TEXT 0 0 1 0 D-01").unwrap();
        let msg = i
            .eval_line(&mut ctx, "TEXTINCREMENT 0 5 0")
            .unwrap()
            .unwrap();
        assert!(msg.contains("D-02"));
    }

    #[test]
    fn winlabel_from_wname_attr() {
        let i = Interpreter::with_builtins();
        let mut ctx = Context::new();
        i.eval_line(&mut ctx, "LINE 0 0 1 0").unwrap();
        i.eval_line(&mut ctx, "SELECT 0").unwrap();
        i.eval_line(&mut ctx, "BLOCK WIN 0 0").unwrap();
        i.eval_line(&mut ctx, "BPARAM WIN WNAME AB12").unwrap();
        i.eval_line(&mut ctx, "INSERT WIN 1 1").unwrap();
        let insert_id = ctx
            .doc
            .entities()
            .iter()
            .find(|e| matches!(e.geometry, Geometry::Insert(_)))
            .unwrap()
            .id;
        i.eval_line(&mut ctx, &format!("SELECT {}", insert_id.0))
            .unwrap();
        let msg = i
            .eval_line(&mut ctx, "WINLABEL AT 0 10")
            .unwrap()
            .unwrap();
        assert!(msg.contains("AB12"));
        let label = ctx
            .doc
            .entities()
            .iter()
            .find_map(|e| match &e.geometry {
                Geometry::Text(t) if e.layer == "WIN_LABELS" => Some(t.value.as_str()),
                _ => None,
            });
        assert_eq!(label, Some("AB"));
    }
}
