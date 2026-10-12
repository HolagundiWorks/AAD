//! The project configuration behind the Project Setup Wizard: one typed, versioned manifest that
//! says what a project is (floors, drawing sets, naming, Xref rules, schedules, the company it is
//! made for), and a pure planner that turns it into the list of folders, drawings and Xrefs to
//! create. Nothing here touches a drawing; only [`company`] (profile files) and [`preflight`]
//! (read-only checks) touch the disk. The generating services (still to be built) read a [`Plan`]
//! and do the work, so the UI never writes files itself. See docs/PROJECT_WIZARD.md.
//!
//! The configuration is the intended project; the DWG/DXF files on disk are its current state, and
//! they are validated against it, never assumed to match. Identities are stable ids, never file
//! names or display names.

pub mod company;
pub mod compat;
mod generate;
mod plan;

pub use generate::{GenerateOptions, Report, apply_typography, cleanup, generate, inspect_recovery};
pub use plan::{Plan, PlannedDrawing, PlannedXref, Preflight, plan, preflight, relative_path, resolve_elevations};

use serde::{Deserialize, Serialize};

/// The manifest schema this build writes. Older files are migrated by [`ProjectConfig::from_json`].
pub const SCHEMA_VERSION: u32 = 1;

/// Largest manifest read (hostile or corrupt files).
pub const MAX_MANIFEST_BYTES: usize = 5_000_000;
pub const MAX_FLOORS: usize = 60;
pub const MAX_SETS: usize = 32;
pub const MAX_DRAWINGS: usize = 5_000;
pub const MAX_RULES: usize = 200;

/// One problem found in a configuration or a profile: where, and what.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

impl Issue {
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Issue { path: path.into(), message: message.into() }
    }
}

/// A new stable identifier: `prefix-` and 32 hex digits. Random enough to never collide in practice
/// (process-random hasher state, the clock and a counter), and needing no network or extra crate.
pub fn new_id(prefix: &str) -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let half = |salt: u64| {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(nanos);
        h.write_u64(n);
        h.write_u64(salt);
        h.finish()
    };
    format!("{prefix}-{:016x}{:016x}", half(1), half(2))
}

/// Whether `s` is usable inside an id, a file name and a path (letters, digits, `-` and `_`).
pub fn is_code(s: &str) -> bool {
    !s.is_empty() && s.len() <= 48 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Address {
    pub line1: String,
    pub line2: String,
    pub locality: String,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub country: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Client {
    pub name: String,
    pub contact: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Units {
    /// `mm` or `m`: the unit every elevation and height in this file is stored in.
    pub length: String,
    pub area: String,
}

impl Default for Units {
    fn default() -> Self {
        Units { length: "mm".into(), area: "m2".into() }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectInfo {
    pub id: String,
    pub code: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub description: String,
    pub address: Address,
    pub client: Client,
    pub units: Units,
    /// The template this configuration was copied from, and its version (a snapshot, not a link).
    pub template_id: Option<String>,
    pub template_version: Option<u32>,
    /// The company profile (resolved through the profile store, never by name).
    pub company_profile_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FloorKind {
    Basement,
    Stilt,
    #[default]
    Floor,
    Podium,
    Terrace,
    Roof,
}

/// A floor. Heights and elevations are `Option`: a missing value is reported, never replaced by a
/// default. Elevations are relative to the project datum, in `units.length`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Floor {
    pub id: String,
    pub name: String,
    /// Short code used in file names and drawing numbers (`GROUND`, `FF`).
    pub code: String,
    pub kind: FloorKind,
    pub sequence: u32,
    pub elevation: Option<f64>,
    pub floor_to_floor: Option<f64>,
    pub clear_height: Option<f64>,
    pub slab_thickness: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FloorPolicy {
    /// One drawing per floor (the default for plans).
    #[default]
    PerFloor,
    /// One drawing for the whole building (site plan, schedules).
    Single,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrawingSet {
    pub id: String,
    /// Short code (`AR`, `ST`, `EL`).
    pub code: String,
    pub name: String,
    pub discipline: String,
    pub enabled: bool,
    pub floor_policy: FloorPolicy,
    /// Folder under `drawings/` (the set name when empty).
    pub folder: String,
    /// Whether the set also gets a model drawing the floor drawings can reference.
    pub has_model: bool,
}

/// One cell of the floor-by-set matrix. A cell that is absent means "create" for an enabled
/// per-floor set; an explicit `create: false` leaves that floor out of that set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatrixCell {
    pub floor_id: String,
    pub set_id: String,
    pub create: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawingKind {
    Master,
    Model,
    #[default]
    Floor,
}

/// Which drawings a rule means: of a kind, of a set (by code; empty for the master), and for a floor
/// drawing optionally only the one on the same floor as the host.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Selector {
    pub kind: DrawingKind,
    pub set: String,
    pub same_floor: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attachment {
    Overlay,
    #[default]
    Attach,
}

/// "Every drawing matching `host` references every drawing matching `target`." The planner expands
/// rules into explicit edges; Xrefs are never inferred from file names.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct XrefRule {
    pub host: Selector,
    pub target: Selector,
    pub attachment: Attachment,
}

/// A schedule the project starts with (an opening schedule template and its opening types).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScheduleDef {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub template_id: Option<String>,
}

/// File and drawing-number patterns: `{project}`, `{set}`, `{floor}` and `{seq}` / `{seq:03}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NamingRules {
    pub drawing_pattern: String,
    pub model_pattern: String,
    pub master_pattern: String,
    pub number_pattern: String,
    /// `dxf` (what AAD writes) or `dwg`.
    pub file_extension: String,
}

impl Default for NamingRules {
    fn default() -> Self {
        NamingRules {
            drawing_pattern: "{project}_{set}_{floor}".into(),
            model_pattern: "{project}_{set}_MODEL".into(),
            master_pattern: "{project}_MASTER".into(),
            number_pattern: "{project}-{set}-{seq:03}".into(),
            file_extension: "dxf".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Draft,
    Validating,
    Generating,
    LinkingXrefs,
    Verifying,
    Ready,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Generation {
    pub status: Status,
    pub manifest_revision: u32,
}

impl Default for Generation {
    fn default() -> Self {
        Generation { status: Status::Draft, manifest_revision: 1 }
    }
}

/// A project's typography: the font preset and any per-role changes. A project copies it when it
/// is created, so changing the application default later does not change an existing project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TypographyConfig {
    pub preset: String,
    pub overrides: Vec<RoleOverride>,
}

impl Default for TypographyConfig {
    fn default() -> Self {
        TypographyConfig { preset: aad_fonts::registry::DEFAULT_PRESET.into(), overrides: Vec::new() }
    }
}

/// One role changed from its default face and weight (`face`: sans, condensed or mono).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoleOverride {
    pub role: String,
    pub face: String,
    pub weight: u16,
}

impl TypographyConfig {
    pub fn issues(&self) -> Vec<Issue> {
        use aad_fonts::registry::{Role, preset};
        let mut v = Vec::new();
        if preset(&self.preset).is_none() {
            v.push(Issue::new("typography.preset", format!("`{}` is not a font preset", self.preset)));
        }
        for (i, o) in self.overrides.iter().enumerate() {
            if Role::from_id(&o.role).is_none() {
                v.push(Issue::new(format!("typography.overrides[{i}].role"), format!("`{}` is not a font role", o.role)));
            }
            if !matches!(o.face.as_str(), "sans" | "condensed" | "mono") {
                v.push(Issue::new(format!("typography.overrides[{i}].face"), "must be sans, condensed or mono"));
            }
            if !(400..=700).contains(&o.weight) {
                v.push(Issue::new(
                    format!("typography.overrides[{i}].weight"),
                    "must be between 400 and 700 (nothing lighter than Regular for small CAD text)",
                ));
            }
        }
        v
    }

    /// The registry form (invalid overrides are left out: [`TypographyConfig::issues`] reports them).
    pub fn to_registry(&self) -> aad_fonts::registry::Typography {
        use aad_fonts::registry::{Face, Role, Typography};
        let overrides = self
            .overrides
            .iter()
            .filter_map(|o| {
                let face = match o.face.as_str() {
                    "sans" => Face::Sans,
                    "condensed" => Face::Condensed,
                    "mono" => Face::Mono,
                    _ => return None,
                };
                Some((Role::from_id(&o.role)?, face, o.weight.clamp(400, 700)))
            })
            .collect();
        Typography { preset: self.preset.clone(), overrides }
    }
}

/// The recorded outcome of generation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Generated {
    pub drawings: Vec<PlannedDrawing>,
    pub xrefs: Vec<PlannedXref>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectConfig {
    pub schema_version: u32,
    pub project: ProjectInfo,
    pub floors: Vec<Floor>,
    pub drawing_sets: Vec<DrawingSet>,
    pub matrix: Vec<MatrixCell>,
    pub xref_rules: Vec<XrefRule>,
    pub schedules: Vec<ScheduleDef>,
    pub naming_rules: NamingRules,
    /// Folders the template creates under the project root (a default set when empty).
    pub folders: Vec<String>,
    /// Whether a master (coordination) drawing is created.
    pub master_drawing: bool,
    /// The font preset and role changes new drawings are created with.
    pub typography: TypographyConfig,
    /// What generation actually produced (kept apart from the intent above): the drawings and the
    /// Xrefs, with the ids the files are known by.
    pub generated: Generated,
    pub generation: Generation,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        ProjectConfig {
            schema_version: SCHEMA_VERSION,
            project: ProjectInfo::default(),
            floors: Vec::new(),
            drawing_sets: Vec::new(),
            matrix: Vec::new(),
            xref_rules: Vec::new(),
            schedules: Vec::new(),
            naming_rules: NamingRules::default(),
            folders: Vec::new(),
            master_drawing: true,
            typography: TypographyConfig::default(),
            generated: Generated::default(),
            generation: Generation::default(),
        }
    }
}

impl ProjectConfig {
    /// Read a manifest. A malformed, oversized or newer-than-supported file is an error to show, not a
    /// crash; older versions are migrated here (there is only version 1 so far).
    pub fn from_json(text: &str) -> Result<ProjectConfig, String> {
        if text.len() > MAX_MANIFEST_BYTES {
            return Err("the project manifest is too large to read".into());
        }
        let cfg: ProjectConfig = serde_json::from_str(text).map_err(|e| format!("the project manifest cannot be read: {e}"))?;
        if cfg.schema_version == 0 || cfg.schema_version > SCHEMA_VERSION {
            return Err(format!(
                "the project manifest is schema version {}, this build reads up to {SCHEMA_VERSION}: update the application",
                cfg.schema_version
            ));
        }
        Ok(cfg)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// The floors in sequence order.
    pub fn floors_in_order(&self) -> Vec<&Floor> {
        let mut v: Vec<&Floor> = self.floors.iter().collect();
        v.sort_by_key(|f| f.sequence);
        v
    }

    /// Everything wrong with the configuration (empty when it can be planned).
    pub fn validate(&self) -> Vec<Issue> {
        plan::validate(self)
    }
}

#[cfg(test)]
mod tests;
