//! Read-only AORMS Connect `session.json` (licence SSO).
//! Activate stays in AORMS Connect — AADT never writes tokens.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LicenceStatus {
    pub bound: bool,
    pub path: PathBuf,
    pub hub_url: Option<String>,
    pub note: String,
}

pub fn default_session_path() -> PathBuf {
    let base = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("AORMS-Connect").join("session.json")
}

/// Strip `--connect-session <path>` from args; return (cleaned_args, resolved_path).
pub fn take_connect_session_arg(args: &[String]) -> (Vec<String>, PathBuf) {
    let mut out = Vec::with_capacity(args.len());
    let mut path = default_session_path();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a.eq_ignore_ascii_case("--connect-session") {
            if let Some(p) = it.next() {
                let trimmed = p.trim().trim_matches('"');
                if !trimmed.is_empty() {
                    path = PathBuf::from(trimmed);
                }
            }
            continue;
        }
        out.push(a.clone());
    }
    (out, path)
}

pub fn try_read_session(path: &Path) -> LicenceStatus {
    let mut status = LicenceStatus {
        bound: false,
        path: path.to_path_buf(),
        hub_url: None,
        note: String::new(),
    };
    let Ok(json) = fs::read_to_string(path) else {
        status.note = "No Connect session.json — Activate in AORMS Connect".into();
        return status;
    };
    let Some(sync) = json_string_field(&json, "syncToken") else {
        status.note = "session.json missing syncToken".into();
        return status;
    };
    if sync.is_empty() {
        status.note = "session.json missing syncToken".into();
        return status;
    }
    if let Some(exp) = json_string_field(&json, "expiresAt") {
        if !exp.is_empty() {
            // Best-effort: ISO-8601 UTC lexicographic compare works for Z forms.
            let now = chrono_like_now();
            if !now.is_empty() && exp < now {
                status.note = "Connect session expired — Activate again in Connect".into();
                return status;
            }
        }
    }
    status.bound = true;
    status.hub_url = json_string_field(&json, "hubUrl");
    status.note = "Licence from AORMS Connect".into();
    status
}

fn json_string_field(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = json.find(&needle)?;
    let after = &json[start + needle.len()..];
    let colon = after.find(':')?;
    let rest = &after[colon + 1..];
    let q0 = rest.find('"')?;
    let rest = &rest[q0 + 1..];
    let q1 = rest.find('"')?;
    Some(rest[..q1].to_string())
}

/// Rough UTC `YYYY-MM-DDTHH:MM:SS` for expiry compare (no chrono dep).
fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let Ok(dur) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return String::new();
    };
    let secs = dur.as_secs() as i64;
    // Civil date via days since epoch (proleptic Gregorian).
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400) as u32;
    let (y, m, d) = civil_from_days(days);
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    // Howard Hinnant algorithm
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

pub fn status_line(s: &LicenceStatus) -> String {
    if s.bound {
        match &s.hub_url {
            Some(hub) if !hub.is_empty() => format!("Licensed · Connect · {hub}"),
            _ => "Licensed · Connect".into(),
        }
    } else {
        "Unbound · Connect".into()
    }
}
