//! Natural-language drafting → AADT command lines (Foundry Local / small models).
//!
//! Simple phrases like *"create a room of 12' x 16'"* or *"add a window of 5'
//! to the selected wall"* are handled by a **deterministic local parser** first
//! (works offline). Anything else goes to Foundry Local (`phi-3.5-mini` by
//! default) which must reply with JSON `{"commands":["…"]}`.

use aadt_model::{parse_length, primary_from_props, UnitSystem, METERS_PER_INCH};

use crate::{json_string, string_field, AiError, AuditLog, ModelClient};

/// Default block width for the seeded `WINDOW` definition (metres).
const WINDOW_BLOCK_M: f64 = 1.2;
/// Default block width for `DOOR-SINGLE` (metres).
const DOOR_BLOCK_M: f64 = 0.9;

/// Lightweight fact about one selected entity (no raw geometry dump beyond ends).
#[derive(Debug, Clone, PartialEq)]
pub struct NlSelectionFact {
    pub id: u64,
    pub kind: String,
    pub layer: String,
    /// Line endpoints when known (world units).
    pub a: Option<(f64, f64)>,
    pub b: Option<(f64, f64)>,
}

/// Session hints passed into NL interpretation.
#[derive(Debug, Clone, PartialEq)]
pub struct NlContext {
    /// Drawing units token (`ft-in` / `m`).
    pub units: String,
    /// Placement origin for new rooms / inserts (world).
    pub origin_x: f64,
    pub origin_y: f64,
    pub selection: Vec<NlSelectionFact>,
}

impl Default for NlContext {
    fn default() -> Self {
        Self {
            units: "ft-in".into(),
            origin_x: 0.0,
            origin_y: 0.0,
            selection: Vec::new(),
        }
    }
}

impl NlContext {
    pub fn unit_system(&self) -> UnitSystem {
        primary_from_props(&self.units)
    }

    /// First selected line with endpoints, if any.
    pub fn first_line(&self) -> Option<&NlSelectionFact> {
        self.selection
            .iter()
            .find(|s| s.kind.eq_ignore_ascii_case("LINE") && s.a.is_some() && s.b.is_some())
    }
}

/// Result of interpreting an utterance.
#[derive(Debug, Clone, PartialEq)]
pub struct NlPlan {
    /// How the plan was produced.
    pub source: NlSource,
    /// Command lines to feed the interpreter (in order).
    pub commands: Vec<String>,
    /// Short human summary.
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NlSource {
    Local,
    Model,
}

/// Interpret natural language into CAD command lines.
///
/// 1. Try the local rule parser (fast, offline).
/// 2. Else call the model with a tight JSON-only prompt.
pub fn interpret_nl(
    client: &dyn ModelClient,
    audit: &mut AuditLog,
    utterance: &str,
    ctx: &NlContext,
) -> Result<NlPlan, AiError> {
    let text = utterance.trim();
    if text.is_empty() {
        return Err(AiError::Parse("empty request".into()));
    }
    if let Some(plan) = try_parse_nl_local(text, ctx) {
        audit.record("nl-local", text);
        return Ok(plan);
    }
    let prompt = build_nl_prompt(text, ctx);
    audit.record("nl-model", &prompt);
    let response = client.generate(&prompt)?;
    let commands = parse_nl_response(&response)?;
    if commands.is_empty() {
        return Err(AiError::Parse(
            "model returned no commands — try: create a room of 12' x 16'".into(),
        ));
    }
    Ok(NlPlan {
        source: NlSource::Model,
        note: format!("AI → {} command(s)", commands.len()),
        commands,
    })
}

/// Deterministic patterns for common drafting phrases (no model required).
pub fn try_parse_nl_local(utterance: &str, ctx: &NlContext) -> Option<NlPlan> {
    let lower = utterance.to_ascii_lowercase();
    let units = ctx.unit_system();

    // create / draw / make a room of W x H
    if looks_like_room(&lower) {
        if let Some((w, h)) = extract_pair_size(utterance, units) {
            let name = extract_room_name(utterance).unwrap_or_else(|| "Room".into());
            let x0 = ctx.origin_x;
            let y0 = ctx.origin_y;
            let x1 = x0 + w;
            let y1 = y0 + h;
            let line = format!("ROOMLABEL {name} {x0} {y0} {x1} {y1}");
            return Some(NlPlan {
                source: NlSource::Local,
                note: format!("room {name} {w:.3} × {h:.3}"),
                commands: vec![line],
            });
        }
    }

    // add / place a window of WIDTH on the selected wall
    if looks_like_window(&lower) {
        if let Some(width) = extract_single_size(utterance, units) {
            let line_ent = ctx.first_line()?;
            let (ax, ay) = line_ent.a?;
            let (bx, by) = line_ent.b?;
            let mx = (ax + bx) * 0.5;
            let my = (ay + by) * 0.5;
            let ang = (by - ay).atan2(bx - ax).to_degrees();
            let scale = opening_scale(width, units, WINDOW_BLOCK_M);
            let cmd = format!("INSERT WINDOW {mx} {my} {ang:.4} A-GLAZ {scale:.6}");
            return Some(NlPlan {
                source: NlSource::Local,
                note: format!("window width {width:.3} on entity {}", line_ent.id),
                commands: vec![cmd],
            });
        }
    }

    // add / place a door of WIDTH on the selected wall
    if looks_like_door(&lower) {
        if let Some(width) = extract_single_size(utterance, units) {
            let line_ent = ctx.first_line()?;
            let (ax, ay) = line_ent.a?;
            let (bx, by) = line_ent.b?;
            let mx = (ax + bx) * 0.5;
            let my = (ay + by) * 0.5;
            let ang = (by - ay).atan2(bx - ax).to_degrees();
            let scale = opening_scale(width, units, DOOR_BLOCK_M);
            let cmd = format!("INSERT DOOR-SINGLE {mx} {my} {ang:.4} A-DOOR {scale:.6}");
            return Some(NlPlan {
                source: NlSource::Local,
                note: format!("door width {width:.3} on entity {}", line_ent.id),
                commands: vec![cmd],
            });
        }
    }

    // draw a line from … (simple absolute)
    if lower.contains("draw a line") || lower.starts_with("line ") {
        if let Some((x1, y1, x2, y2)) = extract_two_points(utterance, units) {
            return Some(NlPlan {
                source: NlSource::Local,
                note: "line".into(),
                commands: vec![format!("LINE {x1} {y1} {x2} {y2}")],
            });
        }
    }

    None
}

fn opening_scale(width_world: f64, units: UnitSystem, block_m: f64) -> f64 {
    let width_m = match units {
        UnitSystem::Meters => width_world,
        UnitSystem::FeetInches => width_world * METERS_PER_INCH,
    };
    (width_m / block_m).max(0.05)
}

fn looks_like_room(lower: &str) -> bool {
    (lower.contains("room") || lower.contains("bedroom") || lower.contains("kitchen"))
        && (lower.contains("create")
            || lower.contains("make")
            || lower.contains("draw")
            || lower.contains("add")
            || lower.contains("of "))
}

fn looks_like_window(lower: &str) -> bool {
    lower.contains("window")
        && (lower.contains("add")
            || lower.contains("place")
            || lower.contains("insert")
            || lower.contains("put"))
}

fn looks_like_door(lower: &str) -> bool {
    lower.contains("door")
        && (lower.contains("add")
            || lower.contains("place")
            || lower.contains("insert")
            || lower.contains("put"))
}

fn extract_room_name(utterance: &str) -> Option<String> {
    let lower = utterance.to_ascii_lowercase();
    for (needle, name) in [
        ("master bedroom", "Master Bedroom"),
        ("living room", "Living Room"),
        ("dining room", "Dining Room"),
        ("bedroom", "Bedroom"),
        ("kitchen", "Kitchen"),
        ("bathroom", "Bathroom"),
        ("office", "Office"),
        ("garage", "Garage"),
    ] {
        if lower.contains(needle) {
            return Some(name.into());
        }
    }
    None
}

/// Pull `W x H` / `W by H` sizes (feet-inches or metres tokens).
pub fn extract_pair_size(text: &str, units: UnitSystem) -> Option<(f64, f64)> {
    // Normalize separators.
    let t = text.replace('×', "x").replace(" by ", " x ");
    // Find patterns like 12' x 16' or 12x16 or 3.6m x 4.8m
    let re_parts: Vec<&str> = t.split(['x', 'X']).collect();
    if re_parts.len() < 2 {
        return None;
    }
    // Search for a size token in each half near the x.
    let left = re_parts[re_parts.len() - 2];
    let right = re_parts[re_parts.len() - 1];
    let w = last_length_token(left, units)?;
    let h = first_length_token(right, units)?;
    if w > 0.0 && h > 0.0 {
        Some((w, h))
    } else {
        None
    }
}

/// Pull a single size (`5'`, `1.2m`, `60`).
pub fn extract_single_size(text: &str, units: UnitSystem) -> Option<f64> {
    // Prefer tokens with ' or " or m
    for token in tokenize_lengths(text) {
        if let Ok(v) = parse_length(&token, units) {
            if v > 0.0 {
                return Some(v);
            }
        }
    }
    None
}

fn last_length_token(chunk: &str, units: UnitSystem) -> Option<f64> {
    let mut last = None;
    for token in tokenize_lengths(chunk) {
        if let Ok(v) = parse_length(&token, units) {
            if v > 0.0 {
                last = Some(v);
            }
        }
    }
    last
}

fn first_length_token(chunk: &str, units: UnitSystem) -> Option<f64> {
    for token in tokenize_lengths(chunk) {
        if let Ok(v) = parse_length(&token, units) {
            if v > 0.0 {
                return Some(v);
            }
        }
    }
    None
}

fn tokenize_lengths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() || matches!(c, '\'' | '"' | '.' | '-') {
            cur.push(c);
        } else if matches!(c, 'm' | 'M' | 'f' | 't') && !cur.is_empty() {
            // Tolerate trailing unit letters (`3.6m`, `12ft`) by ending the token.
            out.push(std::mem::take(&mut cur));
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn extract_two_points(text: &str, units: UnitSystem) -> Option<(f64, f64, f64, f64)> {
    let nums: Vec<f64> = tokenize_lengths(text)
        .into_iter()
        .filter_map(|t| parse_length(&t, units).ok())
        .collect();
    if nums.len() >= 4 {
        Some((nums[0], nums[1], nums[2], nums[3]))
    } else {
        None
    }
}

/// Build the Foundry / small-model prompt for free-form NL.
pub fn build_nl_prompt(utterance: &str, ctx: &NlContext) -> String {
    let mut sel = String::from("[");
    for (i, s) in ctx.selection.iter().enumerate() {
        if i > 0 {
            sel.push(',');
        }
        let ends = match (s.a, s.b) {
            (Some((ax, ay)), Some((bx, by))) => {
                format!(",\"a\":[{ax:.3},{ay:.3}],\"b\":[{bx:.3},{by:.3}]")
            }
            _ => String::new(),
        };
        sel.push_str(&format!(
            "{{\"id\":{},\"kind\":{},\"layer\":{}{ends}}}",
            s.id,
            json_string(&s.kind),
            json_string(&s.layer),
        ));
    }
    sel.push(']');

    format!(
        "You translate plain English into AADT CAD command lines. Reply with JSON only:\n\
{{\"commands\":[\"CMD args…\"]}}\n\
Allowed commands (examples):\n\
- ROOMLABEL name x1 y1 x2 y2\n\
- RECTANGLE x1 y1 x2 y2\n\
- LINE x1 y1 x2 y2\n\
- INSERT WINDOW x y rotation_deg A-GLAZ scale\n\
- INSERT DOOR-SINGLE x y rotation_deg A-DOOR scale\n\
Drawing units: {}. Origin for new rooms: ({:.3},{:.3}).\n\
Selection: {sel}\n\
User: {}\n\
Rules: use world coordinates matching units; for a window/door on a selected LINE, \
place INSERT at the midpoint with rotation along the line; scale = width/1.2 (window) \
or width/0.9 (door) when units are metres (convert inches→metres first if ft-in). \
Prefer one or two short commands. No prose.",
        ctx.units, ctx.origin_x, ctx.origin_y, utterance
    )
}

/// Parse `{"commands":["…"]}` from a model reply (tolerant of surrounding prose).
pub fn parse_nl_response(text: &str) -> Result<Vec<String>, AiError> {
    let start = text
        .find("\"commands\"")
        .ok_or_else(|| AiError::Parse("no `commands` field".into()))?;
    let rest = &text[start..];
    let open = rest
        .find('[')
        .ok_or_else(|| AiError::Parse("no commands array".into()))?;
    let close = rest[open..]
        .find(']')
        .ok_or_else(|| AiError::Parse("unterminated commands array".into()))?;
    let array = &rest[open + 1..open + close];
    let mut commands = Vec::new();
    // Pull "…" string literals from the array.
    let mut in_string = false;
    let mut escaped = false;
    let mut cur = String::new();
    for c in array.chars() {
        if in_string {
            if escaped {
                cur.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
                let line = cur.trim().to_string();
                cur.clear();
                if !line.is_empty() {
                    commands.push(line);
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_string = true;
        }
    }
    // Also accept a single string field fallback.
    if commands.is_empty() {
        if let Some(one) = string_field(text, "command") {
            if !one.trim().is_empty() {
                commands.push(one.trim().to_string());
            }
        }
    }
    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockClient;

    fn ft_ctx() -> NlContext {
        NlContext {
            units: "ft-in".into(),
            origin_x: 0.0,
            origin_y: 0.0,
            selection: Vec::new(),
        }
    }

    fn wall_ctx() -> NlContext {
        NlContext {
            units: "ft-in".into(),
            origin_x: 0.0,
            origin_y: 0.0,
            selection: vec![NlSelectionFact {
                id: 3,
                kind: "LINE".into(),
                layer: "A-WALL".into(),
                a: Some((0.0, 0.0)),
                b: Some((192.0, 0.0)),
            }],
        }
    }

    #[test]
    fn local_room_12_by_16_feet() {
        let plan = try_parse_nl_local("create a room of 12' x 16'", &ft_ctx()).unwrap();
        assert_eq!(plan.source, NlSource::Local);
        assert_eq!(plan.commands.len(), 1);
        assert!(plan.commands[0].starts_with("ROOMLABEL Room 0 0 144 192"));
    }

    #[test]
    fn local_bedroom_name() {
        let plan = try_parse_nl_local("make a bedroom of 10' x 12'", &ft_ctx()).unwrap();
        assert!(plan.commands[0].contains("Bedroom"));
        assert!(plan.commands[0].contains("120 144"));
    }

    #[test]
    fn local_window_on_selected_wall() {
        let plan =
            try_parse_nl_local("add a window of 5' to the selected wall", &wall_ctx()).unwrap();
        assert_eq!(plan.commands.len(), 1);
        assert!(plan.commands[0].starts_with("INSERT WINDOW "));
        assert!(plan.commands[0].contains("A-GLAZ"));
    }

    #[test]
    fn local_door_on_selected_wall() {
        let plan =
            try_parse_nl_local("place a door of 3' on the selected wall", &wall_ctx()).unwrap();
        assert!(plan.commands[0].contains("DOOR-SINGLE"));
    }

    #[test]
    fn model_path_parses_commands_json() {
        let client = MockClient::responding(
            "{\"commands\":[\"ROOMLABEL Office 0 0 10 12\",\"ENSUREBLOCKS\"]}",
        );
        let mut audit = AuditLog::new();
        // Force model path with an unrecognized phrase.
        let plan = interpret_nl(
            &client,
            &mut audit,
            "please sketch an office box somehow",
            &ft_ctx(),
        )
        .unwrap();
        assert_eq!(plan.source, NlSource::Model);
        assert_eq!(plan.commands.len(), 2);
        assert_eq!(audit.entries().len(), 1);
        assert_eq!(audit.entries()[0].action, "nl-model");
    }

    #[test]
    fn pair_size_metres() {
        let (w, h) = extract_pair_size("3.6m x 4.8m", UnitSystem::Meters).unwrap();
        assert!((w - 3.6).abs() < 1e-9);
        assert!((h - 4.8).abs() < 1e-9);
    }
}
