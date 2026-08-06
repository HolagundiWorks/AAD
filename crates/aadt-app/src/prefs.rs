//! Persisted user preferences (`prefs.json` under the platform config dir).

use std::fs;
use std::path::PathBuf;

/// How a manager dock is presented (§28 / M9.6). Chrome 2.0 defaults to auto-hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelMode {
    /// Edge-docked `SidePanel` (expanded or collapsed to a rail).
    Docked,
    /// Free-floating `egui::Window` over the canvas.
    Floating,
    /// Collapsed to an edge rail; expands as an overlay on hover.
    #[default]
    AutoHide,
}

impl PanelMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PanelMode::Docked => "docked",
            PanelMode::Floating => "floating",
            PanelMode::AutoHide => "autohide",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "docked" | "dock" => Some(PanelMode::Docked),
            "floating" | "float" => Some(PanelMode::Floating),
            "autohide" | "auto_hide" | "auto-hide" => Some(PanelMode::AutoHide),
            _ => None,
        }
    }
}

/// Screen rectangle for a floating manager window (logical points).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl FloatRect {
    pub fn left_default() -> Self {
        Self {
            x: 48.0,
            y: 64.0,
            w: 320.0,
            h: 520.0,
        }
    }

    pub fn right_default() -> Self {
        Self {
            x: 720.0,
            y: 64.0,
            w: 324.0,
            h: 520.0,
        }
    }
}

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
    /// Dark chrome theme (false = light, the default).
    pub dark_theme: bool,
    /// Left manager presentation (docked / floating / auto-hide).
    pub left_panel_mode: PanelMode,
    /// Right manager presentation (docked / floating / auto-hide).
    pub right_panel_mode: PanelMode,
    /// Last floating geometry for the left manager.
    pub left_float: FloatRect,
    /// Last floating geometry for the right manager.
    pub right_float: FloatRect,
}

impl Default for UserPrefs {
    fn default() -> Self {
        Self {
            ortho: false,
            osnap: true,
            polar: false,
            osnap_track: false,
            dyn_mode: true,
            show_layer_mgr: false,
            show_left_mgr: false,
            show_lweights: true,
            right_panel_width: 280.0,
            left_panel_width: 260.0,
            autosave_secs: 120,
            dark_theme: false,
            left_panel_mode: PanelMode::AutoHide,
            right_panel_mode: PanelMode::AutoHide,
            left_float: FloatRect::left_default(),
            right_float: FloatRect::right_default(),
        }
    }
}

impl UserPrefs {
    /// `%APPDATA%/AAD/prefs.json` on Windows; `dirs`-style elsewhere via
    /// `$XDG_CONFIG_HOME` / `$HOME/.config`.
    pub fn config_path() -> Option<PathBuf> {
        #[cfg(windows)]
        {
            let appdata = std::env::var_os("APPDATA")?;
            Some(PathBuf::from(appdata).join("AAD").join("prefs.json"))
        }
        #[cfg(not(windows))]
        {
            if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
                return Some(PathBuf::from(xdg).join("aad").join("prefs.json"));
            }
            let home = std::env::var_os("HOME")?;
            Some(
                PathBuf::from(home)
                    .join(".config")
                    .join("aad")
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
            "{{\n  \"ortho\": {},\n  \"osnap\": {},\n  \"polar\": {},\n  \"osnap_track\": {},\n  \"dyn_mode\": {},\n  \"show_layer_mgr\": {},\n  \"show_left_mgr\": {},\n  \"show_lweights\": {},\n  \"right_panel_width\": {},\n  \"left_panel_width\": {},\n  \"autosave_secs\": {},\n  \"dark_theme\": {},\n  \"left_panel_mode\": \"{}\",\n  \"right_panel_mode\": \"{}\",\n  \"left_float\": [{}, {}, {}, {}],\n  \"right_float\": [{}, {}, {}, {}]\n}}\n",
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
            bool_json(self.dark_theme),
            self.left_panel_mode.as_str(),
            self.right_panel_mode.as_str(),
            self.left_float.x,
            self.left_float.y,
            self.left_float.w,
            self.left_float.h,
            self.right_float.x,
            self.right_float.y,
            self.right_float.w,
            self.right_float.h,
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
        if let Some(v) = parse_bool(text, "dark_theme") {
            p.dark_theme = v;
        }
        if let Some(v) = parse_string(text, "left_panel_mode").and_then(|s| PanelMode::parse(&s)) {
            p.left_panel_mode = v;
        }
        if let Some(v) = parse_string(text, "right_panel_mode").and_then(|s| PanelMode::parse(&s)) {
            p.right_panel_mode = v;
        }
        if let Some(r) = parse_float_rect(text, "left_float") {
            p.left_float = r;
        }
        if let Some(r) = parse_float_rect(text, "right_float") {
            p.right_float = r;
        }
        Some(p)
    }
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

fn parse_string(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let i = text.find(&needle)?;
    let rest = &text[i + needle.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    if !after.starts_with('"') {
        return None;
    }
    let body = &after[1..];
    let end = body.find('"')?;
    Some(body[..end].to_string())
}

fn parse_float_rect(text: &str, key: &str) -> Option<FloatRect> {
    let needle = format!("\"{key}\"");
    let i = text.find(&needle)?;
    let rest = &text[i + needle.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let start = after.find('[')?;
    let end = after[start..].find(']')?;
    let inner = &after[start + 1..start + end];
    let mut parts = inner.split(',').map(|s| s.trim().parse::<f32>().ok());
    let x = parts.next()??;
    let y = parts.next()??;
    let w = parts.next()??;
    let h = parts.next()??;
    Some(FloatRect {
        x,
        y,
        w: w.clamp(200.0, 640.0),
        h: h.clamp(200.0, 1200.0),
    })
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
            dark_theme: true,
            left_panel_mode: PanelMode::Floating,
            right_panel_mode: PanelMode::AutoHide,
            left_float: FloatRect {
                x: 40.0,
                y: 80.0,
                w: 300.0,
                h: 480.0,
            },
            right_float: FloatRect {
                x: 900.0,
                y: 70.0,
                w: 310.0,
                h: 500.0,
            },
        };
        let back = UserPrefs::from_json(&p.to_json()).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn prefs_legacy_json_defaults_panel_modes() {
        let legacy = r#"{
  "ortho": false,
  "osnap": true,
  "polar": false,
  "osnap_track": false,
  "dyn_mode": true,
  "show_layer_mgr": true,
  "show_left_mgr": true,
  "show_lweights": true,
  "right_panel_width": 280.0,
  "left_panel_width": 260.0,
  "autosave_secs": 120,
  "dark_theme": false
}
"#;
        let p = UserPrefs::from_json(legacy).unwrap();
        assert_eq!(p.left_panel_mode, PanelMode::AutoHide);
        assert_eq!(p.right_panel_mode, PanelMode::AutoHide);
    }

    #[test]
    fn panel_mode_parse() {
        assert_eq!(PanelMode::parse("floating"), Some(PanelMode::Floating));
        assert_eq!(PanelMode::parse("auto-hide"), Some(PanelMode::AutoHide));
        assert_eq!(PanelMode::parse("docked"), Some(PanelMode::Docked));
        assert_eq!(PanelMode::parse("nope"), None);
    }
}
