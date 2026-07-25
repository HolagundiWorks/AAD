//! Persisted user preferences (`prefs.json` under the platform config dir).

use std::fs;
use std::path::PathBuf;

/// Drafting / chrome preferences loaded at startup and saved on change / exit.
#[derive(Debug, Clone, PartialEq)]
pub struct UserPrefs {
    pub ortho: bool,
    pub osnap: bool,
    pub polar: bool,
    pub osnap_track: bool,
    /// Dynamic Input near-cursor box (F12 / DYNMODE).
    pub dyn_mode: bool,
    pub show_layer_mgr: bool,
    /// Left structure dock (Layers / Blocks).
    pub show_left_mgr: bool,
    pub show_lweights: bool,
    pub right_panel_width: f32,
    pub left_panel_width: f32,
    pub autosave_secs: u64,
    /// Takeoff publish sink: `none` | `csv:<dir>` | `json:<dir>` | `postgres:<file.sql>`.
    pub takeoff_sink: String,
}

impl Default for UserPrefs {
    fn default() -> Self {
        Self {
            ortho: false,
            osnap: true,
            polar: false,
            osnap_track: false,
            dyn_mode: true,
            show_layer_mgr: true,
            show_left_mgr: true,
            show_lweights: true,
            right_panel_width: 280.0,
            left_panel_width: 260.0,
            autosave_secs: 120,
            takeoff_sink: "none".into(),
        }
    }
}

impl UserPrefs {
    /// `%APPDATA%/AADT/prefs.json` on Windows; `dirs`-style elsewhere via
    /// `$XDG_CONFIG_HOME` / `$HOME/.config`.
    pub fn config_path() -> Option<PathBuf> {
        #[cfg(windows)]
        {
            let appdata = std::env::var_os("APPDATA")?;
            Some(PathBuf::from(appdata).join("AADT").join("prefs.json"))
        }
        #[cfg(not(windows))]
        {
            if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
                return Some(PathBuf::from(xdg).join("aadt").join("prefs.json"));
            }
            let home = std::env::var_os("HOME")?;
            Some(
                PathBuf::from(home)
                    .join(".config")
                    .join("aadt")
                    .join("prefs.json"),
            )
        }
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default();
        };
        Self::from_json(&text).unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, self.to_json());
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\n  \"ortho\": {},\n  \"osnap\": {},\n  \"polar\": {},\n  \"osnap_track\": {},\n  \"dyn_mode\": {},\n  \"show_layer_mgr\": {},\n  \"show_left_mgr\": {},\n  \"show_lweights\": {},\n  \"right_panel_width\": {},\n  \"left_panel_width\": {},\n  \"autosave_secs\": {},\n  \"takeoff_sink\": \"{}\"\n}}\n",
            bool_json(self.ortho),
            bool_json(self.osnap),
            bool_json(self.polar),
            bool_json(self.osnap_track),
            bool_json(self.dyn_mode),
            bool_json(self.show_layer_mgr),
            bool_json(self.show_left_mgr),
            bool_json(self.show_lweights),
            self.right_panel_width,
            self.left_panel_width,
            self.autosave_secs,
            escape_json_str(&self.takeoff_sink),
        )
    }

    pub fn from_json(text: &str) -> Option<Self> {
        if !(text.contains("\"osnap\"") || text.contains("\"ortho\"")) {
            return None;
        }
        let mut p = Self::default();
        if let Some(v) = parse_bool(text, "ortho") {
            p.ortho = v;
        }
        if let Some(v) = parse_bool(text, "osnap") {
            p.osnap = v;
        }
        if let Some(v) = parse_bool(text, "polar") {
            p.polar = v;
        }
        if let Some(v) = parse_bool(text, "osnap_track") {
            p.osnap_track = v;
        }
        if let Some(v) = parse_bool(text, "dyn_mode") {
            p.dyn_mode = v;
        }
        if let Some(v) = parse_bool(text, "show_layer_mgr") {
            p.show_layer_mgr = v;
        }
        if let Some(v) = parse_bool(text, "show_left_mgr") {
            p.show_left_mgr = v;
        }
        if let Some(v) = parse_bool(text, "show_lweights") {
            p.show_lweights = v;
        }
        if let Some(v) = parse_f32(text, "right_panel_width") {
            p.right_panel_width = v;
        }
        if let Some(v) = parse_f32(text, "left_panel_width") {
            p.left_panel_width = v;
        }
        if let Some(v) = parse_u64(text, "autosave_secs") {
            p.autosave_secs = v;
        }
        if let Some(v) = parse_string(text, "takeoff_sink") {
            p.takeoff_sink = v;
        }
        Some(p)
    }
}

fn escape_json_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn parse_string(text: &str, key: &str) -> Option<String> {
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

fn bool_json(v: bool) -> &'static str {
    if v {
        "true"
    } else {
        "false"
    }
}

fn parse_bool(text: &str, key: &str) -> Option<bool> {
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

fn parse_f32(text: &str, key: &str) -> Option<f32> {
    parse_number(text, key)?.parse().ok()
}

fn parse_u64(text: &str, key: &str) -> Option<u64> {
    parse_number(text, key)?.parse().ok()
}

fn parse_number(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let i = text.find(&needle)?;
    let rest = &text[i + needle.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let end = after
        .find(|c: char| c == ',' || c == '}' || c.is_whitespace())
        .unwrap_or(after.len());
    Some(after[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_round_trip_json() {
        let p = UserPrefs {
            ortho: true,
            osnap: false,
            polar: true,
            osnap_track: true,
            dyn_mode: false,
            show_layer_mgr: false,
            show_left_mgr: true,
            show_lweights: false,
            right_panel_width: 280.5,
            left_panel_width: 260.0,
            autosave_secs: 60,
            takeoff_sink: "csv:C:\\out".into(),
        };
        let back = UserPrefs::from_json(&p.to_json()).unwrap();
        assert_eq!(back, p);
    }
}
