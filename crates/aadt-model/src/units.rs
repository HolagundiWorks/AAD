//! Drawing units and dimension label formatting.
//!
//! Primary systems:
//! - **Feet & inches** — model units are inches; typed values without `'`/`"` are inches.
//! - **Decimal meters** — model units are metres.
//!
//! Optional secondary display swaps the other system in brackets on dimension text.

/// Inches per metre (international foot).
pub const INCHES_PER_METER: f64 = 39.370_078_740_157_48;
/// Metres per inch.
pub const METERS_PER_INCH: f64 = 0.0254;

/// Primary linear unit system for the drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnitSystem {
    /// Architectural feet & inches; world coordinates are **inches**.
    #[default]
    FeetInches,
    /// Decimal metres; world coordinates are **metres**.
    Meters,
}

impl UnitSystem {
    /// Parse a UNITS token (`ft-in`, `ft`, `in`, `m`, `meters`, …).
    pub fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "" | "ft-in" | "ft_in" | "ftin" | "architectural" | "arch" | "ft" | "feet"
            | "foot" | "in" | "inch" | "inches" | "imperial" => Some(Self::FeetInches),
            "m" | "meter" | "meters" | "metre" | "metres" | "metric" | "decimal" => {
                Some(Self::Meters)
            }
            // Millimetres stored as metres for display simplicity (value/1000).
            "mm" | "millimeter" | "millimetre" => Some(Self::Meters),
            _ => None,
        }
    }

    pub fn as_token(self) -> &'static str {
        match self {
            Self::FeetInches => "ft-in",
            Self::Meters => "m",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::FeetInches => "Feet & inches",
            Self::Meters => "Decimal meters",
        }
    }

    /// Default DIMSTYLE name for this system.
    pub fn dimstyle_name(self) -> &'static str {
        match self {
            Self::FeetInches => "ARCHITECTURAL",
            Self::Meters => "METRIC",
        }
    }

    pub fn secondary(self) -> Self {
        match self {
            Self::FeetInches => Self::Meters,
            Self::Meters => Self::FeetInches,
        }
    }
}

/// Resolve primary system from drawing properties (`units` field).
pub fn primary_from_props(units: &str) -> UnitSystem {
    UnitSystem::parse(units).unwrap_or(UnitSystem::FeetInches)
}

/// Secondary display enabled when `dim_secondary` is `1` / `true` / `on`.
pub fn secondary_enabled(flag: &str) -> bool {
    matches!(
        flag.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Parse a typed length into **model units** for the active system.
///
/// Feet & inches examples (result in inches):
/// - `6` or `6.5` → inches (default)
/// - `6"` → 6 in
/// - `12'` → 144 in
/// - `12'-6"` / `12' 6"` / `12'-6` → 150 in
///
/// Meters examples (result in metres): `3.5`, or `12'-6"` converted via inches.
pub fn parse_length(text: &str, system: UnitSystem) -> Result<f64, String> {
    let s = text.trim();
    if s.is_empty() {
        return Err("empty length".into());
    }
    // Architectural feet-inches always parse to inches first, then convert if needed.
    if s.contains('\'') || s.contains('"') {
        let inches = parse_feet_inches_to_inches(s)?;
        return Ok(match system {
            UnitSystem::FeetInches => inches,
            UnitSystem::Meters => inches * METERS_PER_INCH,
        });
    }
    let v: f64 = s
        .parse()
        .map_err(|_| format!("not a length: {text:?}"))?;
    Ok(v) // bare number = inches or metres per active system
}

fn parse_feet_inches_to_inches(s: &str) -> Result<f64, String> {
    let s = s.trim().replace('′', "'").replace('″', "\"");
    let neg = s.starts_with('-');
    let s = s.trim_start_matches('-').trim();

    let (feet_part, inch_part) = if let Some(i) = s.find('\'') {
        let feet = s[..i].trim();
        let rest = s[i + 1..].trim().trim_start_matches('-').trim();
        let rest = rest.trim_end_matches('"').trim();
        (feet, rest)
    } else if let Some(i) = s.find('"') {
        ("", s[..i].trim())
    } else {
        return Err(format!("expected feet/inches markers in {s:?}"));
    };

    let feet: f64 = if feet_part.is_empty() {
        0.0
    } else {
        feet_part
            .parse()
            .map_err(|_| format!("bad feet value {feet_part:?}"))?
    };
    let inches: f64 = if inch_part.is_empty() {
        0.0
    } else {
        inch_part
            .parse()
            .map_err(|_| format!("bad inches value {inch_part:?}"))?
    };
    let total = feet * 12.0 + inches;
    Ok(if neg { -total } else { total })
}

/// Format a model-unit length in the given system.
pub fn format_length(value: f64, system: UnitSystem) -> String {
    match system {
        UnitSystem::FeetInches => format_feet_inches(value),
        UnitSystem::Meters => format!("{value:.3} m"),
    }
}

fn format_feet_inches(inches: f64) -> String {
    let sign = if inches < 0.0 { "-" } else { "" };
    let mut inches = inches.abs();
    // Snap tiny float noise.
    if (inches * 100.0).round() / 100.0 == 0.0 {
        return format!("{sign}0\"");
    }
    let feet = (inches / 12.0).floor() as i64;
    inches -= feet as f64 * 12.0;
    inches = (inches * 100.0).round() / 100.0;
    if inches >= 12.0 {
        return format_feet_inches(if sign.is_empty() {
            feet as f64 * 12.0 + inches
        } else {
            -(feet as f64 * 12.0 + inches)
        });
    }
    if feet == 0 {
        format!("{sign}{inches}\"")
    } else if inches < 0.005 {
        format!("{sign}{feet}'-0\"")
    } else {
        format!("{sign}{feet}'-{inches}\"")
    }
}

/// Convert model value from `from` system interpretation to `to` system display value.
fn convert_model(value: f64, from: UnitSystem, to: UnitSystem) -> f64 {
    if from == to {
        return value;
    }
    match (from, to) {
        (UnitSystem::FeetInches, UnitSystem::Meters) => value * METERS_PER_INCH,
        (UnitSystem::Meters, UnitSystem::FeetInches) => value * INCHES_PER_METER,
        _ => value,
    }
}

/// Format a linear measurement with optional secondary units.
pub fn format_length_pair(value: f64, primary: UnitSystem, show_secondary: bool) -> String {
    let primary_s = format_length(value, primary);
    if !show_secondary {
        return primary_s;
    }
    let sec = primary.secondary();
    let sec_val = convert_model(value, primary, sec);
    format!("{primary_s} [{}]", format_length(sec_val, sec))
}

/// Format a dimension label from kind + measurement + drawing props.
pub fn format_dimension_label(
    kind: crate::DimensionKind,
    measurement: f64,
    override_text: &str,
    units: &str,
    dim_secondary: &str,
) -> String {
    if !override_text.trim().is_empty() {
        return override_text.to_string();
    }
    use crate::DimensionKind::*;
    let primary = primary_from_props(units);
    let secondary = secondary_enabled(dim_secondary);
    match kind {
        Angular => format!("{measurement:.1}°"),
        Radius => format!(
            "R{}",
            format_length_pair(measurement, primary, secondary)
        ),
        Diameter => format!(
            "⌀{}",
            format_length_pair(measurement, primary, secondary)
        ),
        Linear | Aligned | OrdinateX | OrdinateY => {
            format_length_pair(measurement, primary, secondary)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bare_inches_default() {
        assert!((parse_length("6", UnitSystem::FeetInches).unwrap() - 6.0).abs() < 1e-9);
        assert!((parse_length("12'-6\"", UnitSystem::FeetInches).unwrap() - 150.0).abs() < 1e-9);
        assert!((parse_length("12'", UnitSystem::FeetInches).unwrap() - 144.0).abs() < 1e-9);
        assert!((parse_length("6\"", UnitSystem::FeetInches).unwrap() - 6.0).abs() < 1e-9);
    }

    #[test]
    fn format_ft_in_and_secondary() {
        assert_eq!(format_length(150.0, UnitSystem::FeetInches), "12'-6\"");
        let s = format_length_pair(150.0, UnitSystem::FeetInches, true);
        assert!(s.starts_with("12'-6\""));
        assert!(s.contains("m"));
        let s2 = format_length_pair(3.81, UnitSystem::Meters, true);
        assert!(s2.contains("m"));
        assert!(s2.contains('\'' ) || s2.contains('"'));
    }

    #[test]
    fn unit_tokens() {
        assert_eq!(UnitSystem::parse("ft-in"), Some(UnitSystem::FeetInches));
        assert_eq!(UnitSystem::parse("m"), Some(UnitSystem::Meters));
    }
}
