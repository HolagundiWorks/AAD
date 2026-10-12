//! The read-only side of the Project Setup Wizard: check a project configuration and show what it
//! would create, before anything is written. The wizard review step, scripts and the assistant use
//! the same commands. Generating the files is a later stage (docs/PROJECT_WIZARD.md).

use serde_json::{Value, json};

use super::*;
use crate::projectplan::{ProjectConfig, company::CompanyProfile, plan, preflight};
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("project.plan", "Plan Project", run_plan)
            .params("{config: project configuration, root?: folder} → {valid, issues: [{path, message}], plan?: {folders, drawings, xrefs, files, elevations}, preflight?} (writes nothing; with root, reports which planned files already exist)")
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
