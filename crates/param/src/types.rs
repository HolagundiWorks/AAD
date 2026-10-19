//! The type system of ports and the values that flow along edges.
//!
//! A number is not a length, a length is not an area, and geometry is not a component: a
//! connection is only made between ports of the same [`DataType`].

use serde::{Deserialize, Serialize};

use crate::units::Dim;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DataType {
    /// A real number of a dimension (`Dim::NONE` for a plain number).
    Number {
        dim: Dim,
    },
    Integer,
    Boolean,
    Text,
    /// A choice from a fixed list of stable ids.
    Enum,
    Point,
    Curve,
    GeometryRef,
    Wall,
    Material,
    Finish,
    Level,
}

impl DataType {
    pub const fn number(dim: Dim) -> Self {
        DataType::Number { dim }
    }
    pub fn name(&self) -> String {
        match self {
            DataType::Number { dim } => dim.name(),
            DataType::Integer => "integer".into(),
            DataType::Boolean => "boolean".into(),
            DataType::Text => "text".into(),
            DataType::Enum => "choice".into(),
            DataType::Point => "point".into(),
            DataType::Curve => "curve".into(),
            DataType::GeometryRef => "geometry reference".into(),
            DataType::Wall => "wall".into(),
            DataType::Material => "material".into(),
            DataType::Finish => "finish".into(),
            DataType::Level => "level".into(),
        }
    }
}

/// A wall component as evaluated: its centre line and resolved parameters (canonical units).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallValue {
    pub instance_id: String,
    pub component_type: String,
    pub schema_version: u32,
    pub path: Vec<[f64; 2]>,
    pub closed: bool,
    pub length: f64,
    pub height: f64,
    pub thickness: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Value {
    /// Canonical SI value.
    Number {
        value: f64,
        dim: Dim,
    },
    Integer {
        value: i64,
    },
    Boolean {
        value: bool,
    },
    Text {
        value: String,
    },
    Enum {
        value: String,
    },
    Point {
        value: [f64; 2],
    },
    Curve {
        points: Vec<[f64; 2]>,
        closed: bool,
    },
    Wall {
        wall: Box<WallValue>,
    },
}

impl Value {
    pub fn data_type(&self) -> DataType {
        match self {
            Value::Number { dim, .. } => DataType::Number { dim: *dim },
            Value::Integer { .. } => DataType::Integer,
            Value::Boolean { .. } => DataType::Boolean,
            Value::Text { .. } => DataType::Text,
            Value::Enum { .. } => DataType::Enum,
            Value::Point { .. } => DataType::Point,
            Value::Curve { .. } => DataType::Curve,
            Value::Wall { .. } => DataType::Wall,
        }
    }
    pub fn number(&self) -> Option<(f64, Dim)> {
        match self {
            Value::Number { value, dim } => Some((*value, *dim)),
            _ => None,
        }
    }
}
