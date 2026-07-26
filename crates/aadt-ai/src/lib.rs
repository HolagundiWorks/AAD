//! # aadt-ai — AI Orchestration Layer (flagship runtime discipline, spec §6)
//!
//! On-demand, **local** AI. No model runs in the background. An AADT AI action
//! extracts **structured data** (never raw geometry or images, spec §6.2) from a
//! detection result, sends a JSON-only prompt to a local model, applies the
//! structured response as ordinary data, and logs exactly what was sent
//! (spec §6.3).
//!
//! **Default runtime: [Microsoft Foundry Local](https://learn.microsoft.com/azure/ai-foundry/foundry-local/)**
//! via [`FoundryLocalClient`] (OpenAI-compatible Chat Completions on localhost).
//! [`OllamaClient`] remains available as an optional alternate backend.
//!
//! This module is split by verifiability:
//! - **Deterministic, fully tested:** structured-data extraction
//!   ([`extract_wall_facts`]), the JSON prompt builder ([`build_wall_type_prompt`]),
//!   the response parser ([`parse_wall_type_response`]), the orchestrator
//!   ([`infer_wall_types`]) behind the [`ModelClient`] trait (exercised with
//!   [`MockClient`]), the [`AuditLog`], and **graceful degradation**.
//! - **Requires a running local service, not unit-tested here:**
//!   [`FoundryLocalClient`] / [`OllamaClient`] over `std::net`.
//!
//! The process lifecycle (spawn / warm-idle / auto-unload, spec §6.1) lives in
//! [`lifecycle`].

pub mod foundry;
pub mod foundry_ctl;
pub mod lifecycle;
pub mod nl;

pub use foundry::{
    resolved_endpoint, resolved_model, FoundryLocalClient, FoundryStatus, DEFAULT_FOUNDRY_MODEL,
    FOUNDRY_LOCAL_ENDPOINT,
};
pub use foundry_ctl::{
    cli_available as foundry_cli_available, discover_endpoint, list_cached_aliases, model_download,
    model_list, model_load, model_unload, server_restart, server_start, server_status, server_stop,
    wait_for_endpoint, FoundryModelInfo, FoundryServiceInfo,
};
pub use nl::{
    build_nl_prompt, extract_pair_size, extract_single_size, interpret_nl, parse_nl_response,
    try_parse_nl_local, NlContext, NlPlan, NlSelectionFact, NlSource,
};

use std::fmt;

use aadt_detect::Detection;
use aadt_model::Document;

/// Local Ollama endpoint (optional alternate backend).
pub const OLLAMA_ENDPOINT: &str = "http://localhost:11434";

/// Warm-idle window, in seconds, before the model unloads (spec §6.1 step 6).
pub const WARM_IDLE_SECS: u64 = 150;

/// Which local AI backend to use for inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiBackend {
    /// Microsoft Foundry Local (default inbuilt AI).
    #[default]
    FoundryLocal,
    /// Ollama (`ollama serve` on port 11434).
    Ollama,
}

impl AiBackend {
    /// Parse `foundry` / `foundry-local` / `ollama` (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "foundry" | "foundry-local" | "foundrylocal" | "fl" => Some(AiBackend::FoundryLocal),
            "ollama" => Some(AiBackend::Ollama),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AiBackend::FoundryLocal => "foundry-local",
            AiBackend::Ollama => "ollama",
        }
    }
}

/// Build the default client for `backend` and `model` (empty model → backend default).
pub fn make_client(backend: AiBackend, model: &str) -> Box<dyn ModelClient> {
    match backend {
        AiBackend::FoundryLocal => {
            let m = resolved_model(model);
            Box::new(FoundryLocalClient::new(m))
        }
        AiBackend::Ollama => {
            let m = if model.is_empty() {
                "llama3".to_string()
            } else {
                model.to_string()
            };
            Box::new(OllamaClient::new(m))
        }
    }
}

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

    pub fn parse(token: &str) -> Option<WallType> {
        match token.trim().to_lowercase().as_str() {
            "exterior" => Some(WallType::Exterior),
            "partition" => Some(WallType::Partition),
            "shaft" => Some(WallType::Shaft),
            "fire-rated" | "fire_rated" | "firerated" => Some(WallType::FireRated),
            _ => None,
        }
    }
}

/// Lifecycle of an AI (or user) classification on an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassificationStatus {
    Proposed,
    Accepted,
    Frozen,
    Rejected,
}

impl ClassificationStatus {
    pub fn as_token(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Frozen => "frozen",
            Self::Rejected => "rejected",
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "proposed" => Some(Self::Proposed),
            "accepted" => Some(Self::Accepted),
            "frozen" => Some(Self::Frozen),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}

/// Default auto-accept threshold (≥ this → `accepted`, else `proposed`).
pub const DEFAULT_AUTO_ACCEPT_THRESHOLD: f64 = 0.98;

/// The attribute an applied wall type is written to.
pub const WALL_TYPE_ATTR: &str = "AADT-WALL-TYPE";
/// `proposed` | `accepted` | `frozen` | `rejected`.
pub const CLASS_STATUS_ATTR: &str = "AADT-CLASS-STATUS";
/// Confidence `0.0`–`1.0`.
pub const CLASS_CONF_ATTR: &str = "AADT-CLASS-CONF";
/// Integer revision bumped on accept / override / freeze.
pub const CLASS_REV_ATTR: &str = "AADT-CLASS-REV";
/// `ai` | `user`.
pub const CLASS_SOURCE_ATTR: &str = "AADT-CLASS-SOURCE";

/// A model-assigned wall type for a specific wall id, with confidence.
#[derive(Debug, Clone, PartialEq)]
pub struct WallTypeAssignment {
    pub id: usize,
    pub wall_type: WallType,
    /// 0.0–1.0; 0.0 means “unknown / use heuristic”.
    pub confidence: f64,
}

/// One row in the REVIEW list (sorted by ascending confidence).
#[derive(Debug, Clone, PartialEq)]
pub struct ClassificationReviewItem {
    pub entity_id: aadt_model::EntityId,
    pub wall_type: String,
    pub status: ClassificationStatus,
    pub confidence: f64,
    pub source: String,
    pub revision: i64,
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
name, and length. Include a confidence from 0.0 to 1.0 for each assignment. \
Respond with JSON only, no prose, in exactly this shape: \
{{\"assignments\":[{{\"id\":<id>,\"wall_type\":\"<type>\",\"confidence\":<0.0-1.0>}}]}}.\n\
Walls: {{\"walls\":{walls}}}"
    )
}

/// Heuristic confidence when the model omits it (layer / thickness cues).
pub fn heuristic_confidence(fact: &WallFact, wall_type: WallType) -> f64 {
    let layer = fact.layer.to_ascii_uppercase();
    let mut score: f64 = 0.75;
    match wall_type {
        WallType::Exterior => {
            if layer.contains("EXT") || layer.contains("EXTERIOR") {
                score = 0.95;
            } else if fact.thickness >= 0.25 {
                score = 0.88;
            }
        }
        WallType::Partition => {
            if layer.contains("PART") || layer.contains("INT") {
                score = 0.93;
            } else if fact.thickness > 0.0 && fact.thickness < 0.2 {
                score = 0.86;
            }
        }
        WallType::Shaft => {
            if layer.contains("SHAFT") || layer.contains("VERT") {
                score = 0.94;
            }
        }
        WallType::FireRated => {
            if layer.contains("FIRE") || layer.contains("FRR") {
                score = 0.96;
            }
        }
    }
    score.clamp(0.0, 1.0)
}

/// Fill missing / zero confidences from heuristics using matching facts.
pub fn fill_assignment_confidence(facts: &[WallFact], assignments: &mut [WallTypeAssignment]) {
    for a in assignments.iter_mut() {
        if a.confidence > 0.0 {
            a.confidence = a.confidence.clamp(0.0, 1.0);
            continue;
        }
        if let Some(f) = facts.iter().find(|f| f.id == a.id) {
            a.confidence = heuristic_confidence(f, a.wall_type);
        } else {
            a.confidence = 0.75;
        }
    }
}

/// Parse a model response into wall-type assignments. Tolerant of surrounding prose
/// and whitespace: it scans the `assignments` array's objects for `id` and
/// `wall_type`. Optional `confidence`; unknown wall-type tokens are an error.
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
        let confidence = float_field(&object, "confidence").unwrap_or(0.0);
        assignments.push(WallTypeAssignment {
            id: id as usize,
            wall_type,
            confidence,
        });
    }
    Ok(assignments)
}

/// True if either face of the wall carries a frozen classification.
pub fn wall_is_frozen(doc: &Document, wall: &aadt_detect::Wall) -> bool {
    for id in [wall.faces.0, wall.faces.1] {
        if let Some(e) = doc.get(id) {
            if e.attribute(CLASS_STATUS_ATTR) == Some(ClassificationStatus::Frozen.as_token()) {
                return true;
            }
        }
    }
    false
}

/// Extract wall facts, skipping walls whose classification is frozen.
pub fn extract_wall_facts_open(doc: &Document, det: &Detection) -> Vec<WallFact> {
    det.walls
        .iter()
        .filter(|w| !wall_is_frozen(doc, w))
        .map(|w| WallFact {
            id: w.id,
            length: w.length,
            thickness: w.thickness,
            layer: w.layer.clone(),
        })
        .collect()
}

// --- Orchestration + audit (spec §6.1, §6.3) ---------------------------------

/// A transport to a local text-completion model. Implemented by
/// [`FoundryLocalClient`] (default), [`OllamaClient`], and [`MockClient`] (tests).
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

/// Infer wall types for all walls in `det` (does not skip frozen — prefer
/// [`infer_wall_types_open`] for propose/review workflows).
pub fn infer_wall_types(
    client: &dyn ModelClient,
    audit: &mut AuditLog,
    det: &Detection,
) -> Result<Vec<WallTypeAssignment>, AiError> {
    let facts = extract_wall_facts(det);
    infer_from_facts(client, audit, &facts)
}

/// Infer wall types, skipping walls already frozen on `doc`.
pub fn infer_wall_types_open(
    client: &dyn ModelClient,
    audit: &mut AuditLog,
    det: &Detection,
    doc: &Document,
) -> Result<Vec<WallTypeAssignment>, AiError> {
    let facts = extract_wall_facts_open(doc, det);
    infer_from_facts(client, audit, &facts)
}

fn infer_from_facts(
    client: &dyn ModelClient,
    audit: &mut AuditLog,
    facts: &[WallFact],
) -> Result<Vec<WallTypeAssignment>, AiError> {
    if facts.is_empty() {
        return Ok(Vec::new());
    }
    let prompt = build_wall_type_prompt(facts);
    audit.record("wall-type-inference", &prompt);
    let response = client.generate(&prompt)?;
    let mut assignments = parse_wall_type_response(&response)?;
    fill_assignment_confidence(facts, &mut assignments);
    Ok(assignments)
}

fn read_rev(e: &aadt_model::Entity) -> i64 {
    e.attribute(CLASS_REV_ATTR)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn write_face_attrs(
    d: &mut Document,
    id: aadt_model::EntityId,
    wall_type: &str,
    status: ClassificationStatus,
    confidence: f64,
    source: &str,
    bump_rev: bool,
) -> bool {
    let mut changed = false;
    changed |= d.set_attribute(id, WALL_TYPE_ATTR, wall_type);
    changed |= d.set_attribute(id, CLASS_STATUS_ATTR, status.as_token());
    changed |= d.set_attribute(id, CLASS_CONF_ATTR, format!("{confidence:.4}"));
    changed |= d.set_attribute(id, CLASS_SOURCE_ATTR, source);
    if bump_rev {
        let rev = d.get(id).map(read_rev).unwrap_or(0) + 1;
        changed |= d.set_attribute(id, CLASS_REV_ATTR, rev.to_string());
    } else if d
        .get(id)
        .and_then(|e| e.attribute(CLASS_REV_ATTR))
        .is_none()
    {
        changed |= d.set_attribute(id, CLASS_REV_ATTR, "0");
    }
    changed
}

fn propose_one_face(
    d: &mut Document,
    id: aadt_model::EntityId,
    wall_type: &str,
    status: ClassificationStatus,
    confidence: f64,
) -> bool {
    let prev_type = d
        .get(id)
        .and_then(|e| e.attribute(WALL_TYPE_ATTR))
        .map(str::to_string);
    let prev_status = d
        .get(id)
        .and_then(|e| e.attribute(CLASS_STATUS_ATTR))
        .map(str::to_string);
    let meaningful = prev_type.as_deref() != Some(wall_type)
        || prev_status.as_deref() != Some(status.as_token());
    write_face_attrs(d, id, wall_type, status, confidence, "ai", meaningful)
}

/// Propose classifications: writes type + confidence + status (`accepted` if
/// confidence ≥ `auto_accept_threshold`, else `proposed`). Never overwrites
/// `frozen` faces. Returns how many walls were updated.
pub fn propose_wall_types(
    doc: &mut Document,
    det: &Detection,
    assignments: &[WallTypeAssignment],
    auto_accept_threshold: f64,
) -> usize {
    doc.transact("AI-WALL-PROPOSE", |d| {
        let mut applied = 0;
        for a in assignments {
            let Some(wall) = det.walls.iter().find(|w| w.id == a.id) else {
                continue;
            };
            if wall_is_frozen(d, wall) {
                continue;
            }
            let conf = a.confidence.clamp(0.0, 1.0);
            let status = if conf >= auto_accept_threshold {
                ClassificationStatus::Accepted
            } else {
                ClassificationStatus::Proposed
            };
            let token = a.wall_type.as_token();
            let (e1, e2) = wall.faces;
            let c1 = propose_one_face(d, e1, token, status, conf);
            let c2 = propose_one_face(d, e2, token, status, conf);
            if c1 || c2 {
                applied += 1;
            }
        }
        applied
    })
}

/// Legacy entry point: propose with the default auto-accept threshold.
pub fn apply_wall_types(
    doc: &mut Document,
    det: &Detection,
    assignments: &[WallTypeAssignment],
) -> usize {
    propose_wall_types(doc, det, assignments, DEFAULT_AUTO_ACCEPT_THRESHOLD)
}

/// Accept classifications on the given face entity ids. Optional `type_override`
/// replaces the wall type and marks source as `user`.
pub fn accept_classifications(
    doc: &mut Document,
    entity_ids: &[aadt_model::EntityId],
    type_override: Option<&str>,
) -> usize {
    doc.transact("AI-WALL-ACCEPT", |d| {
        let mut n = 0;
        for &id in entity_ids {
            let Some(e) = d.get(id) else { continue };
            if e.attribute(CLASS_STATUS_ATTR) == Some(ClassificationStatus::Frozen.as_token()) {
                continue;
            }
            let wall_type = type_override
                .map(str::to_string)
                .or_else(|| e.attribute(WALL_TYPE_ATTR).map(str::to_string))
                .unwrap_or_default();
            if wall_type.is_empty() {
                continue;
            }
            let conf = e
                .attribute(CLASS_CONF_ATTR)
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0);
            let source = if type_override.is_some() {
                "user"
            } else {
                "ai"
            };
            if write_face_attrs(
                d,
                id,
                &wall_type,
                ClassificationStatus::Accepted,
                conf,
                source,
                true,
            ) {
                n += 1;
            }
        }
        n
    })
}

/// Reject classifications on the given face entity ids (status → `rejected`).
pub fn reject_classifications(doc: &mut Document, entity_ids: &[aadt_model::EntityId]) -> usize {
    doc.transact("AI-WALL-REJECT", |d| {
        let mut n = 0;
        for &id in entity_ids {
            let Some(e) = d.get(id) else { continue };
            if e.attribute(CLASS_STATUS_ATTR) == Some(ClassificationStatus::Frozen.as_token()) {
                continue;
            }
            let wall_type = e.attribute(WALL_TYPE_ATTR).unwrap_or("").to_string();
            let conf = e
                .attribute(CLASS_CONF_ATTR)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            if write_face_attrs(
                d,
                id,
                &wall_type,
                ClassificationStatus::Rejected,
                conf,
                "user",
                true,
            ) {
                n += 1;
            }
        }
        n
    })
}

/// Freeze classifications. If `entity_ids` is empty, freezes all `accepted` entities.
pub fn freeze_classifications(doc: &mut Document, entity_ids: &[aadt_model::EntityId]) -> usize {
    let targets: Vec<aadt_model::EntityId> = if entity_ids.is_empty() {
        doc.entities()
            .into_iter()
            .filter(|e| {
                e.attribute(CLASS_STATUS_ATTR) == Some(ClassificationStatus::Accepted.as_token())
            })
            .map(|e| e.id)
            .collect()
    } else {
        entity_ids.to_vec()
    };
    doc.transact("AI-WALL-FREEZE", |d| {
        let mut n = 0;
        for id in targets {
            let Some(e) = d.get(id) else { continue };
            let status = e
                .attribute(CLASS_STATUS_ATTR)
                .and_then(ClassificationStatus::parse);
            if !matches!(
                status,
                Some(ClassificationStatus::Accepted | ClassificationStatus::Proposed)
            ) {
                // Allow freezing proposed too if explicitly selected; skip rejected/frozen.
                if entity_ids.is_empty() {
                    continue;
                }
                if status == Some(ClassificationStatus::Frozen)
                    || status == Some(ClassificationStatus::Rejected)
                {
                    continue;
                }
            }
            let wall_type = e.attribute(WALL_TYPE_ATTR).unwrap_or("").to_string();
            if wall_type.is_empty() {
                continue;
            }
            let conf = e
                .attribute(CLASS_CONF_ATTR)
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0);
            let source = e.attribute(CLASS_SOURCE_ATTR).unwrap_or("ai").to_string();
            if write_face_attrs(
                d,
                id,
                &wall_type,
                ClassificationStatus::Frozen,
                conf,
                &source,
                true,
            ) {
                n += 1;
            }
        }
        n
    })
}

/// List entities needing review (`proposed`, or any with conf below threshold),
/// sorted by ascending confidence then entity id.
pub fn list_for_review(doc: &Document, conf_threshold: f64) -> Vec<ClassificationReviewItem> {
    let mut items = Vec::new();
    for e in doc.entities() {
        let Some(status) = e
            .attribute(CLASS_STATUS_ATTR)
            .and_then(ClassificationStatus::parse)
        else {
            continue;
        };
        let conf = e
            .attribute(CLASS_CONF_ATTR)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        let needs = status == ClassificationStatus::Proposed || conf < conf_threshold;
        if !needs {
            continue;
        }
        if status == ClassificationStatus::Frozen || status == ClassificationStatus::Rejected {
            continue;
        }
        items.push(ClassificationReviewItem {
            entity_id: e.id,
            wall_type: e.attribute(WALL_TYPE_ATTR).unwrap_or("").to_string(),
            status,
            confidence: conf,
            source: e.attribute(CLASS_SOURCE_ATTR).unwrap_or("").to_string(),
            revision: read_rev(e),
        });
    }
    items.sort_by(|a, b| {
        a.confidence
            .partial_cmp(&b.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.entity_id.0.cmp(&b.entity_id.0))
    });
    items
}

/// Count entities by classification status (for Recognize summary).
pub fn status_counts(doc: &Document) -> [(ClassificationStatus, usize); 4] {
    let mut proposed = 0;
    let mut accepted = 0;
    let mut frozen = 0;
    let mut rejected = 0;
    for e in doc.entities() {
        match e
            .attribute(CLASS_STATUS_ATTR)
            .and_then(ClassificationStatus::parse)
        {
            Some(ClassificationStatus::Proposed) => proposed += 1,
            Some(ClassificationStatus::Accepted) => accepted += 1,
            Some(ClassificationStatus::Frozen) => frozen += 1,
            Some(ClassificationStatus::Rejected) => rejected += 1,
            None => {}
        }
    }
    [
        (ClassificationStatus::Proposed, proposed),
        (ClassificationStatus::Accepted, accepted),
        (ClassificationStatus::Frozen, frozen),
        (ClassificationStatus::Rejected, rejected),
    ]
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
pub(crate) fn json_string(s: &str) -> String {
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
pub(crate) fn string_field(object: &str, key: &str) -> Option<String> {
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

/// Extract a floating field `"key":<float>` from a small JSON object substring.
fn float_field(object: &str, key: &str) -> Option<f64> {
    let needle = format!("\"{key}\"");
    let after_key = &object[object.find(&needle)? + needle.len()..];
    let colon = after_key.find(':')?;
    let digits: String = after_key[colon + 1..]
        .trim_start()
        .chars()
        .take_while(|c| {
            c.is_ascii_digit() || *c == '-' || *c == '.' || *c == 'e' || *c == 'E' || *c == '+'
        })
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
                wall_type: WallType::Exterior,
                confidence: 0.0,
            }
        );
        assert_eq!(got[1].wall_type, WallType::FireRated);
    }

    #[test]
    fn parses_confidence_when_present() {
        let text = "{\"assignments\":[{\"id\":0,\"wall_type\":\"exterior\",\"confidence\":0.99}]}";
        let got = parse_wall_type_response(text).unwrap();
        assert!((got[0].confidence - 0.99).abs() < 1e-9);
    }

    #[test]
    fn apply_wall_types_annotates_faces_as_one_undoable_pass() {
        let mut doc = one_wall_doc(); // two LINE faces, ids 0 and 1
        let det = detect(&doc, &DetectionConfig::default());
        assert_eq!(det.walls.len(), 1);
        let assignments = vec![WallTypeAssignment {
            id: 0,
            wall_type: WallType::Exterior,
            confidence: 0.99,
        }];

        let n = apply_wall_types(&mut doc, &det, &assignments);
        assert_eq!(n, 1, "one wall classified");

        // Both of the wall's face entities carry the inferred type.
        for id in [EntityId(0), EntityId(1)] {
            assert_eq!(
                doc.get(id).unwrap().attribute(WALL_TYPE_ATTR),
                Some("exterior")
            );
            assert_eq!(
                doc.get(id).unwrap().attribute(CLASS_STATUS_ATTR),
                Some("accepted") // 0.99 ≥ 0.98
            );
        }

        // The whole AI pass is one undoable unit.
        assert_eq!(doc.undo_label(), Some("AI-WALL-PROPOSE"));
        doc.undo();
        for id in [EntityId(0), EntityId(1)] {
            assert_eq!(doc.get(id).unwrap().attribute(WALL_TYPE_ATTR), None);
        }
    }

    #[test]
    fn low_confidence_stays_proposed() {
        let mut doc = one_wall_doc();
        let det = detect(&doc, &DetectionConfig::default());
        let n = propose_wall_types(
            &mut doc,
            &det,
            &[WallTypeAssignment {
                id: 0,
                wall_type: WallType::Partition,
                confidence: 0.85,
            }],
            DEFAULT_AUTO_ACCEPT_THRESHOLD,
        );
        assert_eq!(n, 1);
        assert_eq!(
            doc.get(EntityId(0)).unwrap().attribute(CLASS_STATUS_ATTR),
            Some("proposed")
        );
    }

    #[test]
    fn freeze_blocks_repropose() {
        let mut doc = one_wall_doc();
        let det = detect(&doc, &DetectionConfig::default());
        propose_wall_types(
            &mut doc,
            &det,
            &[WallTypeAssignment {
                id: 0,
                wall_type: WallType::Exterior,
                confidence: 0.99,
            }],
            DEFAULT_AUTO_ACCEPT_THRESHOLD,
        );
        let frozen = freeze_classifications(&mut doc, &[]);
        assert!(frozen >= 1);
        let n = propose_wall_types(
            &mut doc,
            &det,
            &[WallTypeAssignment {
                id: 0,
                wall_type: WallType::Shaft,
                confidence: 0.99,
            }],
            DEFAULT_AUTO_ACCEPT_THRESHOLD,
        );
        assert_eq!(n, 0);
        assert_eq!(
            doc.get(EntityId(0)).unwrap().attribute(WALL_TYPE_ATTR),
            Some("exterior")
        );
    }

    #[test]
    fn accept_override_sets_user_source() {
        let mut doc = one_wall_doc();
        let det = detect(&doc, &DetectionConfig::default());
        propose_wall_types(
            &mut doc,
            &det,
            &[WallTypeAssignment {
                id: 0,
                wall_type: WallType::Exterior,
                confidence: 0.85,
            }],
            DEFAULT_AUTO_ACCEPT_THRESHOLD,
        );
        let n = accept_classifications(&mut doc, &[EntityId(0)], Some("partition"));
        assert_eq!(n, 1);
        assert_eq!(
            doc.get(EntityId(0)).unwrap().attribute(WALL_TYPE_ATTR),
            Some("partition")
        );
        assert_eq!(
            doc.get(EntityId(0)).unwrap().attribute(CLASS_SOURCE_ATTR),
            Some("user")
        );
        assert_eq!(
            doc.get(EntityId(0)).unwrap().attribute(CLASS_STATUS_ATTR),
            Some("accepted")
        );
    }

    #[test]
    fn re_applying_the_same_types_changes_nothing() {
        let mut doc = one_wall_doc();
        let det = detect(&doc, &DetectionConfig::default());
        let assignments = vec![WallTypeAssignment {
            id: 0,
            wall_type: WallType::Partition,
            confidence: 0.99,
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
        assert_eq!(audit.entries().len(), 1, "the payload was audited");
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
                wall_type: WallType::Partition,
                confidence: 0.0,
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
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, 0);
        assert_eq!(got[0].wall_type, WallType::Exterior);
        assert!(
            got[0].confidence > 0.0,
            "heuristic fills missing confidence"
        );
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
