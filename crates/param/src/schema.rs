//! Versioned component type definitions. A component instance in a graph names its type and the
//! schema version it was made with; a version this program does not know is refused, never
//! reinterpreted.

use serde::Serialize;

use crate::types::DataType;
use crate::units::Dim;

/// What changing a parameter affects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Affects {
    pub geometry: bool,
    pub takeoff: bool,
    pub specification: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub id: &'static str,
    pub label: &'static str,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamDef {
    pub id: &'static str,
    pub label: &'static str,
    pub data_type: DataType,
    pub required: bool,
    /// Canonical default, for numbers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_choice: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// The unit a control shows the value in.
    pub unit: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<Choice>,
    pub affects: Affects,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentType {
    pub id: &'static str,
    pub label: &'static str,
    pub schema_version: u32,
    pub spec_version: u32,
    pub params: Vec<ParamDef>,
    /// The take-off contract rows of this component follow (`hcw-aqc-bridge` row kind and version).
    pub measurement_rule: &'static str,
}

impl ComponentType {
    pub fn param(&self, id: &str) -> Option<&ParamDef> {
        self.params.iter().find(|p| p.id == id)
    }
}

pub const WALL_TYPE: &str = "osas.wall";

const GEOMETRY: Affects = Affects { geometry: true, takeoff: true, specification: false };
const SPEC_ONLY: Affects = Affects { geometry: false, takeoff: false, specification: true };
const SPEC_AND_TAKEOFF: Affects = Affects { geometry: false, takeoff: true, specification: true };

/// Wall, schema version 1.
pub fn wall_v1() -> ComponentType {
    let length = DataType::number(Dim::LENGTH);
    ComponentType {
        id: WALL_TYPE,
        label: "Wall",
        schema_version: 1,
        spec_version: 1,
        measurement_rule: "hcw-aqc-bridge/1#walls",
        params: vec![
            ParamDef {
                id: "path",
                label: "Centre line",
                data_type: DataType::Curve,
                required: true,
                default: None,
                default_choice: None,
                min: None,
                max: None,
                unit: "",
                choices: vec![],
                affects: GEOMETRY,
            },
            ParamDef {
                id: "height",
                label: "Height",
                data_type: length.clone(),
                required: false,
                default: Some(3.0),
                default_choice: None,
                min: Some(0.1),
                max: Some(20.0),
                unit: "m",
                choices: vec![],
                affects: Affects { geometry: false, takeoff: true, specification: true },
            },
            ParamDef {
                id: "thickness",
                label: "Thickness",
                data_type: length,
                required: false,
                default: Some(0.23),
                default_choice: None,
                min: Some(0.05),
                max: Some(2.0),
                unit: "mm",
                choices: vec![],
                affects: GEOMETRY,
            },
            ParamDef {
                id: "material",
                label: "Material",
                data_type: DataType::Enum,
                required: false,
                default: None,
                default_choice: Some("brick.full"),
                min: None,
                max: None,
                unit: "",
                choices: vec![
                    Choice { id: "brick.full", label: "Brick, full" },
                    Choice { id: "brick.half", label: "Brick, half" },
                    Choice { id: "block.concrete", label: "Concrete block" },
                    Choice { id: "rcc", label: "Reinforced concrete" },
                ],
                affects: SPEC_AND_TAKEOFF,
            },
            ParamDef {
                id: "finish",
                label: "Finish",
                data_type: DataType::Enum,
                required: false,
                default: None,
                default_choice: Some("plaster.internal"),
                min: None,
                max: None,
                unit: "",
                choices: vec![
                    Choice { id: "plaster.internal", label: "Internal plaster" },
                    Choice { id: "plaster.external", label: "External plaster" },
                    Choice { id: "none", label: "No finish" },
                ],
                affects: SPEC_ONLY,
            },
        ],
    }
}

/// Every component type and version this program knows.
pub fn component_types() -> Vec<ComponentType> {
    vec![wall_v1()]
}

pub fn component_type(id: &str, version: u32) -> Option<ComponentType> {
    component_types().into_iter().find(|c| c.id == id && c.schema_version == version)
}
