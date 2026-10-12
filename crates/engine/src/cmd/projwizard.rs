//! The read-only side of the Project Setup Wizard: check a project configuration and show what it
//! would create, before anything is written. The wizard review step, scripts and the assistant use
//! the same commands. Generating the files is a later stage (docs/PROJECT_WIZARD.md).

use serde_json::{Value, json};

use super::*;
use crate::projectplan::{GenerateOptions, ProjectConfig, cleanup, company::CompanyProfile, generate, inspect_recovery, plan, preflight};
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("project.plan", "Plan Project", run_plan)
            .params("{config: project configuration, root?: folder} → {valid, issues: [{path, message}], plan?: {folders, drawings, xrefs, files, elevations}, preflight?} (writes nothing; with root, reports which planned files already exist)")
            .noundo(),
        CommandSpec::new("project.generate", "Generate Project", run_generate)
            .params("{config?: project configuration (saved one when resume), root: folder, resume?: bool, allowNonEmpty?: bool} → {status, created, kept, xrefs_attached, xrefs_recorded, issues} (creates the folders, one drawing per planned drawing and the Xrefs; never overwrites an existing file; ends Ready only when every file opens and every attached Xref resolves)"),
        CommandSpec::new("project.recover", "Recover Project Generation", run_recover)
            .params("{root: folder, action?: inspect|cleanup} → inspect: {recovery}; cleanup: {removed, left} (removes only files the generation log recorded and that are unchanged)")
            .noundo(),
        CommandSpec::new("company.validate", "Check Company Profile", run_company)
            .params("{profile: company profile} → {valid, issues: [{path, message}], nameplate: [lines]}")
            .noundo(),
    ]
}

fn run_plan(_s: &mut Session, p: &Value) -> Result<Value> {
    let raw = p.get("config").ok_or_else(|| bad("project.plan", "`config` is required"))?;
    let cfg = ProjectConfig::from_json(&raw.to_string()).map_err(|e| bad("project.plan", e))?;
    match plan(&cfg) {
        Err(issues) => Ok(json!({"valid": false, "issues": issues})),
        Ok(pl) => {
            let pre = p.get("root").and_then(Value::as_str).map(|r| preflight(std::path::Path::new(r), &pl));
            Ok(json!({"valid": true, "issues": [], "plan": pl, "preflight": pre}))
        }
    }
}

fn run_company(_s: &mut Session, p: &Value) -> Result<Value> {
    let raw = p.get("profile").ok_or_else(|| bad("company.validate", "`profile` is required"))?;
    let profile: CompanyProfile =
        serde_json::from_value(raw.clone()).map_err(|e| bad("company.validate", format!("the profile cannot be read: {e}")))?;
    let issues = profile.validate();
    Ok(json!({"valid": issues.is_empty(), "issues": issues, "nameplate": profile.nameplate_lines()}))
}

fn run_generate(s: &mut Session, p: &Value) -> Result<Value> {
    let root = std::path::PathBuf::from(
        p.get("root").and_then(Value::as_str).ok_or_else(|| bad("project.generate", "`root` (the project folder) is required"))?,
    );
    let resume = p.get("resume").and_then(Value::as_bool).unwrap_or(false);
    let cfg = match p.get("config") {
        Some(raw) => ProjectConfig::from_json(&raw.to_string()).map_err(|e| bad("project.generate", e))?,
        None if resume => {
            let text = std::fs::read_to_string(root.join("project.json"))
                .map_err(|e| bad("project.generate", format!("cannot read the saved configuration: {e}")))?;
            ProjectConfig::from_json(&text).map_err(|e| bad("project.generate", e))?
        }
        None => return Err(bad("project.generate", "`config` is required")),
    };
    let opts = GenerateOptions { resume, allow_non_empty: p.get("allowNonEmpty").and_then(Value::as_bool).unwrap_or(false) };
    let report = generate(s, &root, cfg, opts);
    Ok(json!(report))
}

fn run_recover(_s: &mut Session, p: &Value) -> Result<Value> {
    let root = std::path::PathBuf::from(p.get("root").and_then(Value::as_str).ok_or_else(|| bad("project.recover", "`root` is required"))?);
    match p.get("action").and_then(Value::as_str).unwrap_or("inspect") {
        "inspect" => Ok(json!({"recovery": inspect_recovery(&root)})),
        "cleanup" => {
            let (removed, left) = cleanup(&root).map_err(|e| bad("project.recover", e))?;
            Ok(json!({"removed": removed, "left": left}))
        }
        other => Err(bad("project.recover", format!("unknown action `{other}`: use inspect or cleanup (resume is project.generate with resume)"))),
    }
}
