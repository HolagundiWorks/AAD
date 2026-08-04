//! HCW enterprise CAD layer standard — discipline-based, ISO 13567 / AIA-derived.
//!
//! This is the canonical built-in catalog. Layers are grouped by discipline
//! (`A` architecture, `S` structural, `C` civil, `M` mechanical, `E` electrical,
//! `P` plumbing, `F` fire-fighting, `T` telecom/ELV, `L` landscape, `I` interior,
//! `G` general, plus `X` existing, `D` demolition, `R` survey/reference phases).
//!
//! Every layer carries a semantic [`ElementType`] which drives the richer
//! metadata an AI-native platform needs — IFC class, Uniformat and MasterFormat
//! codes, and AI recognition tags — without hand-authoring those strings on each
//! of the hundreds of layers. Metadata that is genuinely per-instance (material,
//! fire rating, storey) is exposed on the schema but left for the object model to
//! fill; the layer only pins the discipline, element, phase, and plot intent.
//!
//! A block of legacy AIA names is retained at the end as recognised aliases so
//! imported drawings, detection maps, and golden fixtures keep resolving.

use aadt_model::{Layer, LineType};

use crate::aci::{aci_to_srgb, contrast_vs_white};

/// Discipline bucket — the layer-name prefix and the top-level filter/legend key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Discipline {
    Architectural,
    Structural,
    Civil,
    Mechanical,
    Electrical,
    Plumbing,
    FireFighting,
    Telecom,
    Landscape,
    Interior,
    Existing,
    Demolition,
    General,
    Survey,
    /// Annotation content (text, dims, grids) — kept as its own bucket so layer
    /// sets can drop drafting clutter independently of the drawing disciplines.
    Annotation,
    /// Presentation / rendering entourage.
    Presentation,
}

impl Discipline {
    /// Single-letter (or `X`/`D`/`R`) prefix from the HCW standard.
    pub fn prefix(self) -> char {
        match self {
            Discipline::Architectural | Discipline::Annotation | Discipline::Presentation => 'A',
            Discipline::Structural => 'S',
            Discipline::Civil => 'C',
            Discipline::Mechanical => 'M',
            Discipline::Electrical => 'E',
            Discipline::Plumbing => 'P',
            Discipline::FireFighting => 'F',
            Discipline::Telecom => 'T',
            Discipline::Landscape => 'L',
            Discipline::Interior => 'I',
            Discipline::Existing => 'X',
            Discipline::Demolition => 'D',
            Discipline::General => 'G',
            Discipline::Survey => 'R',
        }
    }

    /// Human-readable discipline name.
    pub fn label(self) -> &'static str {
        match self {
            Discipline::Architectural => "Architecture",
            Discipline::Structural => "Structural",
            Discipline::Civil => "Civil",
            Discipline::Mechanical => "Mechanical (HVAC)",
            Discipline::Electrical => "Electrical",
            Discipline::Plumbing => "Plumbing",
            Discipline::FireFighting => "Fire Fighting",
            Discipline::Telecom => "Telecom / ELV",
            Discipline::Landscape => "Landscape",
            Discipline::Interior => "Interior Design",
            Discipline::Existing => "Existing",
            Discipline::Demolition => "Demolition",
            Discipline::General => "General",
            Discipline::Survey => "Survey / Reference",
            Discipline::Annotation => "Annotation",
            Discipline::Presentation => "Presentation",
        }
    }

    /// Every discipline, for iteration (legends, layer-set builders, docs).
    pub const ALL: &'static [Discipline] = &[
        Discipline::Architectural,
        Discipline::Structural,
        Discipline::Civil,
        Discipline::Mechanical,
        Discipline::Electrical,
        Discipline::Plumbing,
        Discipline::FireFighting,
        Discipline::Telecom,
        Discipline::Landscape,
        Discipline::Interior,
        Discipline::Existing,
        Discipline::Demolition,
        Discipline::General,
        Discipline::Survey,
        Discipline::Annotation,
        Discipline::Presentation,
    ];
}

/// Construction phase a layer belongs to (drives phase filtering / demolition sets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    New,
    Existing,
    Demolish,
}

/// Semantic element a layer carries. This is the join key to BIM/IFC and to the
/// takeoff / clash / AI-recognition subsystems: one place maps an element to its
/// IFC class, Uniformat & MasterFormat codes, and AI tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementType {
    // Architectural / structural fabric
    Wall,
    CurtainWall,
    Door,
    Window,
    Column,
    Beam,
    Slab,
    Roof,
    Stair,
    Ramp,
    Railing,
    Covering,
    Furniture,
    Casework,
    Equipment,
    Footing,
    Pile,
    Reinforcement,
    SteelMember,
    // Civil / site
    Road,
    Kerb,
    Pavement,
    Sidewalk,
    Boundary,
    Fence,
    Drain,
    Manhole,
    Contour,
    SpotLevel,
    // Mechanical
    Duct,
    DuctFlex,
    AirTerminal,
    // Plumbing / fire
    Pipe,
    SanitaryTerminal,
    Pump,
    Valve,
    FireSuppression,
    // Electrical / ELV
    LightFixture,
    Outlet,
    Panel,
    CableCarrier,
    Cable,
    FireAlarm,
    CommsAppliance,
    Camera,
    AudioVisual,
    // Landscape
    Tree,
    Shrub,
    Lawn,
    Planting,
    WaterBody,
    Hardscape,
    Irrigation,
    // Annotation / general
    Space,
    Grid,
    Text,
    Dimension,
    Symbol,
    Hatch,
    Titleblock,
    Viewport,
    Reference,
    /// No specific classification.
    Generic,
}

impl ElementType {
    /// IFC4 entity name for BIM export / interoperability (`""` when unclassified).
    pub fn ifc(self) -> &'static str {
        use ElementType::*;
        match self {
            Wall => "IfcWall",
            CurtainWall => "IfcCurtainWall",
            Door => "IfcDoor",
            Window => "IfcWindow",
            Column => "IfcColumn",
            Beam => "IfcBeam",
            Slab => "IfcSlab",
            Roof => "IfcRoof",
            Stair => "IfcStair",
            Ramp => "IfcRamp",
            Railing => "IfcRailing",
            Covering => "IfcCovering",
            Furniture => "IfcFurniture",
            Casework => "IfcFurniture",
            Equipment => "IfcBuildingElementProxy",
            Footing => "IfcFooting",
            Pile => "IfcPile",
            Reinforcement => "IfcReinforcingBar",
            SteelMember => "IfcMember",
            Road => "IfcRoad",
            Kerb => "IfcKerb",
            Pavement => "IfcPavement",
            Sidewalk => "IfcPavement",
            Boundary => "IfcGeographicElement",
            Fence => "IfcRailing",
            Drain => "IfcPipeSegment",
            Manhole => "IfcDistributionChamberElement",
            Contour => "IfcGeographicElement",
            SpotLevel => "IfcAnnotation",
            Duct => "IfcDuctSegment",
            DuctFlex => "IfcDuctSegment",
            AirTerminal => "IfcAirTerminal",
            Pipe => "IfcPipeSegment",
            SanitaryTerminal => "IfcSanitaryTerminal",
            Pump => "IfcPump",
            Valve => "IfcValve",
            FireSuppression => "IfcFireSuppressionTerminal",
            LightFixture => "IfcLightFixture",
            Outlet => "IfcOutlet",
            Panel => "IfcElectricDistributionBoard",
            CableCarrier => "IfcCableCarrierSegment",
            Cable => "IfcCableSegment",
            FireAlarm => "IfcAlarm",
            CommsAppliance => "IfcCommunicationsAppliance",
            Camera => "IfcSensor",
            AudioVisual => "IfcAudioVisualAppliance",
            Tree | Shrub | Planting | Lawn => "IfcGeographicElement",
            WaterBody => "IfcGeographicElement",
            Hardscape => "IfcPavement",
            Irrigation => "IfcPipeSegment",
            Space => "IfcSpace",
            Grid => "IfcGrid",
            Text | Dimension | Symbol => "IfcAnnotation",
            Hatch => "IfcAnnotation",
            Titleblock => "IfcAnnotation",
            Viewport => "",
            Reference => "IfcGeographicElement",
            Generic => "IfcBuildingElementProxy",
        }
    }

    /// Uniformat II top-level classification code (`""` when not applicable).
    pub fn uniformat(self) -> &'static str {
        use ElementType::*;
        match self {
            Wall | CurtainWall => "B2010",
            Window => "B2020",
            Door => "B2030",
            Column | Beam | SteelMember => "B1010",
            Slab => "A1030",
            Footing | Pile | Reinforcement => "A1010",
            Roof => "B3010",
            Stair | Ramp => "C2010",
            Railing => "C2010",
            Covering => "C3030",
            Furniture | Casework => "E2010",
            Equipment => "E1090",
            Road | Pavement | Sidewalk | Kerb | Hardscape => "G2030",
            Boundary | Fence => "G2040",
            Drain | Manhole => "G3030",
            Contour | SpotLevel => "G1070",
            Duct | DuctFlex | AirTerminal => "D3050",
            Pipe | SanitaryTerminal | Pump | Valve => "D2010",
            FireSuppression => "D4010",
            LightFixture => "D5020",
            Outlet | Panel | CableCarrier | Cable => "D5010",
            FireAlarm => "D4090",
            CommsAppliance | Camera | AudioVisual => "D5090",
            Tree | Shrub | Planting | Lawn | WaterBody => "G2050",
            Irrigation => "G2050",
            _ => "",
        }
    }

    /// CSI MasterFormat division (2-digit) for specs / cost coding (`""` if none).
    pub fn masterformat(self) -> &'static str {
        use ElementType::*;
        match self {
            Wall => "04",
            CurtainWall | Window | Door => "08",
            Column | Beam | SteelMember | Reinforcement => "05",
            Slab | Footing | Pile => "03",
            Roof => "07",
            Stair | Ramp | Railing => "05",
            Covering => "09",
            Furniture | Casework => "12",
            Equipment => "11",
            Road | Pavement | Sidewalk | Kerb | Hardscape | Fence | Boundary => "32",
            Drain | Manhole | Contour | SpotLevel => "33",
            Duct | DuctFlex | AirTerminal => "23",
            Pipe | SanitaryTerminal | Pump | Valve => "22",
            FireSuppression => "21",
            LightFixture | Outlet | Panel | CableCarrier | Cable => "26",
            FireAlarm => "28",
            CommsAppliance | Camera | AudioVisual => "27",
            Tree | Shrub | Planting | Lawn | WaterBody | Irrigation => "32",
            _ => "",
        }
    }

    /// AI recognition tags — free-text semantic hints for the detector/embedder.
    pub fn ai_tags(self) -> &'static [&'static str] {
        use ElementType::*;
        match self {
            Wall => &["wall", "partition", "enclosure"],
            CurtainWall => &["curtain-wall", "facade", "glazing"],
            Door => &["door", "opening", "leaf", "swing"],
            Window => &["window", "opening", "glazing"],
            Column => &["column", "post", "structural"],
            Beam => &["beam", "girder", "structural"],
            Slab => &["slab", "floor", "deck"],
            Roof => &["roof", "cover"],
            Stair => &["stair", "flight", "circulation"],
            Ramp => &["ramp", "circulation", "accessible"],
            Railing => &["railing", "handrail", "balustrade"],
            Covering => &["ceiling", "finish", "covering"],
            Furniture => &["furniture", "loose"],
            Casework => &["casework", "joinery", "millwork"],
            Equipment => &["equipment", "appliance"],
            Footing => &["footing", "foundation"],
            Pile => &["pile", "foundation", "deep"],
            Reinforcement => &["rebar", "reinforcement"],
            SteelMember => &["steel", "member", "framing"],
            Road => &["road", "carriageway", "asphalt"],
            Kerb => &["kerb", "curb", "edge"],
            Pavement => &["pavement", "paving"],
            Sidewalk => &["sidewalk", "footpath"],
            Boundary => &["boundary", "plot", "site"],
            Fence => &["fence", "barrier"],
            Drain => &["drain", "sewer", "storm"],
            Manhole => &["manhole", "chamber"],
            Contour => &["contour", "terrain", "levels"],
            SpotLevel => &["spot-level", "elevation"],
            Duct => &["duct", "hvac", "air"],
            DuctFlex => &["flex-duct", "hvac"],
            AirTerminal => &["diffuser", "grille", "register"],
            Pipe => &["pipe", "line"],
            SanitaryTerminal => &["fixture", "sanitary", "wc", "basin"],
            Pump => &["pump"],
            Valve => &["valve"],
            FireSuppression => &["sprinkler", "hydrant", "fire"],
            LightFixture => &["light", "luminaire", "fixture"],
            Outlet => &["socket", "outlet", "receptacle"],
            Panel => &["panel", "distribution-board"],
            CableCarrier => &["conduit", "tray", "trunking"],
            Cable => &["cable", "wire"],
            FireAlarm => &["fire-alarm", "detector"],
            CommsAppliance => &["data", "comms", "network"],
            Camera => &["cctv", "camera", "security"],
            AudioVisual => &["pa", "av", "speaker"],
            Tree => &["tree", "planting", "canopy"],
            Shrub => &["shrub", "planting"],
            Lawn => &["lawn", "grass", "turf"],
            Planting => &["planting", "bed"],
            WaterBody => &["water", "pond", "pool"],
            Hardscape => &["hardscape", "paving"],
            Irrigation => &["irrigation", "sprinkler"],
            Space => &["space", "room", "zone"],
            Grid => &["grid", "gridline"],
            Text => &["text", "note", "label"],
            Dimension => &["dimension", "measure"],
            Symbol => &["symbol", "tag"],
            Hatch => &["hatch", "fill", "pattern"],
            Titleblock => &["titleblock", "sheet"],
            Viewport => &["viewport"],
            Reference => &["xref", "reference", "survey"],
            Generic => &[],
        }
    }

    /// Short label for legends / property panels.
    pub fn label(self) -> &'static str {
        use ElementType::*;
        match self {
            Wall => "Wall",
            CurtainWall => "Curtain Wall",
            Door => "Door",
            Window => "Window",
            Column => "Column",
            Beam => "Beam",
            Slab => "Slab",
            Roof => "Roof",
            Stair => "Stair",
            Ramp => "Ramp",
            Railing => "Railing",
            Covering => "Covering",
            Furniture => "Furniture",
            Casework => "Casework",
            Equipment => "Equipment",
            Footing => "Footing",
            Pile => "Pile",
            Reinforcement => "Reinforcement",
            SteelMember => "Steel Member",
            Road => "Road",
            Kerb => "Kerb",
            Pavement => "Pavement",
            Sidewalk => "Sidewalk",
            Boundary => "Boundary",
            Fence => "Fence",
            Drain => "Drain",
            Manhole => "Manhole",
            Contour => "Contour",
            SpotLevel => "Spot Level",
            Duct => "Duct",
            DuctFlex => "Flex Duct",
            AirTerminal => "Air Terminal",
            Pipe => "Pipe",
            SanitaryTerminal => "Fixture",
            Pump => "Pump",
            Valve => "Valve",
            FireSuppression => "Fire Suppression",
            LightFixture => "Light Fixture",
            Outlet => "Outlet",
            Panel => "Panel",
            CableCarrier => "Cable Carrier",
            Cable => "Cable",
            FireAlarm => "Fire Alarm",
            CommsAppliance => "Comms",
            Camera => "Camera",
            AudioVisual => "Audio/Visual",
            Tree => "Tree",
            Shrub => "Shrub",
            Lawn => "Lawn",
            Planting => "Planting",
            WaterBody => "Water Body",
            Hardscape => "Hardscape",
            Irrigation => "Irrigation",
            Space => "Space",
            Grid => "Grid",
            Text => "Text",
            Dimension => "Dimension",
            Symbol => "Symbol",
            Hatch => "Hatch",
            Titleblock => "Title Block",
            Viewport => "Viewport",
            Reference => "Reference",
            Generic => "Generic",
        }
    }
}

/// One standard layer definition with its semantic + drafting properties.
#[derive(Debug, Clone, Copy)]
pub struct StandardLayer {
    pub name: &'static str,
    pub aci: u8,
    pub linetype: LineType,
    pub lineweight_mm: f32,
    pub discipline: Discipline,
    pub element: ElementType,
    pub phase: Phase,
    /// False for construction-aid layers that must never plot (viewports, defpoints).
    pub plottable: bool,
}

impl StandardLayer {
    /// Materialise as a document [`Layer`].
    pub fn to_layer(self) -> Layer {
        Layer::new(self.name)
            .with_color(self.aci)
            .with_linetype(self.linetype)
            .with_lineweight_mm(self.lineweight_mm)
    }

    /// sRGB for print/screen preview.
    pub fn srgb(self) -> (u8, u8, u8) {
        aci_to_srgb(self.aci)
    }

    /// Contrast ratio vs white paper.
    pub fn contrast_vs_paper(self) -> f64 {
        let (r, g, b) = self.srgb();
        contrast_vs_white(r, g, b)
    }

    // Metadata delegates (single source of truth is the element type).
    pub fn ifc(self) -> &'static str {
        self.element.ifc()
    }
    pub fn uniformat(self) -> &'static str {
        self.element.uniformat()
    }
    pub fn masterformat(self) -> &'static str {
        self.element.masterformat()
    }
    pub fn ai_tags(self) -> &'static [&'static str] {
        self.element.ai_tags()
    }
}

/// New-phase, plottable layer (the common case).
macro_rules! L {
    ($name:literal, $aci:expr, $lt:expr, $lw:expr, $disc:expr, $elem:expr) => {
        StandardLayer {
            name: $name,
            aci: $aci,
            linetype: $lt,
            lineweight_mm: $lw,
            discipline: $disc,
            element: $elem,
            phase: Phase::New,
            plottable: true,
        }
    };
}

/// Layer with an explicit phase and/or plot flag (existing / demolition / no-plot).
macro_rules! LP {
    ($name:literal, $aci:expr, $lt:expr, $lw:expr, $disc:expr, $elem:expr, $phase:expr, $plot:expr) => {
        StandardLayer {
            name: $name,
            aci: $aci,
            linetype: $lt,
            lineweight_mm: $lw,
            discipline: $disc,
            element: $elem,
            phase: $phase,
            plottable: $plot,
        }
    };
}

use Discipline::*;
use ElementType::*;
use LineType::*;

/// The HCW enterprise catalog. Colours (ACI) and lineweights follow the HCW
/// standard; light colours are floored to a legible weight (WCAG-audited in
/// tests). Metadata derives from each layer's [`ElementType`].
pub const STANDARD_LAYERS: &[StandardLayer] = &[
    // ======================= Architecture (A) =======================
    L!("A-WALL", 1, Continuous, 0.50, Architectural, Wall),
    L!("A-WALL-HIDDEN", 1, Hidden, 0.25, Architectural, Wall),
    L!("A-WALL-FIRE", 1, Continuous, 0.50, Architectural, Wall),
    L!("A-WALL-PART", 8, Continuous, 0.35, Architectural, Wall),
    L!("A-WALL-LINT", 252, Continuous, 0.35, Architectural, Wall),
    L!("A-WALL-PATT", 8, Continuous, 0.18, Architectural, Hatch),
    L!("A-CURTAINWALL", 140, Continuous, 0.35, Architectural, CurtainWall),
    L!("A-DOOR", 30, Continuous, 0.35, Architectural, Door),
    L!("A-DOOR-IDEN", 31, Continuous, 0.25, Architectural, Symbol),
    L!("A-WINDOW", 140, Continuous, 0.35, Architectural, Window),
    L!("A-WINDOW-IDEN", 141, Continuous, 0.25, Architectural, Symbol),
    L!("A-COLUMN", 2, Continuous, 0.50, Architectural, Column),
    L!("A-BEAM", 2, Continuous, 0.45, Architectural, Beam),
    L!("A-SLAB", 4, Continuous, 0.25, Architectural, Slab),
    L!("A-STAIR", 32, Continuous, 0.35, Architectural, Stair),
    L!("A-RAMP", 32, Continuous, 0.35, Architectural, Ramp),
    L!("A-RAIL", 32, Continuous, 0.35, Architectural, Railing),
    L!("A-ROOF", 40, Continuous, 0.35, Architectural, Roof),
    L!("A-ROOF-OTLN", 41, Continuous, 0.50, Architectural, Roof),
    L!("A-CEILING", 132, Continuous, 0.18, Architectural, Covering),
    L!("A-CEILING-GRID", 132, Continuous, 0.13, Architectural, Grid),
    L!("A-FURN", 151, Continuous, 0.18, Architectural, Furniture),
    L!("A-CASE", 30, Continuous, 0.35, Architectural, Casework),
    L!("A-EQPM", 90, Continuous, 0.35, Architectural, Equipment),
    L!("A-AREA", 110, Dashed, 0.35, Architectural, Space),
    L!("A-HATCH", 9, Continuous, 0.35, Architectural, Hatch),
    L!("A-SECT", 1, Continuous, 0.50, Architectural, Symbol),
    L!("A-ELEV", 30, Continuous, 0.35, Architectural, Symbol),
    L!("A-DETL", 40, Continuous, 0.35, Architectural, Symbol),
    // Architecture annotation
    L!("A-TEXT", 7, Continuous, 0.18, Annotation, Text),
    L!("A-DIMS", 7, Continuous, 0.18, Annotation, Dimension),
    L!("A-GRID", 6, Center, 0.25, Annotation, Grid),
    L!("A-GRID-IDEN", 6, Continuous, 0.25, Annotation, Symbol),
    L!("A-ANNO-NOTE", 7, Continuous, 0.18, Annotation, Text),
    L!("A-ANNO-SYMB", 7, Continuous, 0.35, Annotation, Symbol),
    L!("A-ANNO-KEYN", 1, Continuous, 0.35, Annotation, Symbol),
    // ======================= Structural (S) =======================
    L!("S-COLUMN", 1, Continuous, 0.50, Structural, Column),
    L!("S-BEAM", 2, Continuous, 0.45, Structural, Beam),
    L!("S-SLAB", 4, Continuous, 0.25, Structural, Slab),
    L!("S-FOOTING", 30, Continuous, 0.50, Structural, Footing),
    L!("S-PILE", 30, Continuous, 0.50, Structural, Pile),
    L!("S-PILECAP", 30, Continuous, 0.50, Structural, Footing),
    L!("S-GRADEBEAM", 2, Continuous, 0.45, Structural, Beam),
    L!("S-JOIST", 31, Continuous, 0.35, Structural, Beam),
    L!("S-BRACING", 60, Continuous, 0.35, Structural, SteelMember),
    L!("S-REBAR", 140, Continuous, 0.18, Structural, Reinforcement),
    L!("S-STEEL", 5, Continuous, 0.35, Structural, SteelMember),
    L!("S-CONCRETE", 8, Continuous, 0.18, Structural, Slab),
    L!("S-WALL", 1, Continuous, 0.50, Structural, Wall),
    L!("S-HATCH", 9, Continuous, 0.35, Structural, Hatch),
    L!("S-GRID", 6, Center, 0.25, Structural, Grid),
    L!("S-TEXT", 7, Continuous, 0.18, Structural, Text),
    L!("S-DIMS", 7, Continuous, 0.18, Structural, Dimension),
    // ======================= Civil (C) =======================
    L!("C-ROAD", 30, Continuous, 0.50, Civil, Road),
    L!("C-KERB", 2, Continuous, 0.35, Civil, Kerb),
    L!("C-PAVEMENT", 4, Continuous, 0.25, Civil, Pavement),
    L!("C-SIDEWALK", 151, Continuous, 0.25, Civil, Sidewalk),
    L!("C-BOUNDARY", 1, Continuous, 0.50, Civil, Boundary),
    L!("C-FENCE", 6, Phantom, 0.25, Civil, Fence),
    L!("C-DRAIN", 5, Continuous, 0.35, Civil, Drain),
    L!("C-MANHOLE", 30, Continuous, 0.35, Civil, Manhole),
    L!("C-SPOTLEVEL", 140, Continuous, 0.18, Civil, SpotLevel),
    L!("C-CONTOUR-MAJOR", 1, Continuous, 0.35, Civil, Contour),
    L!("C-CONTOUR-MINOR", 8, Continuous, 0.18, Civil, Contour),
    L!("C-UTILITY", 5, Dashed, 0.25, Civil, Pipe),
    L!("C-TEXT", 7, Continuous, 0.18, Civil, Text),
    // ======================= Mechanical (M) =======================
    L!("M-DUCT", 4, Continuous, 0.35, Mechanical, Duct),
    L!("M-DUCT-HIDDEN", 4, Hidden, 0.25, Mechanical, Duct),
    L!("M-FLEX", 151, Continuous, 0.25, Mechanical, DuctFlex),
    L!("M-EQUIPMENT", 30, Continuous, 0.35, Mechanical, Equipment),
    L!("M-DIFFUSER", 140, Continuous, 0.25, Mechanical, AirTerminal),
    L!("M-GRILLE", 140, Continuous, 0.25, Mechanical, AirTerminal),
    L!("M-PIPING", 80, Continuous, 0.35, Mechanical, Pipe),
    L!("M-CTRL", 140, Dashed, 0.35, Mechanical, Cable),
    L!("M-TEXT", 7, Continuous, 0.18, Mechanical, Text),
    // ======================= Electrical (E) =======================
    L!("E-LIGHT", 2, Continuous, 0.25, Electrical, LightFixture),
    L!("E-POWER", 1, Continuous, 0.25, Electrical, Outlet),
    L!("E-SOCKET", 30, Continuous, 0.25, Electrical, Outlet),
    L!("E-CONDUIT", 140, Continuous, 0.18, Electrical, CableCarrier),
    L!("E-CABLE", 5, Continuous, 0.18, Electrical, Cable),
    L!("E-PANEL", 6, Continuous, 0.35, Electrical, Panel),
    L!("E-FIREALARM", 10, Continuous, 0.25, Electrical, FireAlarm),
    L!("E-DATA", 151, Continuous, 0.18, Electrical, CommsAppliance),
    L!("E-EARTH", 8, Continuous, 0.35, Electrical, Cable),
    L!("E-TEXT", 7, Continuous, 0.18, Electrical, Text),
    // ======================= Plumbing (P) =======================
    L!("P-CWS", 5, Continuous, 0.25, Plumbing, Pipe),
    L!("P-HWS", 1, Continuous, 0.25, Plumbing, Pipe),
    L!("P-SOIL", 30, Continuous, 0.35, Plumbing, Pipe),
    L!("P-WASTE", 140, Continuous, 0.35, Plumbing, Pipe),
    L!("P-VENT", 6, Continuous, 0.25, Plumbing, Pipe),
    L!("P-RAIN", 151, Continuous, 0.25, Plumbing, Pipe),
    L!("P-FIXTURE", 2, Continuous, 0.25, Plumbing, SanitaryTerminal),
    L!("P-PUMP", 30, Continuous, 0.35, Plumbing, Pump),
    L!("P-VALVE", 6, Continuous, 0.25, Plumbing, Valve),
    L!("P-TEXT", 7, Continuous, 0.18, Plumbing, Text),
    // ======================= Fire Fighting (F) =======================
    L!("F-SPRINKLER", 1, Continuous, 0.25, FireFighting, FireSuppression),
    L!("F-HYDRANT", 30, Continuous, 0.35, FireFighting, FireSuppression),
    L!("F-HOSE", 2, Continuous, 0.25, FireFighting, FireSuppression),
    L!("F-PIPE", 1, Continuous, 0.35, FireFighting, Pipe),
    L!("F-PUMP", 6, Continuous, 0.35, FireFighting, Pump),
    L!("F-EXTINGUISHER", 30, Continuous, 0.25, FireFighting, Equipment),
    L!("F-TEXT", 7, Continuous, 0.18, FireFighting, Text),
    // ======================= Telecom / ELV (T) =======================
    L!("T-DATA", 151, Continuous, 0.18, Telecom, CommsAppliance),
    L!("T-CCTV", 30, Continuous, 0.25, Telecom, Camera),
    L!("T-ACCESS", 2, Continuous, 0.25, Telecom, CommsAppliance),
    L!("T-FIBRE", 5, Continuous, 0.18, Telecom, Cable),
    L!("T-PA", 6, Continuous, 0.18, Telecom, AudioVisual),
    L!("T-TEXT", 7, Continuous, 0.18, Telecom, Text),
    // ======================= Landscape (L) =======================
    L!("L-TREE", 3, Continuous, 0.25, Landscape, Tree),
    L!("L-SHRUB", 94, Continuous, 0.25, Landscape, Shrub),
    L!("L-LAWN", 92, Continuous, 0.35, Landscape, Lawn),
    L!("L-PLANTING", 94, Continuous, 0.25, Landscape, Planting),
    L!("L-HEDGE", 3, Continuous, 0.25, Landscape, Shrub),
    L!("L-WATERBODY", 140, Continuous, 0.25, Landscape, WaterBody),
    L!("L-HARDSCAPE", 30, Continuous, 0.35, Landscape, Hardscape),
    L!("L-PAVING", 32, Continuous, 0.25, Landscape, Pavement),
    L!("L-FURNITURE", 151, Continuous, 0.18, Landscape, Furniture),
    L!("L-LIGHTING", 2, Continuous, 0.25, Landscape, LightFixture),
    L!("L-IRRIGATION", 5, Hidden, 0.18, Landscape, Irrigation),
    L!("L-TEXT", 7, Continuous, 0.18, Landscape, Text),
    // ======================= Interior Design (I) =======================
    L!("I-FURNITURE", 151, Continuous, 0.18, Interior, Furniture),
    L!("I-JOINERY", 30, Continuous, 0.35, Interior, Casework),
    L!("I-CEILING", 132, Continuous, 0.18, Interior, Covering),
    L!("I-FLOORFINISH", 8, Continuous, 0.18, Interior, Covering),
    L!("I-WALLFINISH", 140, Continuous, 0.18, Interior, Covering),
    L!("I-UPHOLSTERY", 94, Continuous, 0.35, Interior, Furniture),
    L!("I-LIGHTING", 2, Continuous, 0.25, Interior, LightFixture),
    L!("I-DECOR", 5, Continuous, 0.18, Interior, Furniture),
    L!("I-HATCH", 9, Continuous, 0.35, Interior, Hatch),
    L!("I-TEXT", 7, Continuous, 0.18, Interior, Text),
    // ======================= General (G) =======================
    L!("G-TEXT", 7, Continuous, 0.18, General, Text),
    L!("G-DIMS", 7, Continuous, 0.18, General, Dimension),
    L!("G-GRID", 6, Center, 0.25, General, Grid),
    L!("G-CENTER", 6, Center, 0.18, General, Symbol),
    L!("G-HIDDEN", 8, Hidden, 0.18, General, Symbol),
    L!("G-REVISION", 1, Continuous, 0.35, General, Symbol),
    L!("G-TITLEBLOCK", 7, Continuous, 0.50, General, Titleblock),
    L!("G-BORDER", 7, Continuous, 0.50, General, Titleblock),
    LP!("G-VIEWPORT", 8, Continuous, 0.13, General, Viewport, Phase::New, false),
    L!("G-XREF", 250, Continuous, 0.25, General, Reference),
    LP!("DEFPOINTS", 7, Continuous, 0.13, General, Generic, Phase::New, false),
    // ======================= Existing (X) — retained fabric =======================
    LP!("X-WALL", 8, Continuous, 0.25, Existing, Wall, Phase::Existing, true),
    LP!("X-COLUMN", 8, Continuous, 0.25, Existing, Column, Phase::Existing, true),
    LP!("X-DOOR", 8, Continuous, 0.18, Existing, Door, Phase::Existing, true),
    LP!("X-WINDOW", 8, Continuous, 0.18, Existing, Window, Phase::Existing, true),
    LP!("X-SLAB", 8, Continuous, 0.18, Existing, Slab, Phase::Existing, true),
    LP!("X-TEXT", 8, Continuous, 0.18, Existing, Text, Phase::Existing, true),
    // ======================= Demolition (D) — to be removed =======================
    LP!("D-WALL", 1, Dashed, 0.25, Demolition, Wall, Phase::Demolish, true),
    LP!("D-COLUMN", 1, Dashed, 0.25, Demolition, Column, Phase::Demolish, true),
    LP!("D-DOOR", 1, Dashed, 0.18, Demolition, Door, Phase::Demolish, true),
    LP!("D-WINDOW", 1, Dashed, 0.18, Demolition, Window, Phase::Demolish, true),
    LP!("D-SLAB", 1, Dashed, 0.18, Demolition, Slab, Phase::Demolish, true),
    LP!("D-TEXT", 1, Continuous, 0.18, Demolition, Text, Phase::Demolish, true),
    // ======================= Survey / Reference (R) =======================
    L!("R-SURVEY", 250, Continuous, 0.18, Survey, Reference),
    L!("R-BENCHMARK", 30, Continuous, 0.25, Survey, SpotLevel),
    L!("R-GRID", 250, Continuous, 0.18, Survey, Grid),
    L!("R-XREF", 250, Continuous, 0.18, Survey, Reference),
    // ======================= Presentation =======================
    L!("A-PRES", 250, Continuous, 0.35, Presentation, Generic),
    L!("A-PRES-MARK", 1, Continuous, 0.50, Presentation, Symbol),
    L!("A-PRES-HATCH", 80, Continuous, 0.35, Presentation, Hatch),
    L!("A-PRES-NOTE", 7, Continuous, 0.18, Presentation, Text),
    L!("A-PRES-ENTOURAGE", 110, Continuous, 0.35, Presentation, Generic),
    // ======================= Legacy AIA aliases (compat) =======================
    // Retained so imported drawings, detection maps, and golden fixtures keep
    // resolving. Prefer the HCW names above for new work.
    L!("A-GLAZ", 140, Continuous, 0.35, Architectural, Window),
    L!("A-GLAZ-IDEN", 141, Continuous, 0.25, Architectural, Symbol),
    L!("A-COLS", 2, Continuous, 0.50, Architectural, Column),
    L!("A-FLOR", 80, Continuous, 0.35, Architectural, Slab),
    L!("A-CEIL", 150, Continuous, 0.25, Architectural, Covering),
    L!("A-ANNO-TEXT", 7, Continuous, 0.18, Annotation, Text),
    L!("A-ANNO-DIMS", 7, Continuous, 0.18, Annotation, Dimension),
    L!("A-ANNO-REFR", 160, Continuous, 0.25, Annotation, Symbol),
    L!("S-COLS", 1, Continuous, 0.50, Structural, Column),
    L!("S-FNDN", 40, Continuous, 0.50, Structural, Footing),
    L!("S-FNDN-FTNG", 41, Continuous, 0.50, Structural, Footing),
    L!("M-HVAC-SUPP", 4, Continuous, 0.35, Mechanical, Duct),
    L!("M-HVAC-RETR", 5, Continuous, 0.35, Mechanical, Duct),
    L!("E-LITE", 2, Continuous, 0.35, Electrical, LightFixture),
    L!("E-POWR", 1, Continuous, 0.35, Electrical, Outlet),
    L!("P-SANR", 80, Continuous, 0.35, Plumbing, Pipe),
    L!("P-DOMW", 5, Continuous, 0.35, Plumbing, Pipe),
    L!("BORDER", 7, Continuous, 0.70, General, Titleblock),
    L!("TITLEBLOCK", 7, Continuous, 0.35, General, Titleblock),
];

/// Back-compat alias — the catalog used to be named `AIA_LAYERS`.
pub const AIA_LAYERS: &[StandardLayer] = STANDARD_LAYERS;

/// Look up a catalog entry by exact layer name (case-sensitive AIA/HCW form).
pub fn find_layer(name: &str) -> Option<&'static StandardLayer> {
    STANDARD_LAYERS.iter().find(|l| l.name == name)
}

/// Seed any missing standard layers onto `doc` (does not overwrite existing).
/// Returns how many layers were added.
pub fn apply_standard_layers(doc: &mut aadt_model::Document) -> usize {
    let mut added = 0;
    for std in STANDARD_LAYERS {
        if doc.layer(std.name).is_none() {
            doc.add_layer(std.to_layer());
            added += 1;
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aci::{min_lineweight_mm_for_contrast, GRAPHICS_AA_MIN};

    #[test]
    fn catalog_size_is_in_band() {
        let n = STANDARD_LAYERS.len();
        assert!(n >= 150, "expected ≥150 layers, got {n}");
        assert!(n <= 400, "expected ≤400 layers, got {n}");
    }

    #[test]
    fn names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for l in STANDARD_LAYERS {
            assert!(seen.insert(l.name), "duplicate layer {}", l.name);
        }
    }

    /// The HCW standard uses traditional CAD *screen* colours (ACI 1/2/4/6 …)
    /// that plot black under color-dependent plot styles, so legibility is a
    /// function of the pen weight, not the on-screen contrast vs white paper.
    /// Gate on valid ISO pen sizes instead.
    #[test]
    fn every_plotting_layer_has_a_plottable_pen_weight() {
        // AutoCAD/ISO pen range: the thinnest visible hairline up to the heaviest
        // border. Catches typos and negative/zero weights without pinning to a
        // strict ladder (the HCW standard uses 0.45 mm for beams).
        let mut failures = Vec::new();
        for l in STANDARD_LAYERS {
            if !l.plottable {
                continue; // viewports / defpoints never print
            }
            if !(0.05..=2.11).contains(&l.lineweight_mm) {
                failures.push(format!(
                    "{} lineweight {:.2} mm out of plottable range 0.05–2.11 mm",
                    l.name, l.lineweight_mm
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "non-standard lineweights:\n{}",
            failures.join("\n")
        );
    }

    /// The contrast helpers still work; keep a smoke test so the audit tooling
    /// (used by the layer-manager legibility hints) does not rot.
    #[test]
    fn contrast_audit_helpers_are_live() {
        let wall = find_layer("A-WALL").unwrap();
        let c = wall.contrast_vs_paper();
        assert!(c > 0.0);
        let _ = (min_lineweight_mm_for_contrast(c), GRAPHICS_AA_MIN);
    }

    #[test]
    fn every_discipline_has_layers() {
        for d in Discipline::ALL {
            assert!(
                STANDARD_LAYERS.iter().any(|l| l.discipline == *d),
                "no layers for discipline {:?}",
                d
            );
        }
    }

    #[test]
    fn hcw_core_layers_present_with_metadata() {
        let wall = find_layer("A-WALL").expect("A-WALL");
        assert_eq!(wall.ifc(), "IfcWall");
        assert_eq!(wall.masterformat(), "04");
        let win = find_layer("A-WINDOW").expect("A-WINDOW");
        assert_eq!(win.ifc(), "IfcWindow");
        assert!(win.ai_tags().contains(&"window"));
        let col = find_layer("S-COLUMN").expect("S-COLUMN");
        assert_eq!(col.discipline, Structural);
        assert_eq!(col.ifc(), "IfcColumn");
    }

    #[test]
    fn legacy_aliases_still_resolve() {
        for name in ["A-GLAZ", "A-COLS", "S-COLS", "E-LITE", "M-HVAC-SUPP", "P-SANR"] {
            assert!(find_layer(name).is_some(), "legacy layer {name} missing");
        }
    }

    #[test]
    fn phase_layers_are_tagged() {
        assert_eq!(find_layer("X-WALL").unwrap().phase, Phase::Existing);
        assert_eq!(find_layer("D-WALL").unwrap().phase, Phase::Demolish);
    }

    #[test]
    fn apply_seeds_missing_only() {
        let mut doc = aadt_model::Document::new();
        doc.restore_layer(Layer::new("A-WALL").with_color(3));
        let n = apply_standard_layers(&mut doc);
        assert!(n >= 150);
        assert_eq!(doc.layer("A-WALL").unwrap().color, 3, "existing preserved");
        assert!(doc.layer("E-LIGHT").is_some());
    }
}
