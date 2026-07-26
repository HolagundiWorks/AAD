//! In-process control of the Microsoft Foundry Local CLI (`foundry`).
//!
//! Newer CLIs use `foundry server …`; older docs said `foundry service …`.
//! Helpers try `server` first, then `service`, so AADT works across installs.
//! Output prefers `-o json` when available.

use std::process::Command;
use std::time::Duration;

/// Snapshot from `foundry server status` / `foundry service status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundryServiceInfo {
    pub running: bool,
    pub state: String,
    /// First OpenAI-compatible base URL (`http://127.0.0.1:PORT`), if reported.
    pub endpoint: Option<String>,
    pub pid: Option<u32>,
}

/// One catalog / cache entry from `foundry model list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundryModelInfo {
    pub alias: String,
    pub cached: bool,
    pub device: String,
    pub file_size_mb: u64,
}

/// Whether the `foundry` executable is on `PATH`.
pub fn cli_available() -> bool {
    Command::new("foundry")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Discover the live OpenAI-compatible endpoint from the Foundry daemon.
pub fn discover_endpoint() -> Option<String> {
    server_status().ok().and_then(|s| s.endpoint)
}

/// Daemon status (running, URL, pid).
pub fn server_status() -> Result<FoundryServiceInfo, String> {
    let out = run_foundry_group(&["status"], true)?;
    parse_service_status(&out)
}

/// Start the Foundry daemon.
pub fn server_start() -> Result<String, String> {
    run_foundry_group(&["start"], false)
}

/// Stop the Foundry daemon.
pub fn server_stop() -> Result<String, String> {
    run_foundry_group(&["stop"], false)
}

/// Restart the Foundry daemon.
pub fn server_restart() -> Result<String, String> {
    run_foundry_group(&["restart"], false)
}

/// Load a cached model alias into the daemon.
pub fn model_load(alias: &str) -> Result<String, String> {
    if alias.trim().is_empty() {
        return Err("model alias is empty".into());
    }
    run_foundry(&["model", "load", alias.trim()], false)
}

/// Unload a model from the daemon.
pub fn model_unload(alias: &str) -> Result<String, String> {
    if alias.trim().is_empty() {
        return Err("model alias is empty".into());
    }
    run_foundry(&["model", "unload", alias.trim()], false)
}

/// Download a model into the local cache (may take a long time).
pub fn model_download(alias: &str) -> Result<String, String> {
    if alias.trim().is_empty() {
        return Err("model alias is empty".into());
    }
    run_foundry(&["model", "download", alias.trim()], false)
}

/// List catalog models (cached flag included when the CLI reports it).
pub fn model_list() -> Result<Vec<FoundryModelInfo>, String> {
    let out = run_foundry(&["model", "list"], true)?;
    Ok(parse_model_list(&out))
}

/// Aliases that are already downloaded (`cached: true`), preferred for Load.
pub fn list_cached_aliases() -> Result<Vec<String>, String> {
    let mut aliases: Vec<String> = model_list()?
        .into_iter()
        .filter(|m| m.cached)
        .map(|m| m.alias)
        .collect();
    aliases.sort();
    aliases.dedup();
    Ok(aliases)
}

/// Run `foundry server <args>` then fall back to `foundry service <args>`.
fn run_foundry_group(sub: &[&str], json: bool) -> Result<String, String> {
    let mut server_args = vec!["server"];
    server_args.extend_from_slice(sub);
    match run_foundry(&server_args, json) {
        Ok(o) => Ok(o),
        Err(e1) => {
            let mut service_args = vec!["service"];
            service_args.extend_from_slice(sub);
            run_foundry(&service_args, json).map_err(|e2| {
                format!("foundry server/service failed — server: {e1}; service: {e2}")
            })
        }
    }
}

fn run_foundry(args: &[&str], json: bool) -> Result<String, String> {
    let mut cmd = Command::new("foundry");
    cmd.args(args);
    if json {
        cmd.args(["-o", "json"]);
    }
    // Model load/download can be slow on first use.
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = cmd
        .output()
        .map_err(|e| format!("foundry not runnable ({e}). Is Foundry Local installed?"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        if !stdout.is_empty() {
            Ok(stdout)
        } else if !stderr.is_empty() {
            Ok(stderr)
        } else {
            Ok("ok".into())
        }
    } else {
        let msg = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("foundry exited {}", output.status)
        };
        Err(msg)
    }
}

fn parse_service_status(raw: &str) -> Result<FoundryServiceInfo, String> {
    // JSON: {"running":true,"state":"ready","pid":…,"webUrls":["http://…"]}
    if raw.trim_start().starts_with('{') {
        let running = json_bool(raw, "running").unwrap_or(false);
        let state = json_string_field(raw, "state").unwrap_or_else(|| {
            if running {
                "running".into()
            } else {
                "stopped".into()
            }
        });
        let pid = json_u32(raw, "pid");
        let endpoint = first_web_url(raw);
        return Ok(FoundryServiceInfo {
            running: running || endpoint.is_some() || state.eq_ignore_ascii_case("ready"),
            state,
            endpoint,
            pid,
        });
    }
    // Text fallback: look for http URL + running/ready keywords.
    let endpoint = raw
        .split_whitespace()
        .find(|t| t.starts_with("http://") || t.starts_with("https://"))
        .map(|s| s.trim_matches(|c| c == '"' || c == ',' || c == ')').to_string());
    let lower = raw.to_ascii_lowercase();
    let running = lower.contains("ready")
        || lower.contains("running")
        || endpoint.is_some();
    Ok(FoundryServiceInfo {
        running,
        state: if running {
            "ready".into()
        } else {
            "stopped".into()
        },
        endpoint,
        pid: None,
    })
}

fn parse_model_list(raw: &str) -> Vec<FoundryModelInfo> {
    let mut out = Vec::new();
    if raw.contains("\"alias\"") {
        // Walk each {"alias":…} object loosely.
        let mut rest = raw;
        while let Some(i) = rest.find("\"alias\"") {
            let chunk = &rest[i..];
            let end = chunk.find('}').unwrap_or(chunk.len().min(400));
            let obj = &chunk[..end];
            if let Some(alias) = json_string_field(obj, "alias") {
                let cached = json_bool(obj, "cached").unwrap_or(false);
                let device = json_string_field(obj, "device").unwrap_or_default();
                let file_size_mb = json_u64(obj, "fileSizeMb").unwrap_or(0);
                out.push(FoundryModelInfo {
                    alias,
                    cached,
                    device,
                    file_size_mb,
                });
            }
            rest = &rest[i + 7..];
        }
        return out;
    }
    // Text table: first column often the alias.
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('+') || t.starts_with('|') && t.contains("Model") {
            continue;
        }
        if let Some(name) = t.strip_prefix('|').and_then(|s| s.split('|').next()) {
            let alias = name.trim().to_string();
            if !alias.is_empty() && alias != "Model Name" {
                out.push(FoundryModelInfo {
                    alias,
                    cached: t.contains('●') || t.to_ascii_lowercase().contains("cached"),
                    device: String::new(),
                    file_size_mb: 0,
                });
            }
        }
    }
    out
}

fn first_web_url(raw: &str) -> Option<String> {
    if let Some(i) = raw.find("\"webUrls\"") {
        let after = &raw[i..];
        if let Some(http) = after.find("http://").or_else(|| after.find("https://")) {
            let s = &after[http..];
            let end = s
                .find(|c: char| c == '"' || c == ',' || c == ']' || c.is_whitespace())
                .unwrap_or(s.len());
            let url = s[..end].trim_end_matches('/').to_string();
            if !url.is_empty() {
                return Some(url);
            }
        }
    }
    raw.split_whitespace()
        .find(|t| t.starts_with("http://") || t.starts_with("https://"))
        .map(|s| s.trim_matches(|c| c == '"' || c == ',').trim_end_matches('/').to_string())
}

fn json_bool(text: &str, key: &str) -> Option<bool> {
    let needle = format!("\"{key}\"");
    let i = text.find(&needle)?;
    let rest = &text[i + needle.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    if after.starts_with("true") {
        Some(true)
    } else if after.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn json_string_field(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let i = text.find(&needle)?;
    let after = &text[i + needle.len()..];
    let colon = after.find(':')?;
    let rest = after[colon + 1..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    let mut out = String::new();
    let mut chars = rest[1..].chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            '"' => break,
            other => out.push(other),
        }
    }
    Some(out)
}

fn json_u32(text: &str, key: &str) -> Option<u32> {
    json_u64(text, key)?.try_into().ok()
}

fn json_u64(text: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\"");
    let i = text.find(&needle)?;
    let rest = &text[i + needle.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let end = after
        .find(|c: char| c == ',' || c == '}' || c == ']' || c.is_whitespace())
        .unwrap_or(after.len());
    after[..end].trim().parse().ok()
}

/// Wait briefly for the daemon to report an endpoint after start/restart.
pub fn wait_for_endpoint(timeout: Duration) -> Option<String> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if let Some(url) = discover_endpoint() {
            return Some(url);
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_server_status_json() {
        let raw = r#"{"running":true,"state":"ready","pid":28252,"webUrls":["http://127.0.0.1:55263"]}"#;
        let s = parse_service_status(raw).unwrap();
        assert!(s.running);
        assert_eq!(s.endpoint.as_deref(), Some("http://127.0.0.1:55263"));
        assert_eq!(s.pid, Some(28252));
    }

    #[test]
    fn parses_model_list_aliases() {
        let raw = r#"{"models":[{"alias":"phi-3.5-mini","cached":false,"device":"Cpu","fileSizeMb":2590},{"alias":"qwen2.5-coder-1.5b","cached":true,"device":"Cpu","fileSizeMb":1822}]}"#;
        let list = parse_model_list(raw);
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].alias, "qwen2.5-coder-1.5b");
        assert!(list[1].cached);
    }
}
