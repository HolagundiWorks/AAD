//! Professional title block — built-in nameplate presets, variable attributes.
//!
//! Layout matches a standard landscape architectural sheet: a **vertical strip
//! on the right** (fraction of the border interior width depends on
//! [`NameplatePreset`], full height). Geometry and layers are identical on every
//! sheet for a given preset; only attribute values change.
//!
//! Presets: **Professional** (full plate), **Compact** (tighter strip),
//! **Minimal** (firm / title / number / rev / scale / date). A freeform template
//! designer is out of scope (see ADR-0006).
//!
//! Layers (company standard):
//! - [`LAYER_TITLEBLOCK`] — title-block linework
//! - [`LAYER_TB_TEXT`] — static labels
//! - [`LAYER_TB_ATTR`] — editable attribute values
//! - [`LAYER_REVISION`] — revision table
//! - [`LAYER_LOGO`] — logo cell
//! - Sheet border uses [`crate::sheet::SHEET_LAYER`] (`BORDER`)

use aadt_geometry::{Point2, Segment};
use aadt_model::{Document, DrawingProperties, EntityId, Geometry, Text};

use crate::sheet::{Orientation, Sheet};

/// Title-block layout driven by sheet orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleBlockLayout {
    /// Landscape sheets: vertical strip on the right (~16% width).
    VerticalRight,
    /// Portrait sheets: horizontal band along the bottom (~16% height).
    HorizontalBottom,
}

impl TitleBlockLayout {
    pub fn for_orientation(orientation: Orientation) -> Self {
        match orientation {
            Orientation::Landscape => Self::VerticalRight,
            Orientation::Portrait => Self::HorizontalBottom,
        }
    }
}

/// Built-in nameplate density / content presets (not a freeform designer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NameplatePreset {
    /// Full professional plate (default).
    #[default]
    Professional,
    /// Same fields, tighter strip and slightly smaller type.
    Compact,
    /// Firm, title, drawing number, revision, scale, date only.
    Minimal,
}

impl NameplatePreset {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "professional" | "full" => Some(Self::Professional),
            "compact" => Some(Self::Compact),
            "minimal" | "min" => Some(Self::Minimal),
            _ => None,
        }
    }

    pub fn as_token(self) -> &'static str {
        match self {
            Self::Professional => "professional",
            Self::Compact => "compact",
            Self::Minimal => "minimal",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Professional => "Professional",
            Self::Compact => "Compact",
            Self::Minimal => "Minimal",
        }
    }

    /// Strip / band fraction of the sheet interior.
    pub fn strip_frac(self) -> f64 {
        match self {
            Self::Professional => STRIP_WIDTH_FRAC,
            Self::Compact => 0.12,
            Self::Minimal => 0.10,
        }
    }

    pub fn text_scale(self) -> f64 {
        match self {
            Self::Professional => 1.0,
            Self::Compact => 0.9,
            Self::Minimal => 0.95,
        }
    }
}

/// Title-block linework.
pub const LAYER_TITLEBLOCK: &str = "TITLEBLOCK";
/// Static labels (PROJECT:, DRAWN BY:, …).
pub const LAYER_TB_TEXT: &str = "TB_TEXT";
/// Editable attribute values.
pub const LAYER_TB_ATTR: &str = "TB_ATTR";
/// Revision table geometry + text.
pub const LAYER_REVISION: &str = "REVISION";
/// Logo cell.
pub const LAYER_LOGO: &str = "LOGO";

/// Legacy alias used by older callers / DXF round-trips.
pub const TITLEBLOCK_LAYER: &str = "A-TTLB";

/// Fraction of the sheet interior width occupied by the title strip (15–18%).
pub const STRIP_WIDTH_FRAC: f64 = 0.16;
/// Gap (mm) between drawing area and title strip.
pub const STRIP_GAP_MM: f64 = 2.0;

/// Standard text heights in paper millimetres.
pub const HT_TITLE: f64 = 5.0;
pub const HT_LABEL: f64 = 3.5;
pub const HT_VALUE: f64 = 2.5;
pub const HT_SHEET_NO: f64 = 10.0;

/// One revision-table row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RevisionRow {
    pub rev: String,
    pub date: String,
    pub description: String,
    pub by: String,
    pub checked: String,
}

/// The information recorded in a sheet's title block (attribute set).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TitleBlock {
    pub company_name: String,
    pub firm_tagline: String,
    pub phone: String,
    pub email: String,
    pub office_address: String,
    pub website: String,
    pub logo_path: String,
    pub project: String,
    pub client: String,
    pub address: String,
    pub project_number: String,
    pub discipline: String,
    /// Drawing / sheet content title (e.g. Ground Floor Plan).
    pub sheet_title: String,
    /// Large drawing number (e.g. `A-101`).
    pub drawing_number: String,
    /// Sheet index text (e.g. `01 of 45` or `1/2`).
    pub sheet_number: String,
    pub total_sheets: String,
    pub scale: String,
    pub date: String,
    pub drawn_by: String,
    pub checked_by: String,
    pub approved_by: String,
    pub revision: String,
    pub drawing_status: String,
    pub architect: String,
    pub area: String,
    pub revision_rows: Vec<RevisionRow>,
    /// Firm/project-custom rows (legacy).
    pub extra: Vec<(String, String)>,
    /// Built-in nameplate preset.
    pub preset: NameplatePreset,
}

impl TitleBlock {
    /// Fields a sheet must carry to be considered complete (used by QA).
    pub const REQUIRED: &'static [&'static str] =
        &["Project", "Drawing Title", "Sheet Number", "Scale"];

    /// Build from Drawing Property Manager fields.
    /// `drawing_name` overrides the drawing title when non-empty (asked at plot).
    pub fn from_properties(props: &DrawingProperties, drawing_name: &str) -> Self {
        let title = if drawing_name.trim().is_empty() {
            if !props.title.trim().is_empty() {
                props.title.clone()
            } else {
                props.subject.clone()
            }
        } else {
            drawing_name.trim().to_string()
        };
        let drawn = if props.drawn_by.trim().is_empty() {
            props.author.clone()
        } else {
            props.drawn_by.clone()
        };
        let sheet_no = if props.sheet_number.trim().is_empty() {
            if props.total_sheets.trim().is_empty() {
                String::new()
            } else {
                format!("1 of {}", props.total_sheets.trim())
            }
        } else {
            props.sheet_number.clone()
        };
        Self {
            company_name: props.firm_name.clone(),
            firm_tagline: props.firm_tagline.clone(),
            phone: props.phone.clone(),
            email: props.email.clone(),
            office_address: props.office_address.clone(),
            website: props.website.clone(),
            logo_path: props.logo_path.clone(),
            project: props.project.clone(),
            client: props.client.clone(),
            address: props.address.clone(),
            project_number: props.project_number.clone(),
            discipline: props.discipline.clone(),
            sheet_title: title,
            drawing_number: props.drawing_number.clone(),
            sheet_number: sheet_no,
            total_sheets: props.total_sheets.clone(),
            scale: props.scale.clone(),
            date: props.date.clone(),
            drawn_by: drawn,
            checked_by: props.checked_by.clone(),
            approved_by: props.approved_by.clone(),
            revision: props.revision.clone(),
            drawing_status: props.drawing_status.clone(),
            architect: if props.architect.trim().is_empty() {
                props.author.clone()
            } else {
                props.architect.clone()
            },
            area: props.area.clone(),
            revision_rows: parse_revision_history(&props.revision_history),
            extra: Vec::new(),
            preset: NameplatePreset::parse(&props.nameplate_preset)
                .unwrap_or(NameplatePreset::Professional),
        }
    }

    /// Strip width as a fraction of the sheet interior (fixed for the set).
    pub const STRIP_FRAC: f64 = STRIP_WIDTH_FRAC;

    /// Legacy plate constants (kept for callers that still mention them).
    pub const PLATE_HEIGHT: f64 = 56.0;
    pub const PLATE_WIDTH: f64 = 210.0;

    /// Width (mm) of the vertical title strip (landscape).
    pub fn strip_width_mm(sheet: &Sheet) -> f64 {
        Self::strip_width_mm_for(sheet, NameplatePreset::Professional)
    }

    /// Strip width for a nameplate preset.
    pub fn strip_width_mm_for(sheet: &Sheet, preset: NameplatePreset) -> f64 {
        let (min, max) = sheet.inner();
        ((max.x - min.x) * preset.strip_frac()).clamp(40.0, 120.0)
    }

    /// Height (mm) of the horizontal title band (portrait).
    pub fn strip_height_mm(sheet: &Sheet) -> f64 {
        Self::strip_height_mm_for(sheet, NameplatePreset::Professional)
    }

    pub fn strip_height_mm_for(sheet: &Sheet, preset: NameplatePreset) -> f64 {
        let (min, max) = sheet.inner();
        ((max.y - min.y) * preset.strip_frac()).clamp(36.0, 90.0)
    }

    /// Layout for this sheet's orientation.
    pub fn layout_for(sheet: &Sheet) -> TitleBlockLayout {
        TitleBlockLayout::for_orientation(sheet.orientation)
    }

    /// Rows as `(label, value)` — QA / legacy.
    pub fn rows(&self) -> Vec<(String, String)> {
        let mut rows = vec![
            ("Project".to_string(), self.project.clone()),
            ("Drawing Title".to_string(), self.sheet_title.clone()),
            ("Client".to_string(), self.client.clone()),
            ("Address".to_string(), self.address.clone()),
            ("Architect".to_string(), self.architect.clone()),
            ("Date".to_string(), self.date.clone()),
            ("Area".to_string(), self.area.clone()),
            ("Scale".to_string(), self.scale.clone()),
            ("Sheet Number".to_string(), self.sheet_id_display()),
            ("Drawn By".to_string(), self.drawn_by.clone()),
            ("Checked By".to_string(), self.checked_by.clone()),
            ("Approved By".to_string(), self.approved_by.clone()),
            ("Revision".to_string(), self.revision.clone()),
            ("Firm".to_string(), self.company_name.clone()),
            ("Discipline".to_string(), self.discipline.clone()),
            ("Status".to_string(), self.drawing_status.clone()),
            ("Project Number".to_string(), self.project_number.clone()),
        ];
        rows.extend(self.extra.iter().cloned());
        rows
    }

    /// Large corner ID: prefers `drawing_number`, else `sheet_number`.
    pub fn sheet_id_display(&self) -> String {
        if !self.drawing_number.trim().is_empty() {
            self.drawing_number.clone()
        } else if !self.sheet_number.trim().is_empty() {
            self.sheet_number.clone()
        } else {
            "A-101".into()
        }
    }

    pub fn field(&self, label: &str) -> Option<&str> {
        match label {
            "Project" => Some(&self.project),
            "Drawing Title" | "Sheet Title" => Some(&self.sheet_title),
            "Sheet Number" => {
                if !self.drawing_number.is_empty() {
                    Some(self.drawing_number.as_str())
                } else {
                    Some(self.sheet_number.as_str())
                }
            }
            "Date" => Some(&self.date),
            "Scale" => Some(&self.scale),
            "Drawn By" => Some(&self.drawn_by),
            "Checked By" => Some(&self.checked_by),
            "Approved By" => Some(&self.approved_by),
            "Revision" => Some(&self.revision),
            "Client" => Some(&self.client),
            "Address" => Some(&self.address),
            "Architect" => Some(&self.architect),
            "Area" => Some(&self.area),
            "Firm" | "Company" => Some(&self.company_name),
            "Discipline" => Some(&self.discipline),
            "Status" => Some(&self.drawing_status),
            "Project Number" => Some(&self.project_number),
            other => self
                .extra
                .iter()
                .find(|(l, _)| l == other)
                .map(|(_, v)| v.as_str()),
        }
    }

    /// Draw the title block into `doc` at `origin` (bottom-left of the plate).
    pub fn place(&self, doc: &mut Document, style: &TitleBlockStyle) -> Vec<EntityId> {
        doc.transact("TITLEBLOCK", |d| match (style.layout, style.preset) {
            (TitleBlockLayout::VerticalRight, NameplatePreset::Minimal) => {
                emit_strip_minimal(d, self, style)
            }
            (TitleBlockLayout::HorizontalBottom, NameplatePreset::Minimal) => {
                emit_band_minimal(d, self, style)
            }
            (TitleBlockLayout::VerticalRight, _) => emit_strip(d, self, style),
            (TitleBlockLayout::HorizontalBottom, _) => emit_band(d, self, style),
        })
    }

    /// Place the title block for `sheet`: right strip (landscape) or bottom band (portrait).
    pub fn place_on_sheet(&self, doc: &mut Document, sheet: &Sheet) -> Vec<EntityId> {
        let (min, max) = sheet.inner();
        let layout = Self::layout_for(sheet);
        let scale = self.preset.text_scale();
        let style = match layout {
            TitleBlockLayout::VerticalRight => {
                let width = Self::strip_width_mm_for(sheet, self.preset);
                TitleBlockStyle {
                    origin: Point2::new(max.x - width, min.y),
                    width,
                    height: max.y - min.y,
                    text_height: HT_VALUE * scale,
                    text_height_large: HT_SHEET_NO * scale,
                    layout,
                    preset: self.preset,
                }
            }
            TitleBlockLayout::HorizontalBottom => {
                let height = Self::strip_height_mm_for(sheet, self.preset);
                TitleBlockStyle {
                    origin: Point2::new(min.x, min.y),
                    width: max.x - min.x,
                    height,
                    text_height: HT_VALUE * scale,
                    text_height_large: HT_SHEET_NO * scale,
                    layout,
                    preset: self.preset,
                }
            }
        };
        self.place(doc, &style)
    }
}

/// Placement and sizing for the title plate, in millimetres on the sheet.
#[derive(Debug, Clone)]
pub struct TitleBlockStyle {
    /// Bottom-left corner of the plate.
    pub origin: Point2,
    pub width: f64,
    pub height: f64,
    pub text_height: f64,
    pub text_height_large: f64,
    pub layout: TitleBlockLayout,
    pub preset: NameplatePreset,
}

impl Default for TitleBlockStyle {
    fn default() -> Self {
        Self {
            origin: Point2::new(0.0, 0.0),
            width: 70.0,
            height: 277.0,
            text_height: HT_VALUE,
            text_height_large: HT_SHEET_NO,
            layout: TitleBlockLayout::VerticalRight,
            preset: NameplatePreset::Professional,
        }
    }
}

fn parse_revision_history(raw: &str) -> Vec<RevisionRow> {
    let mut rows = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        if parts.is_empty() {
            continue;
        }
        rows.push(RevisionRow {
            rev: parts.first().copied().unwrap_or("").to_string(),
            date: parts.get(1).copied().unwrap_or("").to_string(),
            description: parts.get(2).copied().unwrap_or("").to_string(),
            by: parts.get(3).copied().unwrap_or("").to_string(),
            checked: parts.get(4).copied().unwrap_or("").to_string(),
        });
    }
    rows
}

/// Minimal vertical strip: firm, title, drawing number, revision, scale, date.
fn emit_strip_minimal(
    doc: &mut Document,
    tb: &TitleBlock,
    style: &TitleBlockStyle,
) -> Vec<EntityId> {
    let mut ids = Vec::new();
    let o = style.origin;
    let w = style.width;
    let h = style.height;
    let inset = 2.0;
    let ht = style.text_height;
    let line = |d: &mut Document, ids: &mut Vec<EntityId>, a: Point2, b: Point2| {
        ids.push(d.add_geometry(LAYER_TITLEBLOCK, Geometry::Line(Segment::new(a, b))));
    };
    let attr = |d: &mut Document, ids: &mut Vec<EntityId>, value: &str, p: Point2, ht: f64| {
        if value.trim().is_empty() {
            return;
        }
        ids.push(d.add_geometry(LAYER_TB_ATTR, Geometry::Text(Text::new(value, p, ht, 0.0))));
    };
    let label = |d: &mut Document, ids: &mut Vec<EntityId>, value: &str, p: Point2, ht: f64| {
        if value.trim().is_empty() {
            return;
        }
        ids.push(d.add_geometry(LAYER_TB_TEXT, Geometry::Text(Text::new(value, p, ht, 0.0))));
    };
    line(doc, &mut ids, o, Point2::new(o.x + w, o.y));
    line(
        doc,
        &mut ids,
        Point2::new(o.x + w, o.y),
        Point2::new(o.x + w, o.y + h),
    );
    line(
        doc,
        &mut ids,
        Point2::new(o.x + w, o.y + h),
        Point2::new(o.x, o.y + h),
    );
    line(doc, &mut ids, Point2::new(o.x, o.y + h), o);

    let firm = if tb.company_name.trim().is_empty() {
        "AAD"
    } else {
        tb.company_name.as_str()
    };
    let mut y = o.y + h - ht - inset;
    attr(doc, &mut ids, firm, Point2::new(o.x + inset, y), ht * 1.2);
    y -= ht * 2.0;
    label(
        doc,
        &mut ids,
        "TITLE",
        Point2::new(o.x + inset, y),
        ht * 0.8,
    );
    y -= ht + 0.5;
    attr(
        doc,
        &mut ids,
        &tb.sheet_title,
        Point2::new(o.x + inset, y),
        ht,
    );
    y -= ht * 2.0;
    for (lab, val) in [
        ("SCALE", tb.scale.as_str()),
        ("DATE", tb.date.as_str()),
        ("REV", tb.revision.as_str()),
    ] {
        if val.trim().is_empty() {
            continue;
        }
        label(doc, &mut ids, lab, Point2::new(o.x + inset, y), ht * 0.75);
        y -= ht;
        attr(doc, &mut ids, val, Point2::new(o.x + inset, y), ht);
        y -= ht * 1.5;
    }
    attr(
        doc,
        &mut ids,
        &tb.sheet_id_display(),
        Point2::new(o.x + inset, o.y + inset * 1.5),
        style.text_height_large.max(HT_SHEET_NO * 0.85),
    );
    ids
}

fn emit_band_minimal(
    doc: &mut Document,
    tb: &TitleBlock,
    style: &TitleBlockStyle,
) -> Vec<EntityId> {
    let mut ids = Vec::new();
    let o = style.origin;
    let w = style.width;
    let h = style.height;
    let inset = 2.0;
    let ht = style.text_height;
    let line = |d: &mut Document, ids: &mut Vec<EntityId>, a: Point2, b: Point2| {
        ids.push(d.add_geometry(LAYER_TITLEBLOCK, Geometry::Line(Segment::new(a, b))));
    };
    let attr = |d: &mut Document, ids: &mut Vec<EntityId>, value: &str, p: Point2, ht: f64| {
        if value.trim().is_empty() {
            return;
        }
        ids.push(d.add_geometry(LAYER_TB_ATTR, Geometry::Text(Text::new(value, p, ht, 0.0))));
    };
    line(doc, &mut ids, o, Point2::new(o.x + w, o.y));
    line(
        doc,
        &mut ids,
        Point2::new(o.x + w, o.y),
        Point2::new(o.x + w, o.y + h),
    );
    line(
        doc,
        &mut ids,
        Point2::new(o.x + w, o.y + h),
        Point2::new(o.x, o.y + h),
    );
    line(doc, &mut ids, Point2::new(o.x, o.y + h), o);

    let firm = if tb.company_name.trim().is_empty() {
        "AAD"
    } else {
        tb.company_name.as_str()
    };
    let mid = o.y + h * 0.35;
    attr(doc, &mut ids, firm, Point2::new(o.x + inset, mid), ht * 1.1);
    attr(
        doc,
        &mut ids,
        &tb.sheet_title,
        Point2::new(o.x + w * 0.22, mid),
        ht,
    );
    let mut meta = format!("Scale {}", tb.scale);
    if !tb.date.is_empty() {
        meta.push_str("  ·  ");
        meta.push_str(&tb.date);
    }
    if !tb.revision.is_empty() {
        meta.push_str("  ·  Rev ");
        meta.push_str(&tb.revision);
    }
    attr(
        doc,
        &mut ids,
        &meta,
        Point2::new(o.x + w * 0.55, mid),
        ht * 0.9,
    );
    attr(
        doc,
        &mut ids,
        &tb.sheet_id_display(),
        Point2::new(o.x + w * 0.82, o.y + inset),
        style.text_height_large.max(HT_SHEET_NO * 0.75),
    );
    ids
}

fn emit_strip(doc: &mut Document, tb: &TitleBlock, style: &TitleBlockStyle) -> Vec<EntityId> {
    let mut ids = Vec::new();
    let o = style.origin;
    let w = style.width;
    let h = style.height;
    let inset = 2.0;

    // Zone heights (top → bottom), matching the reference sheet proportions.
    let h_brand = h * 0.14;
    let h_project = h * 0.36;
    let h_drawing = h * 0.12;
    let h_meta = h * 0.22;
    let h_sheet = h - (h_brand + h_project + h_drawing + h_meta);

    let y0 = o.y;
    let y_sheet = y0 + h_sheet;
    let y_meta = y_sheet + h_meta;
    let y_drawing = y_meta + h_drawing;
    let y_project = y_drawing + h_project;
    let y_top = o.y + h;

    let line = |d: &mut Document, ids: &mut Vec<EntityId>, a: Point2, b: Point2| {
        ids.push(d.add_geometry(LAYER_TITLEBLOCK, Geometry::Line(Segment::new(a, b))));
    };
    let label = |d: &mut Document, ids: &mut Vec<EntityId>, value: &str, p: Point2, ht: f64| {
        if value.trim().is_empty() {
            return;
        }
        ids.push(d.add_geometry(LAYER_TB_TEXT, Geometry::Text(Text::new(value, p, ht, 0.0))));
    };
    let attr =
        |d: &mut Document, ids: &mut Vec<EntityId>, value: &str, p: Point2, ht: f64, rot: f64| {
            if value.trim().is_empty() {
                return;
            }
            ids.push(d.add_geometry(LAYER_TB_ATTR, Geometry::Text(Text::new(value, p, ht, rot))));
        };

    // Outer strip box (shares right/top/bottom with sheet border; left divider).
    line(doc, &mut ids, o, Point2::new(o.x + w, o.y));
    line(
        doc,
        &mut ids,
        Point2::new(o.x + w, o.y),
        Point2::new(o.x + w, y_top),
    );
    line(
        doc,
        &mut ids,
        Point2::new(o.x + w, y_top),
        Point2::new(o.x, y_top),
    );
    line(doc, &mut ids, Point2::new(o.x, y_top), o);

    // Horizontal zone dividers.
    for y in [y_sheet, y_meta, y_drawing, y_project] {
        line(doc, &mut ids, Point2::new(o.x, y), Point2::new(o.x + w, y));
    }

    // --- Branding (top) ---
    let logo_h = h_brand * 0.42;
    line(
        doc,
        &mut ids,
        Point2::new(o.x, y_top - logo_h),
        Point2::new(o.x + w, y_top - logo_h),
    );
    // Logo cell marker (path label until image embed lands).
    if !tb.logo_path.trim().is_empty() {
        let name = std::path::Path::new(&tb.logo_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("LOGO");
        ids.push(doc.add_geometry(
            LAYER_LOGO,
            Geometry::Text(Text::new(
                name,
                Point2::new(o.x + inset, y_top - HT_VALUE - inset),
                HT_VALUE,
                0.0,
            )),
        ));
    }
    let firm = if tb.company_name.trim().is_empty() {
        "AAD"
    } else {
        tb.company_name.as_str()
    };
    attr(
        doc,
        &mut ids,
        firm,
        Point2::new(o.x + inset, y_top - logo_h - HT_TITLE - inset * 0.5),
        HT_TITLE * 0.85,
        0.0,
    );
    let mut by = y_top - logo_h - HT_TITLE - HT_VALUE - inset;
    for s in [
        tb.firm_tagline.as_str(),
        tb.office_address.as_str(),
        tb.phone.as_str(),
        tb.email.as_str(),
        tb.website.as_str(),
    ] {
        if s.trim().is_empty() {
            continue;
        }
        attr(
            doc,
            &mut ids,
            s,
            Point2::new(o.x + inset, by),
            HT_VALUE * 0.9,
            0.0,
        );
        by -= HT_VALUE + 0.6;
        if by < y_project + inset {
            break;
        }
    }

    // --- Project / client (rotated 90° CCW, reading up the strip) ---
    let project_name = if tb.project.trim().is_empty() {
        tb.client.as_str()
    } else {
        tb.project.as_str()
    };
    let client_line = if !tb.client.trim().is_empty() && tb.client != project_name {
        tb.client.as_str()
    } else {
        ""
    };
    // Insertion near left of strip; rotation 90° → text runs toward top.
    let rot_x = o.x + inset + HT_TITLE * 0.35;
    let rot_y = y_drawing + inset * 2.0;
    attr(
        doc,
        &mut ids,
        project_name,
        Point2::new(rot_x, rot_y),
        HT_TITLE,
        90.0,
    );
    if !client_line.is_empty() {
        attr(
            doc,
            &mut ids,
            client_line,
            Point2::new(rot_x + HT_LABEL + 1.0, rot_y),
            HT_LABEL,
            90.0,
        );
    }
    if !tb.address.trim().is_empty() {
        attr(
            doc,
            &mut ids,
            &tb.address,
            Point2::new(rot_x + HT_LABEL * 2.0 + 2.0, rot_y),
            HT_VALUE,
            90.0,
        );
    }

    // --- Drawing title ---
    label(
        doc,
        &mut ids,
        "DRAWING",
        Point2::new(o.x + inset, y_meta + h_drawing - HT_LABEL - inset),
        HT_LABEL * 0.85,
    );
    let drawing_title = if tb.sheet_title.trim().is_empty() {
        "—"
    } else {
        tb.sheet_title.as_str()
    };
    let mut dy = y_meta + h_drawing - HT_LABEL * 2.2 - inset;
    attr(
        doc,
        &mut ids,
        drawing_title,
        Point2::new(o.x + inset, dy),
        HT_LABEL,
        0.0,
    );
    if !tb.discipline.trim().is_empty() {
        dy -= HT_VALUE + 0.8;
        attr(
            doc,
            &mut ids,
            &tb.discipline,
            Point2::new(o.x + inset, dy),
            HT_VALUE,
            0.0,
        );
    }

    // --- Metadata + revision table ---
    let mut my = y_meta - HT_VALUE - inset;
    let meta_pairs = [
        ("DATE", tb.date.as_str()),
        ("SCALE", tb.scale.as_str()),
        ("STATUS", tb.drawing_status.as_str()),
        ("JOB NO.", tb.project_number.as_str()),
        ("DRAWN", tb.drawn_by.as_str()),
        ("CHECKED", tb.checked_by.as_str()),
        ("APPROVED", tb.approved_by.as_str()),
        ("REV", tb.revision.as_str()),
    ];
    for (lab, val) in meta_pairs {
        if val.trim().is_empty() && lab != "REV" {
            continue;
        }
        label(
            doc,
            &mut ids,
            lab,
            Point2::new(o.x + inset, my),
            HT_VALUE * 0.85,
        );
        attr(
            doc,
            &mut ids,
            val,
            Point2::new(o.x + inset + w * 0.38, my),
            HT_VALUE,
            0.0,
        );
        my -= HT_VALUE + 1.0;
        if my < y_sheet + h_meta * 0.42 {
            break;
        }
    }

    // Revision table (same format every sheet).
    let rev_top = (y_sheet + h_meta * 0.40).min(my - 1.0);
    line(
        doc,
        &mut ids,
        Point2::new(o.x, rev_top),
        Point2::new(o.x + w, rev_top),
    );
    ids.push(doc.add_geometry(
        LAYER_REVISION,
        Geometry::Text(Text::new(
            "REV  DATE        DESCRIPTION",
            Point2::new(o.x + inset, rev_top - HT_VALUE - 0.5),
            HT_VALUE * 0.8,
            0.0,
        )),
    ));
    let mut ry = rev_top - HT_VALUE * 2.0 - 1.0;
    let mut rev_rows = tb.revision_rows.clone();
    if rev_rows.is_empty() && !tb.revision.trim().is_empty() {
        rev_rows.push(RevisionRow {
            rev: tb.revision.clone(),
            date: tb.date.clone(),
            description: tb.drawing_status.clone(),
            by: tb.drawn_by.clone(),
            checked: tb.checked_by.clone(),
        });
    }
    for row in rev_rows.iter().take(4) {
        let line_s = format!(
            "{:<3} {:<10} {}",
            row.rev,
            row.date,
            if row.description.is_empty() {
                "—"
            } else {
                &row.description
            }
        );
        ids.push(doc.add_geometry(
            LAYER_REVISION,
            Geometry::Text(Text::new(
                line_s,
                Point2::new(o.x + inset, ry),
                HT_VALUE * 0.8,
                0.0,
            )),
        ));
        ry -= HT_VALUE + 0.6;
        if ry < y_sheet + inset {
            break;
        }
    }

    // --- Sheet / drawing number (bottom, largest) ---
    if !tb.sheet_number.trim().is_empty() {
        label(
            doc,
            &mut ids,
            &format!("SHEET {}", tb.sheet_number),
            Point2::new(o.x + inset, y_sheet - HT_VALUE - inset),
            HT_VALUE * 0.9,
        );
    }
    let id = tb.sheet_id_display();
    attr(
        doc,
        &mut ids,
        &id,
        Point2::new(o.x + inset, y0 + inset * 1.5),
        style.text_height_large.max(HT_SHEET_NO * 0.85),
        0.0,
    );

    let _ = h_project; // used via zone math above
    ids
}

/// Portrait layout: horizontal band along the bottom (same attribute set as the strip).
fn emit_band(doc: &mut Document, tb: &TitleBlock, style: &TitleBlockStyle) -> Vec<EntityId> {
    let mut ids = Vec::new();
    let o = style.origin;
    let w = style.width;
    let h = style.height;
    let inset = 2.0;

    // Zones left → right (mirrors the vertical strip content).
    let w_brand = w * 0.18;
    let w_project = w * 0.28;
    let w_drawing = w * 0.16;
    let w_meta = w * 0.22;
    let w_sheet = w - (w_brand + w_project + w_drawing + w_meta);

    let x0 = o.x;
    let x1 = x0 + w_brand;
    let x2 = x1 + w_project;
    let x3 = x2 + w_drawing;
    let x4 = x3 + w_meta;
    let x5 = o.x + w;
    let y0 = o.y;
    let y1 = o.y + h;

    let line = |d: &mut Document, ids: &mut Vec<EntityId>, a: Point2, b: Point2| {
        ids.push(d.add_geometry(LAYER_TITLEBLOCK, Geometry::Line(Segment::new(a, b))));
    };
    let label = |d: &mut Document, ids: &mut Vec<EntityId>, value: &str, p: Point2, ht: f64| {
        if value.trim().is_empty() {
            return;
        }
        ids.push(d.add_geometry(LAYER_TB_TEXT, Geometry::Text(Text::new(value, p, ht, 0.0))));
    };
    let attr = |d: &mut Document, ids: &mut Vec<EntityId>, value: &str, p: Point2, ht: f64| {
        if value.trim().is_empty() {
            return;
        }
        ids.push(d.add_geometry(LAYER_TB_ATTR, Geometry::Text(Text::new(value, p, ht, 0.0))));
    };

    // Outer band.
    line(doc, &mut ids, o, Point2::new(x5, y0));
    line(doc, &mut ids, Point2::new(x5, y0), Point2::new(x5, y1));
    line(doc, &mut ids, Point2::new(x5, y1), Point2::new(x0, y1));
    line(doc, &mut ids, Point2::new(x0, y1), o);
    for x in [x1, x2, x3, x4] {
        line(doc, &mut ids, Point2::new(x, y0), Point2::new(x, y1));
    }

    // Branding (left).
    let firm = if tb.company_name.trim().is_empty() {
        "AAD"
    } else {
        tb.company_name.as_str()
    };
    if !tb.logo_path.trim().is_empty() {
        let name = std::path::Path::new(&tb.logo_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("LOGO");
        ids.push(doc.add_geometry(
            LAYER_LOGO,
            Geometry::Text(Text::new(
                name,
                Point2::new(x0 + inset, y1 - HT_VALUE - inset),
                HT_VALUE,
                0.0,
            )),
        ));
    }
    attr(
        doc,
        &mut ids,
        firm,
        Point2::new(x0 + inset, y1 - HT_TITLE - inset * 1.5),
        HT_TITLE * 0.75,
    );
    let mut by = y1 - HT_TITLE - HT_VALUE * 2.0;
    for s in [
        tb.office_address.as_str(),
        tb.phone.as_str(),
        tb.email.as_str(),
        tb.website.as_str(),
    ] {
        if s.trim().is_empty() {
            continue;
        }
        attr(
            doc,
            &mut ids,
            s,
            Point2::new(x0 + inset, by),
            HT_VALUE * 0.85,
        );
        by -= HT_VALUE + 0.5;
        if by < y0 + inset {
            break;
        }
    }

    // Project / client (horizontal — no rotation needed on a bottom band).
    let project_name = if tb.project.trim().is_empty() {
        tb.client.as_str()
    } else {
        tb.project.as_str()
    };
    attr(
        doc,
        &mut ids,
        project_name,
        Point2::new(x1 + inset, y1 - HT_TITLE - inset),
        HT_TITLE * 0.85,
    );
    let mut py = y1 - HT_TITLE - HT_LABEL - inset * 2.0;
    if !tb.client.trim().is_empty() && tb.client != project_name {
        attr(
            doc,
            &mut ids,
            &tb.client,
            Point2::new(x1 + inset, py),
            HT_LABEL,
        );
        py -= HT_LABEL + 0.6;
    }
    if !tb.address.trim().is_empty() {
        attr(
            doc,
            &mut ids,
            &tb.address,
            Point2::new(x1 + inset, py),
            HT_VALUE,
        );
    }

    // Drawing title.
    label(
        doc,
        &mut ids,
        "DRAWING",
        Point2::new(x2 + inset, y1 - HT_VALUE - inset),
        HT_VALUE * 0.85,
    );
    let drawing_title = if tb.sheet_title.trim().is_empty() {
        "—"
    } else {
        tb.sheet_title.as_str()
    };
    attr(
        doc,
        &mut ids,
        drawing_title,
        Point2::new(x2 + inset, y0 + h * 0.45),
        HT_LABEL,
    );
    if !tb.discipline.trim().is_empty() {
        attr(
            doc,
            &mut ids,
            &tb.discipline,
            Point2::new(x2 + inset, y0 + inset * 1.5),
            HT_VALUE,
        );
    }

    // Metadata + revision.
    let mut my = y1 - HT_VALUE - inset;
    for (lab, val) in [
        ("DATE", tb.date.as_str()),
        ("SCALE", tb.scale.as_str()),
        ("DRAWN", tb.drawn_by.as_str()),
        ("CHECKED", tb.checked_by.as_str()),
        ("REV", tb.revision.as_str()),
        ("STATUS", tb.drawing_status.as_str()),
        ("JOB", tb.project_number.as_str()),
    ] {
        if val.trim().is_empty() {
            continue;
        }
        label(
            doc,
            &mut ids,
            lab,
            Point2::new(x3 + inset, my),
            HT_VALUE * 0.8,
        );
        attr(
            doc,
            &mut ids,
            val,
            Point2::new(x3 + inset + w_meta * 0.35, my),
            HT_VALUE * 0.9,
        );
        my -= HT_VALUE + 0.7;
        if my < y0 + h * 0.35 {
            break;
        }
    }
    let mut rev_rows = tb.revision_rows.clone();
    if rev_rows.is_empty() && !tb.revision.trim().is_empty() {
        rev_rows.push(RevisionRow {
            rev: tb.revision.clone(),
            date: tb.date.clone(),
            description: tb.drawing_status.clone(),
            by: tb.drawn_by.clone(),
            checked: tb.checked_by.clone(),
        });
    }
    let mut ry = (y0 + h * 0.32).min(my - 1.0);
    ids.push(doc.add_geometry(
        LAYER_REVISION,
        Geometry::Text(Text::new(
            "REV|DATE|DESC",
            Point2::new(x3 + inset, ry),
            HT_VALUE * 0.75,
            0.0,
        )),
    ));
    ry -= HT_VALUE + 0.4;
    for row in rev_rows.iter().take(2) {
        ids.push(doc.add_geometry(
            LAYER_REVISION,
            Geometry::Text(Text::new(
                format!("{} {} {}", row.rev, row.date, row.description),
                Point2::new(x3 + inset, ry),
                HT_VALUE * 0.75,
                0.0,
            )),
        ));
        ry -= HT_VALUE + 0.4;
        if ry < y0 + inset {
            break;
        }
    }

    // Sheet / drawing number (right, largest).
    if !tb.sheet_number.trim().is_empty() {
        label(
            doc,
            &mut ids,
            &format!("SHEET {}", tb.sheet_number),
            Point2::new(x4 + inset, y1 - HT_VALUE - inset),
            HT_VALUE * 0.85,
        );
    }
    attr(
        doc,
        &mut ids,
        &tb.sheet_id_display(),
        Point2::new(x4 + inset, y0 + inset * 1.5),
        style.text_height_large.max(HT_SHEET_NO * 0.9),
    );

    let _ = w_sheet;
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TitleBlock {
        TitleBlock {
            company_name: "Studio AAD".into(),
            firm_tagline: "ARCHITECTURE · INTERIORS".into(),
            phone: "+1 555 0100".into(),
            email: "studio@example.com".into(),
            office_address: "10 Draft Ave".into(),
            website: "www.example.com".into(),
            logo_path: String::new(),
            project: "Riverside Depot".into(),
            client: "Acme Co".into(),
            address: "1 Main St".into(),
            project_number: "2026-01".into(),
            discipline: "Architecture".into(),
            sheet_title: "Ground Floor Plan".into(),
            drawing_number: "A-101".into(),
            sheet_number: "01 of 12".into(),
            total_sheets: "12".into(),
            scale: "1:100".into(),
            date: "22-Jul-2026".into(),
            drawn_by: "VH".into(),
            checked_by: "RK".into(),
            approved_by: "PM".into(),
            revision: "00".into(),
            drawing_status: "IFC".into(),
            architect: "Jane Doe".into(),
            area: "120 m²".into(),
            revision_rows: vec![RevisionRow {
                rev: "00".into(),
                date: "22-Jul-2026".into(),
                description: "Initial Issue".into(),
                by: "VH".into(),
                checked: "RK".into(),
            }],
            extra: vec![],
            preset: NameplatePreset::Professional,
        }
    }

    #[test]
    fn nameplate_preset_parse() {
        assert_eq!(
            NameplatePreset::parse("compact"),
            Some(NameplatePreset::Compact)
        );
        assert_eq!(
            NameplatePreset::parse(""),
            Some(NameplatePreset::Professional)
        );
        assert_eq!(NameplatePreset::Minimal.as_token(), "minimal");
    }

    #[test]
    fn minimal_preset_emits_fewer_attr_texts_than_professional() {
        let mut pro = sample();
        pro.preset = NameplatePreset::Professional;
        let mut min = sample();
        min.preset = NameplatePreset::Minimal;
        let style_pro = TitleBlockStyle {
            width: 70.0,
            height: 280.0,
            preset: NameplatePreset::Professional,
            ..TitleBlockStyle::default()
        };
        let style_min = TitleBlockStyle {
            width: 50.0,
            height: 280.0,
            preset: NameplatePreset::Minimal,
            ..TitleBlockStyle::default()
        };
        let mut doc_pro = Document::new();
        let mut doc_min = Document::new();
        pro.place(&mut doc_pro, &style_pro);
        min.place(&mut doc_min, &style_min);
        let count = |doc: &Document| {
            doc.entities()
                .into_iter()
                .filter(|e| e.layer == LAYER_TB_ATTR)
                .count()
        };
        assert!(
            count(&doc_min) < count(&doc_pro),
            "minimal={} professional={}",
            count(&doc_min),
            count(&doc_pro)
        );
    }

    #[test]
    fn from_properties_prefers_drawing_name() {
        let props = DrawingProperties {
            title: "Stored Title".into(),
            project: "P1".into(),
            drawing_number: "A-101".into(),
            ..Default::default()
        };
        let tb = TitleBlock::from_properties(&props, "Plot Name");
        assert_eq!(tb.sheet_title, "Plot Name");
        assert_eq!(tb.project, "P1");
        assert_eq!(tb.drawing_number, "A-101");
    }

    #[test]
    fn rows_include_title_plate_fields() {
        let rows = sample().rows();
        assert_eq!(rows[0], ("Project".into(), "Riverside Depot".into()));
        assert!(rows.iter().any(|(l, v)| l == "Client" && v == "Acme Co"));
    }

    #[test]
    fn field_lookup_covers_standard_fields() {
        let tb = sample();
        assert_eq!(tb.field("Sheet Number"), Some("A-101"));
        assert_eq!(tb.field("Drawing Title"), Some("Ground Floor Plan"));
        assert_eq!(tb.field("Client"), Some("Acme Co"));
        assert_eq!(tb.field("Nonexistent"), None);
    }

    #[test]
    fn place_emits_vertical_strip_geometry() {
        let mut doc = Document::new();
        let tb = sample();
        let style = TitleBlockStyle {
            width: 70.0,
            height: 280.0,
            ..TitleBlockStyle::default()
        };
        let ids = tb.place(&mut doc, &style);
        assert!(!ids.is_empty());
        assert_eq!(doc.len(), ids.len());
        assert!(doc.entities().into_iter().any(|e| matches!(
            &e.geometry,
            Geometry::Text(t) if t.value.contains("Riverside Depot") && (t.rotation - 90.0).abs() < 1e-6
        )));
        assert!(doc.entities().into_iter().any(|e| matches!(
            &e.geometry,
            Geometry::Text(t) if t.value == "A-101"
        )));
        assert!(doc.entities().into_iter().any(|e| e.layer == LAYER_TB_ATTR));
        assert!(doc
            .entities()
            .into_iter()
            .any(|e| e.layer == LAYER_REVISION));
    }

    #[test]
    fn place_is_one_undoable_unit() {
        let mut doc = Document::new();
        sample().place(
            &mut doc,
            &TitleBlockStyle {
                width: 70.0,
                height: 280.0,
                ..TitleBlockStyle::default()
            },
        );
        assert_eq!(doc.undo_label(), Some("TITLEBLOCK"));
        doc.undo();
        assert_eq!(doc.len(), 0, "one undo clears the whole block");
    }

    #[test]
    fn place_on_sheet_is_right_strip() {
        let mut doc = Document::new();
        let sheet = Sheet::a1_landscape();
        sample().place_on_sheet(&mut doc, &sheet);
        let (min, max) = sheet.inner();
        let strip_w = TitleBlock::strip_width_mm(&sheet);
        for e in doc.entities() {
            if let Geometry::Text(t) = &e.geometry {
                assert!(t.position.x >= max.x - strip_w - 1.0);
                assert!(t.position.x <= max.x + 1.0);
                assert!(t.position.y >= min.y - 1.0 && t.position.y <= max.y + 1.0);
            }
        }
    }

    #[test]
    fn place_on_portrait_is_bottom_band() {
        use crate::sheet::{Orientation, PaperSize};
        let mut doc = Document::new();
        let sheet = Sheet::new(PaperSize::IsoA3, Orientation::Portrait, 10.0);
        sample().place_on_sheet(&mut doc, &sheet);
        let (min, max) = sheet.inner();
        let strip_h = TitleBlock::strip_height_mm(&sheet);
        assert_eq!(
            TitleBlock::layout_for(&sheet),
            TitleBlockLayout::HorizontalBottom
        );
        for e in doc.entities() {
            if let Geometry::Text(t) = &e.geometry {
                assert!(t.position.y >= min.y - 1.0);
                assert!(t.position.y <= min.y + strip_h + 1.0);
                assert!(t.position.x >= min.x - 1.0 && t.position.x <= max.x + 1.0);
            }
        }
        assert!(doc.entities().into_iter().any(|e| matches!(
            &e.geometry,
            Geometry::Text(t) if t.value.contains("Riverside Depot") && t.rotation.abs() < 1e-6
        )));
    }

    #[test]
    fn strip_is_about_sixteen_percent_of_width() {
        let sheet = Sheet::a1_landscape();
        let (min, max) = sheet.inner();
        let frac = TitleBlock::strip_width_mm(&sheet) / (max.x - min.x);
        assert!((0.14..=0.18).contains(&frac), "frac={frac}");
    }
}
