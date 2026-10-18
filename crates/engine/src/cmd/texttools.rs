//! Text tools carried over from the Holgundi Consulting Works AutoLISP toolkit (the AccelDraw
//! repository's `hcwtools.lsp`, written by the same authors), rebuilt as AAD commands:
//! FIXTXT / FIXTXTH (uniform height, no overlap), TXTALIGN, TXTDUP, TXTAUDIT, TXTEXPORT and
//! TXTINC (copy and increment a numbered label).
//!
//! The edit commands work on the selection or on `handles`; the report commands (TXTDUP,
//! TXTAUDIT, TXTEXPORT) look at all text in the current space when nothing is selected.

use aad_color::Color;
use aad_doc::{EntityKind, HAlign, Handle, VAlign};
use aad_geom::Vec3;
use serde_json::{Value, json};

use super::machines::SelectRun;
use super::*;
use crate::{Result, Session};

/// Rows or points one call will handle; input-sized loops stay bounded.
const MAX_ITEMS: usize = 1_000_000;
const MAX_COPIES: usize = 10_000;
/// Height used when none of the selected text has one.
const DEFAULT_HEIGHT: f64 = 2.5;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("fixtxt", "Fix Overlapping Text", |s, p| run_fix(s, p, Axis::Vertical))
            .menu(&["Architecture", "Text Tools", "Fix Overlapping Text"])
            .params("{handles?} → {fixed, height, moved} (every selected text gets the tallest height and middle-centre justification; text closer than 1.5 heights centre to centre is pushed up, so edges are 0.5 height apart)")
            .interactive(|_| Ok(Box::new(SelectRun::new("fixtxt", "FIXTXT")))),
        CommandSpec::new("fixtxth", "Fix Overlapping Text (Horizontal)", |s, p| run_fix(s, p, Axis::Horizontal))
            .menu(&["Architecture", "Text Tools", "Fix Overlapping Text (Horizontal)"])
            .params("{handles?} → {fixed, height, moved} (as fixtxt, but text is sorted by X and pushed to the right)")
            .interactive(|_| Ok(Box::new(SelectRun::new("fixtxth", "FIXTXTH")))),
        CommandSpec::new("txtalign", "Align Text", run_align)
            .menu(&["Architecture", "Text Tools", "Align Text"])
            .params("{handles?, mode: left|right|centrex|top|middle|bottom, to: [x, y]} → {aligned} (left, right and centrex set the X of the insertion point to `to`; top, middle and bottom set the Y)"),
        CommandSpec::new("txtdup", "Find Duplicate Text", run_dup)
            .menu(&["Architecture", "Text Tools", "Find Duplicate Text"])
            .params("{handles? (default: all text in the current space), delete?: bool} → {duplicates: [handle], deleted} (text with the same content and the same insertion point to 0.001; the first of each is kept)")
            .interactive(|_| Ok(Box::new(SelectRun::new("txtdup", "TXTDUP")))),
        CommandSpec::new("txtaudit", "Audit Text Colours", run_audit)
            .menu(&["Architecture", "Text Tools", "Audit Text Colours"])
            .params("{handles?} → {count, layers: [{layer, count}]} (text with an explicit colour instead of ByLayer, counted per layer)")
            .noundo(),
        CommandSpec::new("txtexport", "Export Text to CSV", run_export)
            .menu(&["Architecture", "Text Tools", "Export Text to CSV"])
            .params("{handles?, path?} → {count, csv?, path?} (Layer, Type, X, Y, Height, Style, Content; written to `path` when given, else returned as `csv`)")
            .noundo(),
        CommandSpec::new("txtinc", "Copy and Increment Text", run_increment)
            .menu(&["Architecture", "Text Tools", "Copy and Increment Text"])
            .params("{handle? (one text; default the selection), points: [[x, y], ...]} → {handles, texts} (one copy per point; the last run of digits counts up by one each time and keeps its leading zeros)"),
    ]
}

#[derive(Clone, Copy, PartialEq)]
enum Axis {
    Vertical,
    Horizontal,
}

/// The text and multiline text in `hs`, or, when `hs` is empty and `all` is set, in the current space.
fn text_handles(s: &Session, p: &Value, all: bool) -> Result<Vec<Handle>> {
    let explicit = p.get("handles").is_some_and(|v| !v.is_null()) || p.get("handle").is_some_and(|v| !v.is_null());
    let mut hs = targets(s, p)?;
    if hs.is_empty() && all && !explicit {
        let space = s.space();
        hs = s.doc()?.space(&space).map(|st| st.iter().map(|e| e.handle).collect()).unwrap_or_default();
    }
    let d = s.doc()?;
    hs.retain(|h| d.entity(*h).is_some_and(|e| matches!(e.kind, EntityKind::Text(_) | EntityKind::MText(_))));
    hs.truncate(MAX_ITEMS);
    Ok(hs)
}

fn locked(s: &Session, h: Handle) -> bool {
    curves::is_locked(s, h)
}

/// Where the text sits and how tall it is. Text already centred and middled is placed by its
/// alignment point; anything else by its insertion point (as the toolkit did).
fn anchor(kind: &EntityKind) -> Option<(Vec3, f64)> {
    match kind {
        EntityKind::Text(t) => {
            let centred = t.halign == HAlign::Center && t.valign == VAlign::Middle;
            Some((if centred { t.align_pt.unwrap_or(t.insert) } else { t.insert }, t.height))
        }
        EntityKind::MText(m) => Some((m.insert, m.height)),
        _ => None,
    }
}

fn run_fix(s: &mut Session, p: &Value, axis: Axis) -> Result<Value> {
    let name = if axis == Axis::Vertical { "fixtxt" } else { "fixtxth" };
    let all = text_handles(s, p, false)?;
    if all.is_empty() {
        return Err(bad(name, "select text or multiline text"));
    }
    let hs: Vec<Handle> = all.into_iter().filter(|h| !locked(s, *h)).collect();
    if hs.is_empty() {
        return Err(bad(name, "the selected text is on locked layers"));
    }
    let d = s.doc()?;
    let tallest =
        hs.iter().filter_map(|h| d.entity(*h)).filter_map(|e| anchor(&e.kind)).map(|(_, h)| h).filter(|h| h.is_finite()).fold(0.0_f64, f64::max);
    let height = if tallest > 0.0 { tallest } else { DEFAULT_HEIGHT };
    let c2c = height * 1.5;
    let mut spots: Vec<(Handle, Vec3)> = hs.iter().filter_map(|h| d.entity(*h).and_then(|e| anchor(&e.kind)).map(|(at, _)| (*h, at))).collect();
    let along = |v: Vec3| if axis == Axis::Vertical { v.y } else { v.x };
    spots.sort_by(|a, b| along(a.1).total_cmp(&along(b.1)));
    // Push apart only where two texts are closer than the minimum.
    let mut moved = 0usize;
    let mut previous: Option<f64> = None;
    for (_, at) in spots.iter_mut() {
        let mut pos = along(*at);
        if let Some(prev) = previous {
            let need = prev + c2c;
            if pos < need {
                if axis == Axis::Vertical {
                    at.y += need - pos;
                } else {
                    at.x += need - pos;
                }
                pos = need;
                moved += 1;
            }
        }
        previous = Some(pos);
    }
    let doc = s.doc_mut()?;
    for (h, at) in &spots {
        doc.modify_entity(*h, |e| match &mut e.kind {
            EntityKind::Text(t) => {
                t.height = height;
                t.halign = HAlign::Center;
                t.valign = VAlign::Middle;
                t.insert = *at;
                t.align_pt = Some(*at);
            }
            EntityKind::MText(m) => {
                m.height = height;
                m.insert = *at;
            }
            _ => {}
        })?;
    }
    Ok(json!({ "fixed": spots.len(), "height": height, "moved": moved }))
}

fn run_align(s: &mut Session, p: &Value) -> Result<Value> {
    let mode = str_param(p, "mode")
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| bad("txtalign", "`mode` is required: left, right, centrex, top, middle or bottom"))?;
    let on_x = match mode.as_str() {
        "left" | "right" | "centrex" => true,
        "top" | "middle" | "bottom" => false,
        _ => return Err(bad("txtalign", "`mode` must be left, right, centrex, top, middle or bottom")),
    };
    let to = point_req("txtalign", p, "to")?;
    let hs: Vec<Handle> = text_handles(s, p, false)?.into_iter().filter(|h| !locked(s, *h)).collect();
    if hs.is_empty() {
        return Err(bad("txtalign", "select text"));
    }
    let doc = s.doc_mut()?;
    for h in &hs {
        doc.modify_entity(*h, |e| {
            let set = |v: &mut Vec3| {
                if on_x {
                    v.x = to.x;
                } else {
                    v.y = to.y;
                }
            };
            match &mut e.kind {
                EntityKind::Text(t) => {
                    set(&mut t.insert);
                    if let Some(a) = t.align_pt.as_mut() {
                        set(a);
                    }
                }
                EntityKind::MText(m) => set(&mut m.insert),
                _ => {}
            }
        })?;
    }
    Ok(json!({ "aligned": hs.len() }))
}

/// The text a label shows: the string of a TEXT, the contents of an MTEXT with paragraph breaks
/// as spaces.
fn content(kind: &EntityKind) -> Option<String> {
    match kind {
        EntityKind::Text(t) => Some(t.value.clone()),
        EntityKind::MText(m) => Some(m.contents.replace("\\P", " ").replace(['\r', '\n'], " ")),
        _ => None,
    }
}

fn run_dup(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = text_handles(s, p, true)?;
    let d = s.doc()?;
    let mut seen = std::collections::HashSet::new();
    let mut dups = Vec::new();
    for h in &hs {
        let Some(e) = d.entity(*h) else { continue };
        let (Some(text), Some((at, _))) = (content(&e.kind), anchor(&e.kind)) else { continue };
        // Rounded to the 0.001 the toolkit used.
        let key = (text, (at.x * 1000.0).round() as i64, (at.y * 1000.0).round() as i64, matches!(e.kind, EntityKind::MText(_)));
        if !seen.insert(key) {
            dups.push(*h);
        }
    }
    let mut deleted = 0usize;
    if bool_or(p, "delete", false) {
        for h in &dups {
            if !locked(s, *h) && s.doc_mut()?.remove_entity(*h).is_some() {
                deleted += 1;
            }
        }
    }
    Ok(json!({ "duplicates": dups.iter().map(|h| h.hex()).collect::<Vec<_>>(), "deleted": deleted }))
}

fn run_audit(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = text_handles(s, p, true)?;
    let d = s.doc()?;
    let mut by_layer: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut count = 0usize;
    for e in hs.iter().filter_map(|h| d.entity(*h)) {
        if matches!(e.common.color, Color::Index(_) | Color::True(_)) {
            *by_layer.entry(e.common.layer.clone()).or_insert(0) += 1;
            count += 1;
        }
    }
    let layers: Vec<Value> = by_layer.into_iter().map(|(layer, n)| json!({ "layer": layer, "count": n })).collect();
    if count > 0 {
        s.echo(format!("{count} text object(s) have an explicit colour."));
    }
    Ok(json!({ "count": count, "layers": layers }))
}

fn csv_field(v: &str) -> String {
    if v.contains([',', '"', '\n', '\r']) { format!("\"{}\"", v.replace('"', "\"\"")) } else { v.to_string() }
}

fn run_export(s: &mut Session, p: &Value) -> Result<Value> {
    let hs = text_handles(s, p, true)?;
    if hs.is_empty() {
        return Err(bad("txtexport", "the drawing has no text"));
    }
    let d = s.doc()?;
    let mut csv = String::from("Layer,Type,X,Y,Height,Style,Content\n");
    for e in hs.iter().filter_map(|h| d.entity(*h)) {
        let (Some(text), Some((at, height))) = (content(&e.kind), anchor(&e.kind)) else { continue };
        let (kind, style) = match &e.kind {
            EntityKind::Text(t) => ("TEXT", t.style.as_str()),
            EntityKind::MText(m) => ("MTEXT", m.style.as_str()),
            _ => continue,
        };
        csv.push_str(&format!(
            "{},{kind},{:.3},{:.3},{height:.4},{},{}\n",
            csv_field(&e.common.layer),
            at.x,
            at.y,
            csv_field(style),
            csv_field(&text)
        ));
    }
    match str_param(p, "path") {
        Some(_path) => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                std::fs::write(_path, &csv).map_err(|e| bad("txtexport", format!("{_path}: {e}")))?;
                return Ok(json!({ "count": hs.len(), "path": _path }));
            }
            #[cfg(target_arch = "wasm32")]
            Err(bad("txtexport", "paths are not available on the web; omit `path` to get the CSV text"))
        }
        None => Ok(json!({ "count": hs.len(), "csv": csv })),
    }
}

/// `(start, len)` of the last run of ASCII digits in `s`, as byte offsets.
fn last_number(s: &str) -> Option<(usize, usize)> {
    let bytes = s.as_bytes();
    let end = bytes.iter().rposition(u8::is_ascii_digit)? + 1;
    let start = bytes[..end].iter().rposition(|b| !b.is_ascii_digit()).map_or(0, |i| i + 1);
    Some((start, end - start))
}

fn run_increment(s: &mut Session, p: &Value) -> Result<Value> {
    let source = *targets(s, p)?.first().ok_or_else(|| bad("txtinc", "select one text"))?;
    let points = points_param(p, "points").ok_or_else(|| bad("txtinc", "`points` must be a list of [x, y] points"))?;
    if points.is_empty() || points.len() > MAX_COPIES {
        return Err(bad("txtinc", format!("give between 1 and {MAX_COPIES} points")));
    }
    let e = s.doc()?.entity(source).cloned().ok_or_else(|| bad("txtinc", "no such object"))?;
    let EntityKind::Text(t) = &e.kind else {
        return Err(bad("txtinc", "select a single-line text"));
    };
    let (start, len) = last_number(&t.value).ok_or_else(|| bad("txtinc", "the text has no number to count up"))?;
    let digits = &t.value[start..start + len];
    let mut n: u64 = digits.parse().map_err(|_| bad("txtinc", "the number in the text is too large"))?;
    let (prefix, suffix) = (&t.value[..start], &t.value[start + len..]);
    let space = s.doc()?.space_of(source).unwrap_or_else(|| s.space());
    let mut handles = Vec::new();
    let mut texts = Vec::new();
    for pt in points {
        n = n.checked_add(1).ok_or_else(|| bad("txtinc", "the number in the text is too large"))?;
        let value = format!("{prefix}{n:0width$}{suffix}", width = len);
        let mut copy = t.clone();
        copy.value = value.clone();
        let at = Vec3::new(pt.x, pt.y, t.insert.z);
        copy.insert = at;
        copy.align_pt = Some(at);
        copy.halign = HAlign::Center;
        copy.valign = VAlign::Middle;
        handles.push(s.doc_mut()?.add(&space, e.common.clone(), EntityKind::Text(copy))?.hex());
        texts.push(value);
    }
    Ok(json!({ "handles": handles, "texts": texts }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &mut Session, x: f64, y: f64, h: f64, v: &str) -> String {
        let r = s.execute("text", &json!({ "at": [x, y], "height": h, "text": v })).unwrap();
        r["handle"].as_str().unwrap().to_string()
    }

    fn texts(s: &Session) -> Vec<(String, Vec3, f64)> {
        s.doc()
            .unwrap()
            .model
            .iter()
            .filter_map(|e| if let EntityKind::Text(t) = &e.kind { Some((t.value.clone(), t.insert, t.height)) } else { None })
            .collect()
    }

    #[test]
    fn the_last_run_of_digits_is_found() {
        assert_eq!(last_number("R-007A"), Some((2, 3)));
        assert_eq!(last_number("12 / 34"), Some((5, 2)));
        assert_eq!(last_number("none"), None);
        assert_eq!(last_number(""), None);
    }

    #[test]
    fn increment_counts_up_keeps_leading_zeros_and_is_bounded() {
        let mut s = Session::new();
        let h = text(&mut s, 0.0, 0.0, 2.5, "R-007A");
        let r = s.execute("txtinc", &json!({ "handle": h, "points": [[10, 0], [20, 0]] })).unwrap();
        assert_eq!(r["texts"], json!(["R-008A", "R-009A"]));
        let none = text(&mut s, 0.0, 5.0, 2.5, "nothing");
        assert!(s.execute("txtinc", &json!({ "handle": none, "points": [[0, 0]] })).is_err());
        let big = text(&mut s, 0.0, 9.0, 2.5, "N99999999999999999999999");
        assert!(s.execute("txtinc", &json!({ "handle": big, "points": [[0, 0]] })).is_err(), "an overflowing number is an error, not a panic");
        let max = text(&mut s, 0.0, 12.0, 2.5, "N18446744073709551615");
        assert!(s.execute("txtinc", &json!({ "handle": max, "points": [[0, 0]] })).is_err());
        assert!(s.execute("txtinc", &json!({ "handle": h, "points": [] })).is_err());
    }

    #[test]
    fn fixtxt_makes_heights_equal_and_pushes_overlaps_apart() {
        let mut s = Session::new();
        let a = text(&mut s, 0.0, 0.0, 2.0, "A");
        let b = text(&mut s, 0.0, 1.0, 4.0, "B");
        let c = text(&mut s, 0.0, 100.0, 2.0, "C");
        let r = s.execute("fixtxt", &json!({ "handles": [a, b, c] })).unwrap();
        assert_eq!(r["height"], json!(4.0));
        assert_eq!(r["moved"], json!(1));
        let t = texts(&s);
        let y = |name: &str| t.iter().find(|x| x.0 == name).unwrap().1.y;
        assert!(t.iter().all(|x| x.2 == 4.0));
        assert!((y("B") - y("A") - 6.0).abs() < 1e-9, "centre to centre is 1.5 heights");
        assert_eq!(y("C"), 100.0, "text that was clear stays put");
        assert!(s.execute("fixtxt", &json!({ "handles": [] })).is_err());
    }

    #[test]
    fn fixtxth_works_along_x() {
        let mut s = Session::new();
        let a = text(&mut s, 0.0, 0.0, 2.0, "A");
        let b = text(&mut s, 1.0, 0.0, 2.0, "B");
        s.execute("fixtxth", &json!({ "handles": [a, b] })).unwrap();
        let t = texts(&s);
        let x = |name: &str| t.iter().find(|v| v.0 == name).unwrap().1.x;
        assert!((x("B") - x("A") - 3.0).abs() < 1e-9);
    }

    #[test]
    fn align_sets_x_or_y_and_rejects_bad_modes() {
        let mut s = Session::new();
        let a = text(&mut s, 3.0, 4.0, 2.0, "A");
        let b = text(&mut s, 9.0, 1.0, 2.0, "B");
        s.execute("txtalign", &json!({ "handles": [a, b], "mode": "left", "to": [5, 50] })).unwrap();
        assert!(texts(&s).iter().all(|t| t.1.x == 5.0));
        s.execute("txtalign", &json!({ "handles": [a, b], "mode": "Top", "to": [5, 50] })).unwrap();
        assert!(texts(&s).iter().all(|t| t.1.y == 50.0));
        assert!(s.execute("txtalign", &json!({ "handles": [a], "mode": "sideways", "to": [0, 0] })).is_err());
        assert!(s.execute("txtalign", &json!({ "handles": [a], "mode": "left" })).is_err());
    }

    #[test]
    fn duplicates_are_found_and_deleted_on_request_keeping_the_first() {
        let mut s = Session::new();
        text(&mut s, 1.0, 1.0, 2.0, "Same");
        text(&mut s, 1.0, 1.0, 2.0, "Same");
        text(&mut s, 1.0004, 1.0, 2.0, "Same");
        text(&mut s, 5.0, 1.0, 2.0, "Same");
        let r = s.execute("txtdup", &json!({})).unwrap();
        assert_eq!(r["duplicates"].as_array().unwrap().len(), 2, "1.0004 rounds to the same 0.001 grid point");
        assert_eq!(texts(&s).len(), 4, "reporting deletes nothing");
        let r = s.execute("txtdup", &json!({ "delete": true })).unwrap();
        assert_eq!(r["deleted"], json!(2));
        assert_eq!(texts(&s).len(), 2);
    }

    #[test]
    fn audit_counts_explicit_colours_per_layer() {
        let mut s = Session::new();
        let a = text(&mut s, 0.0, 0.0, 2.0, "A");
        text(&mut s, 0.0, 5.0, 2.0, "B");
        let h = Handle::parse_hex(&a).unwrap();
        s.doc_mut().unwrap().modify_entity(h, |e| e.common.color = Color::Index(1)).unwrap();
        let r = s.execute("txtaudit", &json!({})).unwrap();
        assert_eq!(r["count"], json!(1));
        assert_eq!(r["layers"][0]["layer"], json!("0"));
    }

    #[test]
    fn export_quotes_fields_and_returns_csv_without_a_path() {
        let mut s = Session::new();
        text(&mut s, 1.0, 2.0, 2.5, "a, \"b\"");
        let r = s.execute("txtexport", &json!({})).unwrap();
        let csv = r["csv"].as_str().unwrap();
        assert!(csv.starts_with("Layer,Type,X,Y,Height,Style,Content\n"));
        assert!(csv.contains("TEXT,1.000,2.000,2.5000,"));
        assert!(csv.contains("\"a, \"\"b\"\"\""));
    }
}
