//! XCLIP (block/xref insert clip) and VPCLIP (layout viewport clip).

use aad_doc::EntityKind;
use aad_geom::{Bounds2, Vec2};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

const MAX_CLIP: usize = 10_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("xclip", "Clip Block Reference", run_xclip)
            .menu(&["Modify", "Clip", "Xref / Block"])
            .alias(&["xclipframe"])
            .params(
                "{handles? (INSERT), points?: [[x,y],...] (world; 2 = rectangle corners, 3+ = polygon), \
                 delete?: bool, on?: bool, off?: bool} → {handles, clipping, points}",
            ),
        CommandSpec::new("vpclip", "Clip Viewport", run_vpclip)
            .menu(&["View", "Viewports", "Clip"])
            .params(
                "{handle? (VIEWPORT), points?: [[x,y],...] (paper; 3+ polygon or 2 opposite corners), \
                 delete?: bool, rectangular?: bool} → {handle, width, height, clip}",
            ),
    ]
}

fn clip_points(cmd: &str, p: &Value) -> Result<Vec<Vec2>> {
    let pts = points_param(p, "points").ok_or_else(|| bad(cmd, "`points` is required (unless delete/on/off)"))?;
    let pts = distinct_points(&pts);
    if pts.len() < 2 || pts.len() > MAX_CLIP {
        return Err(bad(cmd, format!("`points` needs 2 to {MAX_CLIP} distinct points")));
    }
    if pts.iter().any(|q| !q.is_finite()) {
        return Err(bad(cmd, "`points` must be finite"));
    }
    Ok(pts)
}

fn run_xclip(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "xclip";
    let hs = targets(s, p)?;
    if hs.is_empty() {
        return Err(bad(CMD, "select one or more block references"));
    }
    let delete = bool_or(p, "delete", false);
    let on = bool_or(p, "on", false);
    let off = bool_or(p, "off", false);
    let pts = if delete || (on || off) && p.get("points").is_none() { None } else { Some(clip_points(CMD, p)?) };
    let mut done = Vec::new();
    for h in hs {
        let is_ins = matches!(s.doc()?.entity(h).map(|e| &e.kind), Some(EntityKind::Insert(_)));
        if !is_ins {
            continue;
        }
        s.doc_mut()?.modify_entity(h, |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                if delete {
                    i.clip.clear();
                    i.clipping = false;
                } else if let Some(ref pts) = pts {
                    i.clip = pts.iter().map(|q| Vec2::new(q.x, q.y)).collect();
                    i.clipping = !off;
                } else if on {
                    i.clipping = !i.clip.is_empty();
                } else if off {
                    i.clipping = false;
                }
                done.push(json!({ "handle": h.hex(), "clipping": i.clipping, "points": i.clip.len() }));
            }
        })?;
    }
    if done.is_empty() {
        return Err(bad(CMD, "no INSERT in the selection"));
    }
    Ok(json!({ "clipped": done }))
}

fn run_vpclip(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "vpclip";
    let h = targets(s, p)?.into_iter().next().ok_or_else(|| bad(CMD, "select a viewport"))?;
    let delete = bool_or(p, "delete", false) || bool_or(p, "rectangular", false);
    let pts = if delete { None } else { Some(clip_points(CMD, p)?) };
    let mut out = Value::Null;
    s.doc_mut()?.modify_entity(h, |e| {
        if let EntityKind::Viewport(v) = &mut e.kind {
            if delete {
                v.clip.clear();
            } else if let Some(pts) = pts {
                let poly = if pts.len() == 2 {
                    let (a, b) = (pts[0], pts[1]);
                    vec![a, Vec2::new(b.x, a.y), b, Vec2::new(a.x, b.y)]
                } else {
                    pts
                };
                let box_ = Bounds2::from_points(poly.iter().copied());
                if !box_.is_empty() {
                    v.center = box_.center().to3(0.0);
                    v.width = box_.width().max(1e-9);
                    v.height = box_.height().max(1e-9);
                }
                v.clip = poly;
            }
            out = json!({ "handle": h.hex(), "width": v.width, "height": v.height, "clip": v.clip.len(), "center": [v.center.x, v.center.y] });
        }
    })?;
    if out.is_null() {
        return Err(bad(CMD, "the object is not a viewport"));
    }
    Ok(out)
}
