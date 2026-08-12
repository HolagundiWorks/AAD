//! Drawing units and dimension label formatting.
//!
//! Primary systems:
//! - **Feet & inches** — model units are inches; typed values without a unit suffix are inches.
//! - **Decimal meters** — model units are metres; bare numbers are metres.
//!
//! Explicit suffixes (`mm`, `cm`, `m`, `ft`, `in`, `'`, `"`) always mean that unit and are
//! converted into the active model system — regardless of which system is selected.
//!
//! Optional secondary display swaps the other system in brackets on dimension text.
//! Imperial labels can use architectural fractions (through 1/16) or decimal inches.

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
            "" | "ft-in" | "ft_in" | "ftin" | "architectural" | "arch" | "ft" | "feet" | "foot"
            | "in" | "inch" | "inches" | "imperial" => Some(Self::FeetInches),
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

/// Decimal places for metric (and decimal-inch) labels. Empty → 3. Clamped 0..=8.
pub fn precision_from_props(s: &str) -> u8 {
    s.trim()
        .parse::<u8>()
        .unwrap_or(3)
        .min(8)
}

/// Imperial fraction denominator: `0`/`decimal` = decimal inches; else 1, 2, 4, 8, or 16.
/// Empty defaults to decimal (`0`) so existing drawings keep prior label style.
pub fn fraction_den_from_props(s: &str) -> u32 {
    match s.trim().to_ascii_lowercase().as_str() {
        "" | "0" | "dec" | "decimal" | "off" | "none" => 0,
        "1" => 1,
        "2" => 2,
        "4" => 4,
        "8" => 8,
        "16" => 16,
        _ => 0,
    }
}

fn normalize_length_text(text: &str) -> String {
    text.trim()
        .replace('′', "'")
        .replace('″', "\"")
        .replace('‘', "'")
        .replace('’', "'")
        .replace('“', "\"")
        .replace('”', "\"")
}

/// Strip a trailing unit token (ascii letters) and return `(number_text, unit_lower)`.
fn split_trailing_unit(s: &str) -> Option<(&str, &str)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let bytes = s.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1].is_ascii_alphabetic() {
        i -= 1;
    }
    if i == 0 || i == bytes.len() {
        return None;
    }
    let num = s[..i].trim();
    let unit = s[i..].trim();
    if num.is_empty() || unit.is_empty() {
        return None;
    }
    Some((num, unit))
}

/// Parse an explicit metric length (`12mm`, `2.5 cm`, `3m`) into **metres**.
fn parse_explicit_metric_meters(s: &str) -> Option<Result<f64, String>> {
    let (num, unit) = split_trailing_unit(s)?;
    let u = unit.to_ascii_lowercase();
    let factor = match u.as_str() {
        "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" => 0.001,
        "cm" | "centimeter" | "centimeters" | "centimetre" | "centimetres" => 0.01,
        "m" | "meter" | "meters" | "metre" | "metres" => 1.0,
        _ => return None,
    };
    match num.parse::<f64>() {
        Ok(v) => Some(Ok(v * factor)),
        Err(_) => Some(Err(format!("bad metric value {num:?}"))),
    }
}

fn has_word_imperial_suffix(s: &str) -> bool {
    let Some((_, unit)) = split_trailing_unit(s) else {
        return false;
    };
    matches!(
        unit.to_ascii_lowercase().as_str(),
        "ft" | "feet" | "foot" | "in" | "inch" | "inches"
    )
}

/// Parse a typed length into **model units** for the active system.
///
/// Explicit suffixes always win (converted into the model system):
/// - `2500mm` / `25cm` / `2.5m` → metres then → model
/// - `12'` / `6"` / `12'-6"` / `10ft` / `6in` → inches then → model
///
/// Bare numbers use the active system (inches or metres).
pub fn parse_length(text: &str, system: UnitSystem) -> Result<f64, String> {
    let s = normalize_length_text(text);
    if s.is_empty() {
        return Err("empty length".into());
    }
    let s = s.trim_start_matches('@').trim();
    if s.is_empty() {
        return Err("empty length".into());
    }

    if let Some(res) = parse_explicit_metric_meters(s) {
        let meters = res?;
        return Ok(match system {
            UnitSystem::FeetInches => meters * INCHES_PER_METER,
            UnitSystem::Meters => meters,
        });
    }

    if s.contains('\'') || s.contains('"') || has_word_imperial_suffix(s) {
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
    let s = normalize_length_text(s);
    let neg = s.starts_with('-');
    let s = s.trim_start_matches('-').trim();

    if let Some((num, unit)) = split_trailing_unit(s) {
        let u = unit.to_ascii_lowercase();
        if matches!(u.as_str(), "ft" | "feet" | "foot") {
            let feet: f64 = num
                .parse()
                .map_err(|_| format!("bad feet value {num:?}"))?;
            let total = feet * 12.0;
            return Ok(if neg { -total } else { total });
        }
        if matches!(u.as_str(), "in" | "inch" | "inches") {
            let inches: f64 = num
                .parse()
                .map_err(|_| format!("bad inches value {num:?}"))?;
            return Ok(if neg { -inches } else { inches });
        }
    }

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

fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.max(1)
}

/// Format a model-unit length in the given system (default precision / decimal inches).
pub fn format_length(value: f64, system: UnitSystem) -> String {
    format_length_with(value, system, 3, 0)
}

/// Format a model-unit length with precision and optional imperial fractions.
///
/// - `precision` — decimal places for metres and for decimal-inch mode (0..=8).
/// - `fraction_den` — `0` = decimal inches; else snap inches to 1/`den` (1, 2, 4, 8, 16).
pub fn format_length_with(
    value: f64,
    system: UnitSystem,
    precision: u8,
    fraction_den: u32,
) -> String {
    match system {
        UnitSystem::FeetInches => format_feet_inches(value, precision, fraction_den),
        UnitSystem::Meters => {
            let p = precision.min(8) as usize;
            format!("{value:.p$} m")
        }
    }
}

fn format_inch_part(inches: f64, precision: u8, fraction_den: u32) -> String {
    if fraction_den == 0 {
        let p = precision.min(8) as usize;
        // Trim trailing zeros for cleaner architectural decimal labels when p>0.
        let s = format!("{inches:.p$}");
        if let Some(dot) = s.find('.') {
            let trimmed = s.trim_end_matches('0');
            if trimmed.ends_with('.') {
                trimmed[..trimmed.len() - 1].to_string()
            } else if trimmed.len() > dot {
                trimmed.to_string()
            } else {
                s
            }
        } else {
            s
        }
    } else {
        let den = match fraction_den {
            1 | 2 | 4 | 8 | 16 => fraction_den,
            _ => 16,
        };
        let whole = inches.floor().max(0.0) as i64;
        let mut num = ((inches - whole as f64) * den as f64).round() as i64;
        let mut w = whole;
        if num >= den as i64 {
            w += 1;
            num = 0;
        }
        if num <= 0 {
            format!("{w}")
        } else {
            let g = gcd_u32(num as u32, den);
            format!("{w} {}/{}", num as u32 / g, den / g)
        }
    }
}

fn format_feet_inches(inches: f64, precision: u8, fraction_den: u32) -> String {
    let sign = if inches < 0.0 { "-" } else { "" };
    let mut inches = inches.abs();
    // Snap tiny float noise.
    let eps = if fraction_den > 0 {
        0.5 / fraction_den as f64
    } else {
        0.5 * 10f64.powi(-(precision.min(8) as i32))
    };
    if inches < eps {
        return format!("{sign}0\"");
    }

    // When using fractions, snap total inches to the grid first.
    if fraction_den > 0 {
        let den = match fraction_den {
            1 | 2 | 4 | 8 | 16 => fraction_den,
            _ => 16,
        };
        inches = (inches * den as f64).round() / den as f64;
    }

    let feet = (inches / 12.0).floor() as i64;
    inches -= feet as f64 * 12.0;
    if inches >= 12.0 - 1e-9 {
        return format_feet_inches(
            if sign.is_empty() {
                (feet + 1) as f64 * 12.0
            } else {
                -((feet + 1) as f64 * 12.0)
            },
            precision,
            fraction_den,
        );
    }

    let inch_s = format_inch_part(inches, precision, fraction_den);
    if feet == 0 {
        format!("{sign}{inch_s}\"")
    } else if inches < eps {
        format!("{sign}{feet}'-0\"")
    } else {
        format!("{sign}{feet}'-{inch_s}\"")
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

/// Format a linear measurement with optional secondary units (default formatting).
pub fn format_length_pair(value: f64, primary: UnitSystem, show_secondary: bool) -> String {
    format_length_pair_with(value, primary, show_secondary, 3, 0)
}

/// Format a linear measurement with precision / fraction options.
pub fn format_length_pair_with(
    value: f64,
    primary: UnitSystem,
    show_secondary: bool,
    precision: u8,
    fraction_den: u32,
) -> String {
    let primary_s = format_length_with(value, primary, precision, fraction_den);
    if !show_secondary {
        return primary_s;
    }
    let sec = primary.secondary();
    let sec_val = convert_model(value, primary, sec);
    // Secondary uses same precision; fractions only apply to imperial side.
    let sec_frac = if sec == UnitSystem::FeetInches {
        fraction_den
    } else {
        0
    };
    format!(
        "{primary_s} [{}]",
        format_length_with(sec_val, sec, precision, sec_frac)
    )
}

/// Format a dimension label from kind + measurement + drawing props.
pub fn format_dimension_label(
    kind: crate::DimensionKind,
    measurement: f64,
    override_text: &str,
    units: &str,
    dim_secondary: &str,
) -> String {
    format_dimension_label_ex(
        kind,
        measurement,
        override_text,
        units,
        dim_secondary,
        "",
        "",
    )
}

/// Format a dimension label with precision and imperial fraction settings.
pub fn format_dimension_label_ex(
    kind: crate::DimensionKind,
    measurement: f64,
    override_text: &str,
    units: &str,
    dim_secondary: &str,
    dim_precision: &str,
    dim_fraction: &str,
) -> String {
    if !override_text.trim().is_empty() {
        return override_text.to_string();
    }
    use crate::DimensionKind::*;
    let primary = primary_from_props(units);
    let secondary = secondary_enabled(dim_secondary);
    let precision = precision_from_props(dim_precision);
    let fraction_den = fraction_den_from_props(dim_fraction);
    match kind {
        Angular => format!("{measurement:.1}°"),
        Radius => format!(
            "R{}",
            format_length_pair_with(measurement, primary, secondary, precision, fraction_den)
        ),
        Diameter => format!(
            "⌀{}",
            format_length_pair_with(measurement, primary, secondary, precision, fraction_den)
        ),
        Linear | Aligned | OrdinateX | OrdinateY => {
            format_length_pair_with(measurement, primary, secondary, precision, fraction_den)
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
    fn parse_explicit_metric_any_system() {
        let mm_in =
            parse_length("25.4mm", UnitSystem::FeetInches).unwrap();
        assert!((mm_in - 1.0).abs() < 1e-6);
        let m_m = parse_length("2.5m", UnitSystem::Meters).unwrap();
        assert!((m_m - 2.5).abs() < 1e-9);
        let cm_in = parse_length("2.54cm", UnitSystem::FeetInches).unwrap();
        assert!((cm_in - 1.0).abs() < 1e-6);
        let ft_m = parse_length("10ft", UnitSystem::Meters).unwrap();
        assert!((ft_m - 3.048).abs() < 1e-6);
    }

    #[test]
    fn format_ft_in_and_secondary() {
        assert_eq!(format_length(150.0, UnitSystem::FeetInches), "12'-6\"");
        let s = format_length_pair(150.0, UnitSystem::FeetInches, true);
        assert!(s.starts_with("12'-6\""));
        assert!(s.contains("m"));
        let s2 = format_length_pair(3.81, UnitSystem::Meters, true);
        assert!(s2.contains("m"));
        assert!(s2.contains('\'') || s2.contains('"'));
    }

    #[test]
    fn format_imperial_sixteenths() {
        // 6.0625" = 6 1/16"
        let s = format_length_with(6.0625, UnitSystem::FeetInches, 3, 16);
        assert_eq!(s, "6 1/16\"");
        // 150.5" = 12'-6 1/2"
        let s2 = format_length_with(150.5, UnitSystem::FeetInches, 3, 16);
        assert_eq!(s2, "12'-6 1/2\"");
    }

    #[test]
    fn format_metric_precision() {
        assert_eq!(
            format_length_with(3.14159, UnitSystem::Meters, 2, 0),
            "3.14 m"
        );
        assert_eq!(
            format_length_with(3.1, UnitSystem::Meters, 0, 0),
            "3 m"
        );
    }

    #[test]
    fn unit_tokens() {
        assert_eq!(UnitSystem::parse("ft-in"), Some(UnitSystem::FeetInches));
        assert_eq!(UnitSystem::parse("m"), Some(UnitSystem::Meters));
    }
}
