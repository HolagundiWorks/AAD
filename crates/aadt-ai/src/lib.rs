//! # aadt-ai — AI Orchestration Layer (flagship runtime discipline, spec §6)
//!
//! On-demand, local AI. No model runs in the background. An AADT AI action extracts
//! **structured data** (never raw geometry or images, spec §6.2) from a detection
//! result, sends a JSON-only prompt to a local model, applies the structured
//! response as ordinary data, and logs exactly what was sent (spec §6.3).
//!
//! This module is split by verifiability:
//! - **Deterministic, fully tested:** structured-data extraction
//!   ([`extract_wall_facts`]), the JSON prompt builder ([`build_wall_type_prompt`]),
//!   the response parser ([`parse_wall_type_response`]), the orchestrator
//!   ([`infer_wall_types`]) behind the [`ModelClient`] trait (exercised with
//!   [`MockClient`]), the [`AuditLog`], and **graceful degradation** (an
//!   unavailable model yields [`AiError::Unavailable`] and never blocks the
//!   deterministic pipeline — spec §6.1 step 7).
//! - **Requires a running model, not unit-tested here:** [`OllamaClient`], a thin
//!   dependency-free HTTP client to `localhost:11434` over `std::net`.
//!
//! The process lifecycle (spawn / warm-idle / auto-unload, spec §6.1) lives in
//! [`lifecycle`]: its policy is fully tested against an injected clock and a fake
//! process, and auto-unload is poll-driven so nothing runs in the background.
//!
//! Other AI actions (anomaly flagging, schedule formatting, notes suggestion)
//! build on this surface in later M4 work.

pub mod lifecycle;

use std::fmt;

use aadt_detect::Detection;
use aadt_model::Document;

/// Local Ollama endpoint AADT talks to (spec §6.1).
pub const OLLAMA_ENDPOINT: &str = "http://localhost:11434";

/// Warm-idle window, in seconds, before the model unloads (spec §6.1 step 6).
pub const WARM_IDLE_SECS: u64 = 150;

// --- Errors ------------------------------------------------------------------

/// Failure modes of an AI action. Callers degrade gracefully on any of these:
/// deterministic drafting and takeoff never depend on AI availability (spec §6.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiError {
    /// The local model could not be reached or timed out.
    Unavailable(String),
    /// The model responded, but the output could not be parsed as expected.
    Parse(String),
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AiError::Unavailable(m) => write!(f, "AI model unavailable: {m}"),
            AiError::Parse(m) => write!(f, "AI response not understood: {m}"),
        }
    }
}

impl std::error::Error for AiError {}

// --- Structured data extraction (spec §6.2) ----------------------------------

/// The structured facts about one wall sent to the model — no geometry coordinates,
/// only the classification-relevant summary.
#[derive(Debug, Clone, PartialEq)]
pub struct WallFact {
    pub id: usize,
    pub length: f64,
    pub thickness: f64,
    pub layer: String,
}

/// Extract per-wall facts from a detection result. This is the only wall data that
/// ever leaves for the model.
pub fn extract_wall_facts(det: &Detection) -> Vec<WallFact> {
    det.walls
        .iter()
        .map(|w| WallFact {
            id: w.id,
            length: w.length,
            thickness: w.thickness,
            layer: w.layer.clone(),
        })
        .collect()
}

// --- Wall-type inference (spec §5.1 step 6) ----------------------------------

/// A wall type the model may assign.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallType {
    Exterior,
    Partition,
    Shaft,
    FireRated,
}

impl WallType {
    /// Canonical lowercase token used in prompts and responses.
    pub fn as_token(self) -> &'static str {
        match self {
            WallType::Exterior => "exterior",
            WallType::Partition => "partition",
            WallType::Shaft => "shaft",
            WallType::FireRated => "fire-rated",
        }
    }

    fn parse(token: &str) -> Option<WallType> {
        match token.trim().to_lowercase().as_str() {
            "exterior" => Some(WallType::Exterior),
            "partition" => Some(WallType::Partition),
            "shaft" => Some(WallType::Shaft),
            "fire-rated" | "fire_rated" | "firerated" => Some(WallType::FireRated),
            _ => None,
        }
    }
}

/// A model-assigned wall type for a specific wall id.
#[derive(Debug, Clone, PartialEq)]
pub struct WallTypeAssignment {
    pub id: usize,
    pub wall_type: WallType,
}

/// Build the deterministic JSON-only prompt for wall-type inference. Numbers are
/// fixed to 3 decimals and layer names are JSON-escaped, so the prompt is stable
/// for a given set of facts (aiding caching and auditability).
pub fn build_wall_type_prompt(facts: &[WallFact]) -> String {
    let mut walls = String::from("[");
    for (i, f) in facts.iter().enumerate() {
        if i > 0 {
            walls.push(',');
        }
        walls.push_str(&format!(
            "{{\"id\":{},\"length\":{:.3},\"thickness\":{:.3},\"layer\":{}}}",
            f.id,
            f.length,
            f.thickness,
            json_string(&f.layer),
        ));
    }
    walls.push(']');

    format!(
        "Classify each wall for an architectural takeoff. Choose one type per wall \
from: exterior, partition, shaft, fire-rated. Base the choice on thickness, layer \
name, and length. Respond with JSON only, no prose, in exactly this shape: \
{{\"assignments\":[{{\"id\":<id>,\"wall_type\":\"<type>\"}}]}}.\nWalls: {{\"walls\":{walls}}}"
    )
}

/// Parse a model response into wall-type assignments. Tolerant of surrounding prose
/// and whitespace: it scans the `assignments` array's objects for `id` and
/// `wall_type`. Unknown wall-type tokens are an error.
pub fn parse_wall_type_response(text: &str) -> Result<Vec<WallTypeAssignment>, AiError> {
    let start = text
        .find("\"assignments\"")
        .ok_or_else(|| AiError::Parse("no `assignments` field".to_string()))?;
    let rest = &text[start..];
    let open = rest
        .find('[')
        .ok_or_else(|| AiError::Parse("no assignments array".to_string()))?;
    let close = rest[open..]
        .find(']')
        .ok_or_else(|| AiError::Parse("unterminated assignments array".to_string()))?;
    let array = &rest[open + 1..open + close];

    let mut assignments = Vec::new();
    for object in split_objects(array) {
        let id = int_field(&object, "id")
            .ok_or_else(|| AiError::Parse(format!("missing id in {object:?}")))?;
        let token = string_field(&object, "wall_type")
            .ok_or_else(|| AiError::Parse(format!("missing wall_type in {object:?}")))?;
        let wall_type = WallType::parse(&token)
            .ok_or_else(|| AiError::Parse(format!("unknown wall_type {token:?}")))?;
        assignments.push(WallTypeAssignment {
            id: id as usize,
            wall_type,
        });
    }
    Ok(assignments)
}

// --- Orchestration + audit (spec §6.1, §6.3) ---------------------------------

/// A transport to a local text-completion model. Implemented by [`OllamaClient`]
/// for real use and [`MockClient`] for tests.
pub trait ModelClient {
    /// Send a prompt, return the model's raw text, or an error to degrade on.
    fn generate(&self, prompt: &str) -> Result<String, AiError>;
}

/// One recorded AI invocation: the action and the exact structured payload sent to
/// the model, for firm audit/compliance (spec §6.3).
#[derive(Debug, Clone, PartialEq)]
pub struct AuditEntry {
    pub action: String,
    pub payload: String,
}

/// An append-only, in-memory audit log of AI invocations.
#[derive(Debug, Default, Clone)]
pub struct AuditLog {
    entries: Vec<AuditEntry>,
}

impl AuditLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, action: impl Into<String>, payload: impl Into<String>) {
        self.entries.push(AuditEntry {
            action: action.into(),
            payload: payload.into(),
        });
    }

    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }
}

/// Infer wall types for the walls in `det`: extract facts, log the payload, prompt
/// the model, and parse the response. Returns [`AiError::Unavailable`] (to be
/// degraded on) if the model cannot be reached. Empty wall sets short-circuit with
/// no model call.
pub fn infer_wall_types(
    client: &dyn ModelClient,
    audit: &mut AuditLog,
    det: &Detection,
) -> Result<Vec<WallTypeAssignment>, AiError> {
    let facts = extract_wall_facts(det);
    if facts.is_empty() {
        return Ok(Vec::new());
    }
    let prompt = build_wall_type_prompt(&facts);
    audit.record("wall-type-inference", &prompt);
    let response = client.generate(&prompt)?;
    parse_wall_type_response(&response)
}

/// The attribute an applied wall type is written to.
pub const WALL_TYPE_ATTR: &str = "AADT-WALL-TYPE";

/// Apply wall-type [`assignments`](WallTypeAssignment) to `doc` as a single
/// undoable transaction (`AI-WALL-TYPES`), returning how many walls were
/// changed.
///
/// This is the step that turns an AI inference into a drawing edit — and the
/// spec's discipline shows in *how*: the result is written as a **non-destructive
/// attribute** on each wall's two face entities (never moving or replacing
/// geometry), through the normal transaction stack, so a single `UNDO` reverts
/// the whole AI pass (spec §6.1). Re-applying the same types changes nothing.
/// `det` resolves each assignment's wall id to its face entities, so `doc` and
/// `det` must describe the same drawing.
pub fn apply_wall_types(
    doc: &mut Document,
    det: &Detection,
    assignments: &[WallTypeAssignment],
) -> usize {
    doc.transact("AI-WALL-TYPES", |d| {
        let mut applied = 0;
        for a in assignments {
            let Some(wall) = det.walls.iter().find(|w| w.id == a.id) else {
                continue;
            };
            let token = a.wall_type.as_token();
            let (e1, e2) = wall.faces;
            // Either face changing counts the wall as (re)classified.
            let c1 = d.set_attribute(e1, WALL_TYPE_ATTR, token);
            let c2 = d.set_attribute(e2, WALL_TYPE_ATTR, token);
            if c1 || c2 {
                applied += 1;
            }
        }
        applied
    })
}

/// A thin, dependency-free HTTP client to a local Ollama server (spec §6.1).
///
/// **Not unit-tested in this repo** — it requires a running Ollama at
/// [`OLLAMA_ENDPOINT`]. It POSTs to `/api/generate` with `stream:false` and
/// `format:json`, and returns the model's `response` text. A refused connection
/// surfaces as [`AiError::Unavailable`], so callers degrade gracefully.
pub struct OllamaClient {
    pub host: String,
    pub port: u16,
    pub model: String,
}

impl OllamaClient {
    /// Client for the default local endpoint with the given model name.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 11434,
            model: model.into(),
        }
    }
}

impl ModelClient for OllamaClient {
    fn generate(&self, prompt: &str) -> Result<String, AiError> {
        use std::io::{Read, Write};
        use std::net::TcpStream;

        let body = format!(
            "{{\"model\":{},\"prompt\":{},\"stream\":false,\"format\":\"json\"}}",
            json_string(&self.model),
            json_string(prompt),
        );
        let request = format!(
            "POST /api/generate HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\n\
Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.host,
            self.port,
            body.len(),
            body
        );

        let mut stream = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|e| AiError::Unavailable(e.to_string()))?;
        stream
            .write_all(request.as_bytes())
            .map_err(|e| AiError::Unavailable(e.to_string()))?;
        let mut raw = String::new();
        stream
            .read_to_string(&mut raw)
            .map_err(|e| AiError::Unavailable(e.to_string()))?;

        let body = raw
            .split_once("\r\n\r\n")
            .map(|(_, b)| b)
            .ok_or_else(|| AiError::Parse("no HTTP body".to_string()))?;
        // Ollama wraps the model text in a "response" field.
        string_field(body, "response")
            .ok_or_else(|| AiError::Parse("no `response` field".to_string()))
    }
}

/// A canned [`ModelClient`] for tests.
#[derive(Debug, Clone)]
pub struct MockClient {
    pub reply: Result<String, AiError>,
}

impl MockClient {
    pub fn responding(text: impl Into<String>) -> Self {
        Self {
            reply: Ok(text.into()),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            reply: Err(AiError::Unavailable("mock offline".to_string())),
        }
    }
}

impl ModelClient for MockClient {
    fn generate(&self, _prompt: &str) -> Result<String, AiError> {
        self.reply.clone()
    }
}

// --- Minimal JSON helpers (dependency-free) ----------------------------------

/// Serialize a string as a JSON string literal (quotes + escaping).
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Split a JSON array body into its top-level `{...}` object substrings.
fn split_objects(array: &str) -> Vec<String> {
    let mut objects = Vec::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut current = String::new();
    for c in array.chars() {
        if in_string {
            current.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                current.push(c);
            }
            '{' => {
                depth += 1;
                current.push(c);
            }
            '}' => {
                depth -= 1;
                current.push(c);
                if depth == 0 {
                    objects.push(std::mem::take(&mut current));
                }
            }
            _ if depth > 0 => current.push(c),
            _ => {}
        }
    }
    objects
}

/// Extract a string field `"key":"value"` from a small JSON object substring.
fn string_field(object: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let after_key = &object[object.find(&needle)? + needle.len()..];
    let colon = after_key.find(':')?;
    let after_colon = &after_key[colon + 1..];
    let open = after_colon.find('"')?;
    let value_region = &after_colon[open + 1..];

    let mut value = String::new();
    let mut escaped = false;
    for c in value_region.chars() {
        if escaped {
            match c {
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                other => value.push(other),
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Some(value);
        } else {
            value.push(c);
        }
    }
    None
}

/// Extract an integer field `"key":<int>` from a small JSON object substring.
fn int_field(object: &str, key: &str) -> Option<i64> {
    let needle = format!("\"{key}\"");
    let after_key = &object[object.find(&needle)? + needle.len()..];
    let colon = after_key.find(':')?;
    let digits: String = after_key[colon + 1..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_detect::{detect, DetectionConfig};
    use aadt_geometry::{Point2, Segment};
    use aadt_model::{Document, EntityId, Geometry};

    fn one_wall_doc() -> Document {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.2), Point2::new(10.0, 0.2))),
        );
        doc
    }

    #[test]
    fn extracts_only_structured_wall_facts() {
        let det = detect(&one_wall_doc(), &DetectionConfig::default());
        let facts = extract_wall_facts(&det);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].layer, "A-WALL");
        assert!((facts[0].length - 10.0).abs() < 1e-9);
    }

    #[test]
    fn prompt_is_deterministic_and_json_only() {
        let det = detect(&one_wall_doc(), &DetectionConfig::default());
        let facts = extract_wall_facts(&det);
        let a = build_wall_type_prompt(&facts);
        let b = build_wall_type_prompt(&facts);
        assert_eq!(a, b);
        assert!(a.contains("\"id\":0,\"length\":10.000,\"thickness\":0.200,\"layer\":\"A-WALL\""));
    }

    #[test]
    fn parses_a_well_formed_response() {
        let text = "{\"assignments\":[{\"id\":0,\"wall_type\":\"exterior\"},\
{\"id\":1,\"wall_type\":\"fire-rated\"}]}";
        let got = parse_wall_type_response(text).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(
            got[0],
            WallTypeAssignment {
                id: 0,
                wall_type: WallType::Exterior
            }
        );
        assert_eq!(got[1].wall_type, WallType::FireRated);
    }

    #[test]
    fn apply_wall_types_annotates_faces_as_one_undoable_pass() {
        let mut doc = one_wall_doc(); // two LINE faces, ids 0 and 1
        let det = detect(&doc, &DetectionConfig::default());
        assert_eq!(det.walls.len(), 1);
        let assignments = vec![WallTypeAssignment {
            id: 0,
            wall_type: WallType::Exterior,
        }];

        let n = apply_wall_types(&mut doc, &det, &assignments);
        assert_eq!(n, 1, "one wall classified");

        // Both of the wall's face entities carry the inferred type.
        for id in [EntityId(0), EntityId(1)] {
            assert_eq!(
                doc.get(id).unwrap().attribute(WALL_TYPE_ATTR),
                Some("exterior")
            );
        }

        // The whole AI pass is one undoable unit.
        assert_eq!(doc.undo_label(), Some("AI-WALL-TYPES"));
        doc.undo();
        for id in [EntityId(0), EntityId(1)] {
            assert_eq!(doc.get(id).unwrap().attribute(WALL_TYPE_ATTR), None);
        }
    }

    #[test]
    fn re_applying_the_same_types_changes_nothing() {
        let mut doc = one_wall_doc();
        let det = detect(&doc, &DetectionConfig::default());
        let assignments = vec![WallTypeAssignment {
            id: 0,
            wall_type: WallType::Partition,
        }];
        assert_eq!(apply_wall_types(&mut doc, &det, &assignments), 1);
        assert_eq!(
            apply_wall_types(&mut doc, &det, &assignments),
            0,
            "the second pass is a no-op"
        );
    }

    #[test]
    fn infer_then_apply_end_to_end_with_a_mock() {
        let mut doc = one_wall_doc();
        let det = detect(&doc, &DetectionConfig::default());
        let client =
            MockClient::responding("{\"assignments\":[{\"id\":0,\"wall_type\":\"fire-rated\"}]}");
        let mut audit = AuditLog::new();

        let assignments = infer_wall_types(&client, &mut audit, &det).unwrap();
        let n = apply_wall_types(&mut doc, &det, &assignments);
        assert_eq!(n, 1);
        assert_eq!(
            doc.get(EntityId(0)).unwrap().attribute(WALL_TYPE_ATTR),
            Some("fire-rated")
        );
        assert_eq!(audit.entries().len(), 1, "the payload sent was audited");
    }

    #[test]
    fn parses_response_with_surrounding_prose() {
        let text = "Sure! Here is the JSON:\n{\"assignments\": [ {\"id\": 3, \
\"wall_type\": \"partition\"} ]}\nHope that helps.";
        let got = parse_wall_type_response(text).unwrap();
        assert_eq!(
            got,
            vec![WallTypeAssignment {
                id: 3,
                wall_type: WallType::Partition
            }]
        );
    }

    #[test]
    fn unknown_wall_type_is_a_parse_error() {
        let text = "{\"assignments\":[{\"id\":0,\"wall_type\":\"banana\"}]}";
        assert!(matches!(
            parse_wall_type_response(text),
            Err(AiError::Parse(_))
        ));
    }

    #[test]
    fn orchestrator_succeeds_and_audits() {
        let det = detect(&one_wall_doc(), &DetectionConfig::default());
        let client =
            MockClient::responding("{\"assignments\":[{\"id\":0,\"wall_type\":\"exterior\"}]}");
        let mut audit = AuditLog::new();
        let got = infer_wall_types(&client, &mut audit, &det).unwrap();
        assert_eq!(
            got,
            vec![WallTypeAssignment {
                id: 0,
                wall_type: WallType::Exterior
            }]
        );
        // The exact structured payload was logged.
        assert_eq!(audit.entries().len(), 1);
        assert_eq!(audit.entries()[0].action, "wall-type-inference");
        assert!(audit.entries()[0].payload.contains("\"layer\":\"A-WALL\""));
    }

    #[test]
    fn orchestrator_degrades_when_model_unavailable() {
        let det = detect(&one_wall_doc(), &DetectionConfig::default());
        let client = MockClient::unavailable();
        let mut audit = AuditLog::new();
        let result = infer_wall_types(&client, &mut audit, &det);
        assert!(matches!(result, Err(AiError::Unavailable(_))));
        // The attempt is still audited (payload was prepared and sent).
        assert_eq!(audit.entries().len(), 1);
    }

    #[test]
    fn empty_document_makes_no_model_call() {
        let det = detect(&Document::new(), &DetectionConfig::default());
        // A client that would panic if called proves no call happens.
        struct Boom;
        impl ModelClient for Boom {
            fn generate(&self, _p: &str) -> Result<String, AiError> {
                panic!("should not be called for an empty wall set");
            }
        }
        let mut audit = AuditLog::new();
        let got = infer_wall_types(&Boom, &mut audit, &det).unwrap();
        assert!(got.is_empty());
        assert!(audit.entries().is_empty());
    }
}
