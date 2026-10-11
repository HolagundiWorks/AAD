//! AUDIT and RECOVER (File ▸ Drawing Utilities): check a drawing for errors and fix them, and open
//! a damaged file with the fixes applied. The checks themselves live in [`crate::audit`].

use std::sync::Arc;

use serde_json::{Value, json};

use super::*;
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("audit", "Audit", run_audit)
            .menu(&["File", "Drawing Utilities", "Audit"])
            .params("{fix?: bool} → {errors, fixed, issues: [{kind, location, handle?, message, fix}], message}")
            .interactive(|_| Ok(Box::new(Audit))),
    ]
}

/// Check the current drawing; with `fix`, repair it (one undo step).
fn run_audit(s: &mut Session, p: &Value) -> Result<Value> {
    let fix = bool_or(p, "fix", false);
    let mut copy = (*s.doc()?).clone();
    let issues = crate::audit::repair(&mut copy);
    // A drawing without errors stays the same object: no undo step, no unsaved changes.
    if fix && !issues.is_empty() {
        s.state_mut()?.doc = Arc::new(copy);
    }
    Ok(crate::audit::Report { issues, fixed: fix }.to_json())
}

#[cfg(target_arch = "wasm32")]
fn read_path(cmd: &str, _path: &str) -> Result<Vec<u8>> {
    Err(bad(cmd, "paths are not available on the web; pass `data`"))
}

const YES_NO: Accept = Accept { point: false, number: false, text: false, select: false, enter: true };

fn echo_result(s: &mut Session, r: &Value) {
    if let Some(m) = r.get("message").and_then(Value::as_str) {
        for l in m.lines() {
            s.echo(l.to_string());
        }
    }
}

/// AUDIT at the command line: "Fix any errors detected? [Yes/No] <N>:".
struct Audit;

impl Interactive for Audit {
    fn name(&self) -> &'static str {
        "AUDIT"
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Fix any errors detected?", YES_NO).kw(&["Yes", "No"]).default("N")
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let fix = match i {
            Input::Cancel => return Ok(Step::Cancel),
            Input::Enter => false,
            Input::Keyword(k) | Input::Text(k) => match self.prompt(s).match_keyword(&k).as_deref() {
                Some("Yes") => true,
                Some(_) => false,
                None => return Err(bad("audit", "answer Yes or No")),
            },
            _ => return Ok(Step::Continue),
        };
        let r = run_audit(s, &json!({ "fix": fix }))?;
        echo_result(s, &r);
        Ok(Step::Done)
    }
}
