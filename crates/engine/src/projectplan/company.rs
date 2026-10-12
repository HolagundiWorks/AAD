//! Company profiles: the firm identity reused by every project (name, address, contacts, the
//! architect, the logo, the default nameplate). A profile is kept in the application data folder as
//! `companies/<id>/company.json` with its logo under `assets/`; a project stores only the profile id.
//! Optional fields may be empty and are left out of the nameplate rather than printed as blanks.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Address, Issue, is_code};

pub const SCHEMA_VERSION: u32 = 1;
const MAX_PROFILE_BYTES: u64 = 1_000_000;
const MAX_LOGO_BYTES: u64 = 5_000_000;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Contact {
    pub phone: String,
    pub email: String,
    pub website: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Architect {
    pub name: String,
    pub designation: String,
    pub registration_number: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Logo {
    /// Relative to the profile folder (`assets/logo.png`).
    pub relative_path: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompanyProfile {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub address: Address,
    pub contact: Contact,
    pub architect: Architect,
    pub logo: Logo,
    pub default_nameplate_template_id: Option<String>,
}

impl Default for CompanyProfile {
    fn default() -> Self {
        CompanyProfile {
            schema_version: SCHEMA_VERSION,
            id: String::new(),
            name: String::new(),
            address: Address::default(),
            contact: Contact::default(),
            architect: Architect::default(),
            logo: Logo::default(),
            default_nameplate_template_id: None,
        }
    }
}

impl CompanyProfile {
    /// A new, empty profile with a fresh stable id.
    pub fn new(name: &str) -> Self {
        CompanyProfile { id: super::new_id("company"), name: name.trim().to_string(), ..CompanyProfile::default() }
    }

    pub fn validate(&self) -> Vec<Issue> {
        let mut v = Vec::new();
        if self.schema_version == 0 || self.schema_version > SCHEMA_VERSION {
            v.push(Issue::new("schema_version", format!("{} is not supported (this build reads up to {SCHEMA_VERSION})", self.schema_version)));
        }
        if !is_code(&self.id) {
            v.push(Issue::new("id", "is missing or has characters other than letters, digits, - and _"));
        }
        if self.name.trim().is_empty() {
            v.push(Issue::new("name", "is required"));
        }
        let email = self.contact.email.trim();
        if !email.is_empty() && !(email.contains('@') && !email.starts_with('@') && !email.ends_with('@') && !email.contains(' ')) {
            v.push(Issue::new("contact.email", "does not look like an email address"));
        }
        let logo = &self.logo.relative_path;
        if !logo.is_empty() && !super::plan::safe_rel(logo) {
            v.push(Issue::new("logo.relative_path", "must be a plain relative path inside the profile folder"));
        }
        v
    }

    /// The nameplate lines for the company block, in order, with empty fields left out.
    pub fn nameplate_lines(&self) -> Vec<String> {
        let a = &self.address;
        let place = [a.city.as_str(), a.state.as_str(), a.postal_code.as_str()]
            .iter()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim())
            .collect::<Vec<_>>()
            .join(", ");
        let street = [a.line1.as_str(), a.line2.as_str()].iter().filter(|s| !s.trim().is_empty()).map(|s| s.trim()).collect::<Vec<_>>().join(", ");
        let reach = [self.contact.phone.as_str(), self.contact.email.as_str(), self.contact.website.as_str()]
            .iter()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim())
            .collect::<Vec<_>>()
            .join("  ·  ");
        let who = [self.architect.name.as_str(), self.architect.designation.as_str(), self.architect.registration_number.as_str()]
            .iter()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim())
            .collect::<Vec<_>>()
            .join(" · ");
        [self.name.trim().to_string(), street, place, reach, who].into_iter().filter(|s| !s.is_empty()).collect()
    }
}

/// The folder of a profile under the application data folder `base`.
pub fn profile_dir(base: &Path, id: &str) -> Result<PathBuf, String> {
    if !is_code(id) {
        return Err(format!("`{id}` is not a valid company id"));
    }
    Ok(base.join("companies").join(id))
}

/// Write a file by writing a sibling temporary file and renaming it over the target, so a crash
/// leaves the old file or the new one, never half of either.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path.parent().ok_or("the path has no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let tmp = path.with_extension("tmp-write");
    std::fs::write(&tmp, bytes).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot replace {}: {e}", path.display())
    })
}

/// Save a profile (after validating it). Returns the profile folder.
pub fn save(base: &Path, profile: &CompanyProfile) -> Result<PathBuf, String> {
    let issues = profile.validate();
    if let Some(i) = issues.first() {
        return Err(format!("{}: {}", i.path, i.message));
    }
    let dir = profile_dir(base, &profile.id)?;
    let text = serde_json::to_string_pretty(profile).map_err(|e| e.to_string())?;
    write_atomic(&dir.join("company.json"), text.as_bytes())?;
    Ok(dir)
}

/// Read a profile. A missing, oversized, malformed or newer-schema file is an error to show.
pub fn load(base: &Path, id: &str) -> Result<CompanyProfile, String> {
    let file = profile_dir(base, id)?.join("company.json");
    let meta = std::fs::metadata(&file).map_err(|_| format!("the company profile `{id}` was not found (looked in {})", file.display()))?;
    if meta.len() > MAX_PROFILE_BYTES {
        return Err("the company profile file is too large to read".into());
    }
    let text = std::fs::read_to_string(&file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    let profile: CompanyProfile = serde_json::from_str(&text).map_err(|e| format!("the company profile cannot be read: {e}"))?;
    if profile.id != id {
        return Err(format!("the profile in {} has id `{}`, not `{id}`", file.display(), profile.id));
    }
    if let Some(i) = profile.validate().first() {
        return Err(format!("the company profile is not valid: {}: {}", i.path, i.message));
    }
    Ok(profile)
}

/// The ids and names of the profiles under `base` (unreadable ones are skipped).
pub fn list(base: &Path) -> Vec<(String, String)> {
    let Ok(rd) = std::fs::read_dir(base.join("companies")) else { return Vec::new() };
    let mut out: Vec<(String, String)> = rd
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter_map(|id| load(base, &id).ok().map(|p| (id, p.name)))
        .collect();
    out.sort_by_key(|a| a.1.to_lowercase());
    out
}

/// Copy a logo into the profile as its managed asset (`assets/logo.<ext>`) and record it. Only
/// PNG, JPEG and SVG files up to 5 MB are accepted; the file is checked by extension and size.
pub fn set_logo(base: &Path, profile: &mut CompanyProfile, source: &Path) -> Result<(), String> {
    let ext = source.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "svg") {
        return Err("the logo must be a PNG, JPEG or SVG file".into());
    }
    let size = std::fs::metadata(source).map_err(|e| format!("cannot read the logo: {e}"))?.len();
    if size == 0 || size > MAX_LOGO_BYTES {
        return Err("the logo must be between 1 byte and 5 MB".into());
    }
    let bytes = std::fs::read(source).map_err(|e| format!("cannot read the logo: {e}"))?;
    let rel = format!("assets/logo.{ext}");
    write_atomic(&profile_dir(base, &profile.id)?.join(&rel), &bytes)?;
    profile.logo.relative_path = rel;
    Ok(())
}

/// Problems with the profile's files on disk: a logo that is recorded but not there. The logo is
/// stored as a managed asset and a nameplate template may refer to it, but nothing draws it until the
/// drawing model has an image entity; the nameplate text does not depend on it.
pub fn validate_assets(base: &Path, profile: &CompanyProfile) -> Vec<Issue> {
    let mut v = Vec::new();
    let rel = &profile.logo.relative_path;
    if !rel.is_empty() {
        match profile_dir(base, &profile.id) {
            Ok(dir) if super::plan::safe_rel(rel) && dir.join(rel).is_file() => {}
            Ok(_) => v.push(Issue::new("logo.relative_path", format!("the logo `{rel}` is recorded but the file is not in the profile folder"))),
            Err(e) => v.push(Issue::new("id", e)),
        }
    }
    v
}
