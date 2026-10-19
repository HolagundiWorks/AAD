//! Dimensions and units. Every number in a graph is held in canonical SI units (metre, kilogram,
//! second, radian) with its dimension; units are only read and written at the edges (node
//! configuration, formulas, results).

use serde::{Deserialize, Serialize};

/// Exponents of length, mass, time and angle. `Dim::NONE` is a plain number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dim {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub length: i8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub mass: i8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub time: i8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub angle: i8,
}

fn is_zero(v: &i8) -> bool {
    *v == 0
}

impl Dim {
    pub const NONE: Dim = Dim { length: 0, mass: 0, time: 0, angle: 0 };
    pub const LENGTH: Dim = Dim { length: 1, mass: 0, time: 0, angle: 0 };
    pub const AREA: Dim = Dim { length: 2, mass: 0, time: 0, angle: 0 };
    pub const VOLUME: Dim = Dim { length: 3, mass: 0, time: 0, angle: 0 };
    pub const MASS: Dim = Dim { length: 0, mass: 1, time: 0, angle: 0 };
    pub const TIME: Dim = Dim { length: 0, mass: 0, time: 1, angle: 0 };
    pub const ANGLE: Dim = Dim { length: 0, mass: 0, time: 0, angle: 1 };

    /// The product's dimension, or `None` when an exponent leaves the `i8` range.
    pub fn times(self, o: Dim) -> Option<Dim> {
        Some(Dim {
            length: self.length.checked_add(o.length)?,
            mass: self.mass.checked_add(o.mass)?,
            time: self.time.checked_add(o.time)?,
            angle: self.angle.checked_add(o.angle)?,
        })
    }
    pub fn per(self, o: Dim) -> Option<Dim> {
        Some(Dim {
            length: self.length.checked_sub(o.length)?,
            mass: self.mass.checked_sub(o.mass)?,
            time: self.time.checked_sub(o.time)?,
            angle: self.angle.checked_sub(o.angle)?,
        })
    }
    pub fn pow(self, n: i8) -> Option<Dim> {
        Some(Dim {
            length: self.length.checked_mul(n)?,
            mass: self.mass.checked_mul(n)?,
            time: self.time.checked_mul(n)?,
            angle: self.angle.checked_mul(n)?,
        })
    }
    /// Half the exponents (square root), when they are all even.
    pub fn sqrt(self) -> Option<Dim> {
        let half = |v: i8| (v % 2 == 0).then_some(v / 2);
        Some(Dim { length: half(self.length)?, mass: half(self.mass)?, time: half(self.time)?, angle: half(self.angle)? })
    }

    /// A readable name: `length`, `area`, `volume`, … or the exponents.
    pub fn name(self) -> String {
        match self {
            Dim::NONE => "number".into(),
            Dim::LENGTH => "length".into(),
            Dim::AREA => "area".into(),
            Dim::VOLUME => "volume".into(),
            Dim::MASS => "mass".into(),
            Dim::TIME => "time".into(),
            Dim::ANGLE => "angle".into(),
            d => format!("L^{} M^{} T^{} A^{}", d.length, d.mass, d.time, d.angle),
        }
    }

    /// The dimension a name such as `length` or `area` stands for.
    pub fn from_name(s: &str) -> Option<Dim> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "" | "number" | "none" => Dim::NONE,
            "length" => Dim::LENGTH,
            "area" => Dim::AREA,
            "volume" => Dim::VOLUME,
            "mass" => Dim::MASS,
            "time" => Dim::TIME,
            "angle" => Dim::ANGLE,
            _ => return None,
        })
    }

    /// The unit results of this dimension are shown in.
    pub fn display_unit(self) -> &'static str {
        match self {
            Dim::LENGTH => "m",
            Dim::AREA => "m2",
            Dim::VOLUME => "m3",
            Dim::MASS => "kg",
            Dim::TIME => "s",
            Dim::ANGLE => "deg",
            _ => "",
        }
    }
}

/// A known unit: its dimension and the factor to canonical SI.
pub fn unit(name: &str) -> Option<(Dim, f64)> {
    const DEG: f64 = std::f64::consts::PI / 180.0;
    Some(match name.trim() {
        "" => (Dim::NONE, 1.0),
        "mm" => (Dim::LENGTH, 0.001),
        "cm" => (Dim::LENGTH, 0.01),
        "m" => (Dim::LENGTH, 1.0),
        "km" => (Dim::LENGTH, 1000.0),
        "in" => (Dim::LENGTH, 0.0254),
        "ft" => (Dim::LENGTH, 0.3048),
        "mm2" => (Dim::AREA, 1e-6),
        "cm2" => (Dim::AREA, 1e-4),
        "m2" => (Dim::AREA, 1.0),
        "ft2" => (Dim::AREA, 0.3048 * 0.3048),
        "mm3" => (Dim::VOLUME, 1e-9),
        "m3" => (Dim::VOLUME, 1.0),
        "l" => (Dim::VOLUME, 0.001),
        "ft3" => (Dim::VOLUME, 0.3048 * 0.3048 * 0.3048),
        "kg" => (Dim::MASS, 1.0),
        "t" => (Dim::MASS, 1000.0),
        "s" => (Dim::TIME, 1.0),
        "h" => (Dim::TIME, 3600.0),
        "deg" => (Dim::ANGLE, DEG),
        "rad" => (Dim::ANGLE, 1.0),
        _ => return None,
    })
}

/// `value` given in `unit_name`, in canonical units.
pub fn to_si(value: f64, unit_name: &str) -> Option<(f64, Dim)> {
    let (d, f) = unit(unit_name)?;
    let v = value * f;
    v.is_finite().then_some((v, d))
}

/// A canonical value expressed in `unit_name`, when the dimensions agree.
pub fn from_si(value: f64, dim: Dim, unit_name: &str) -> Option<f64> {
    let (d, f) = unit(unit_name)?;
    (d == dim).then(|| value / f)
}

/// Parse `3.2 m`, `230mm`, `90 deg` or a bare number into a canonical value.
pub fn parse_quantity(s: &str) -> Option<(f64, Dim)> {
    let t = s.trim();
    let split = t.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(t.len());
    let (num, u) = t.split_at(split);
    let v: f64 = num.trim().parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    to_si(v, u.trim())
}
