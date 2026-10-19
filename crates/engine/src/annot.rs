//! Units, plot scale and annotation layout, independent of any user interface.
//!
//! * [`Unit`], [`Paper`] and [`PlotScale`] convert between drawing units, display units and paper
//!   millimetres. Annotation sizes follow the printed size at the plot scale, never screen pixels:
//!   a text that must print 2.5 mm high at 1:100 is 250 mm high in a millimetre drawing and 0.25
//!   in a metre drawing, whatever the zoom.
//! * [`conflicts`] finds annotation text that collides with other text or with drawing geometry.
//! * [`propose`] moves conflicting text into clear space and never moves anything else. The result
//!   is deterministic: the same drawing and settings give the same moves.

use std::collections::{BTreeMap, BTreeSet};

use aad_doc::{Drawing, Entity, EntityKind, Handle, Space, entity_bounds};
use aad_geom::{Bounds2, Vec2};

use crate::select::select_window;

/// A length unit for drawing or display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Mm,
    Cm,
    M,
    In,
    Ft,
    /// Feet and inches, shown as `5'-3"`; values are measured in inches.
    FtIn,
}

impl Unit {
    pub fn parse(s: &str) -> Option<Unit> {
        Some(match s.trim().to_ascii_lowercase().replace([' ', '_', '-'], "").as_str() {
            "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" => Unit::Mm,
            "cm" | "centimeter" | "centimeters" | "centimetre" | "centimetres" => Unit::Cm,
            "m" | "meter" | "meters" | "metre" | "metres" => Unit::M,
            "in" | "inch" | "inches" => Unit::In,
            "ft" | "foot" | "feet" => Unit::Ft,
            "ftin" | "feetinches" | "feetandinches" => Unit::FtIn,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Unit::Mm => "mm",
            Unit::Cm => "cm",
            Unit::M => "m",
            Unit::In => "in",
            Unit::Ft => "ft",
            Unit::FtIn => "ftin",
        }
    }

    /// Metres in one of this unit (feet-and-inches count in inches).
    pub fn metres(self) -> f64 {
        match self {
            Unit::Mm => 0.001,
            Unit::Cm => 0.01,
            Unit::M => 1.0,
            Unit::In | Unit::FtIn => 0.0254,
            Unit::Ft => 0.3048,
        }
    }

    /// The `INSUNITS` code of a drawing in this unit (feet-and-inches is not a drawing unit).
    pub fn insunits(self) -> Option<i64> {
        match self {
            Unit::In => Some(1),
            Unit::Ft => Some(2),
            Unit::Mm => Some(4),
            Unit::Cm => Some(5),
            Unit::M => Some(6),
            Unit::FtIn => None,
        }
    }

    pub fn from_insunits(code: i64) -> Option<Unit> {
        Some(match code {
            1 => Unit::In,
            2 => Unit::Ft,
            4 => Unit::Mm,
            5 => Unit::Cm,
            6 => Unit::M,
            _ => return None,
        })
    }

    /// Decimal places a displayed measurement has by default.
    fn decimals(self) -> u8 {
        match self {
            Unit::Mm => 0,
            Unit::Cm => 1,
            Unit::M => 3,
            Unit::In => 2,
            Unit::Ft => 3,
            Unit::FtIn => 0,
        }
    }
}

/// The drawing's units from `INSUNITS`, or `None` when unset or not a length unit this module knows
/// (the caller must ask, never guess).
pub fn drawing_unit(d: &Drawing) -> Option<Unit> {
    Unit::from_insunits(d.header.i64("INSUNITS", 0))
}

/// The unit measurements are displayed in (`AADDISPUNITS`), the drawing unit when never set.
pub fn display_unit(d: &Drawing) -> Option<Unit> {
    match d.header.str("AADDISPUNITS", "").as_str() {
        "" => drawing_unit(d),
        s => Unit::parse(s),
    }
}

/// Make dimension text show measurements in `shown`: only the style's linear factor, unit format
/// and suffix change, never a coordinate. `drawn` is the drawing unit.
pub fn apply_display_unit(d: &mut Drawing, drawn: Unit, shown: Unit) {
    let factor = drawn.metres() / shown.metres();
    for st in &mut d.dim_styles {
        st.linear_factor = factor;
        st.decimals = shown.decimals();
        if shown == Unit::FtIn {
            st.linear_unit = 4;
            st.post = String::new();
        } else {
            st.linear_unit = 2;
            st.post = format!("<> {}", shown.name());
        }
    }
    d.header.set_str("AADDISPUNITS", shown.name());
}

/// A sheet size in millimetres, landscape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paper {
    pub width: f64,
    pub height: f64,
}

impl Paper {
    pub fn named(name: &str) -> Option<Paper> {
        let (w, h) = match name.trim().to_ascii_uppercase().as_str() {
            "A0" => (1189.0, 841.0),
            "A1" => (841.0, 594.0),
            "A2" => (594.0, 420.0),
            "A3" => (420.0, 297.0),
            "A4" => (297.0, 210.0),
            _ => return None,
        };
        Some(Paper { width: w, height: h })
    }

    pub fn custom(width: f64, height: f64) -> Option<Paper> {
        (width.is_finite() && height.is_finite() && width >= 50.0 && height >= 50.0 && width <= 20_000.0 && height <= 20_000.0)
            .then_some(Paper { width, height })
    }

    /// The printable area: the sheet less a margin on every side.
    pub fn printable(self, margin: f64) -> (f64, f64) {
        ((self.width - 2.0 * margin).max(0.0), (self.height - 2.0 * margin).max(0.0))
    }
}

/// A plot scale 1:`denominator`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlotScale(pub f64);

impl PlotScale {
    /// `"1:100"`, `"1/100"`, or a bare denominator.
    pub fn parse(s: &str) -> Option<PlotScale> {
        let t = s.trim();
        let d = t.strip_prefix("1:").or_else(|| t.strip_prefix("1/")).unwrap_or(t);
        let v: f64 = d.trim().parse().ok()?;
        (v.is_finite() && (0.01..=100_000.0).contains(&v)).then_some(PlotScale(v))
    }

    pub fn label(self) -> String {
        format!("1:{}", self.0)
    }

    /// Drawing units that print as one millimetre on paper.
    pub fn du_per_paper_mm(self, drawn: Unit) -> f64 {
        self.0 * 0.001 / drawn.metres()
    }
}

/// Common 1:N denominators for architectural and civil plot scales.
pub const STANDARD_SCALE_DENOMS: &[f64] = &[
    1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 40.0, 50.0, 75.0, 100.0, 125.0, 150.0, 200.0, 250.0, 400.0, 500.0, 750.0, 1000.0, 1250.0, 1500.0, 2000.0,
    2500.0, 5000.0, 10000.0,
];

/// A fit scale expressed as paper units per drawing unit, with a standard 1:N (or N:1) label.
#[derive(Clone, Debug, PartialEq)]
pub struct NearestScale {
    /// Exact ratio as a label (`1:47.62` or `2:1`).
    pub label: String,
    /// Nearest standard scale that still fits when shrinking (`1:50`), or nearest when enlarging.
    pub nearest: String,
    /// Denominator of `nearest` when it is `1:N` (else 1 / N for `N:1`).
    pub denom: f64,
}

/// Nearest standard plot scale for a fit factor (paper units per drawing unit).
pub fn nearest_plot_scale(paper_per_du: f64) -> NearestScale {
    if !(paper_per_du.is_finite() && paper_per_du > 0.0) {
        return NearestScale { label: String::new(), nearest: String::new(), denom: 0.0 };
    }
    if paper_per_du <= 1.0 {
        let need = 1.0 / paper_per_du;
        let label = if (need - need.round()).abs() < 1e-6 * need.max(1.0) { format!("1:{}", need.round()) } else { format!("1:{need:.2}") };
        // Smallest standard denominator that still fits (ceil in the scale list).
        let denom = STANDARD_SCALE_DENOMS.iter().copied().find(|&d| d + 1e-9 >= need).unwrap_or_else(|| need.ceil().max(1.0));
        let nearest = if (denom - denom.round()).abs() < 1e-9 { format!("1:{}", denom.round()) } else { format!("1:{denom:.2}") };
        NearestScale { label, nearest, denom }
    } else {
        let n = paper_per_du;
        let label = if (n - n.round()).abs() < 1e-9 { format!("{}:1", n.round()) } else { format!("{n:.2}:1") };
        let denom = STANDARD_SCALE_DENOMS.iter().copied().min_by(|a, b| (a - n).abs().total_cmp(&(b - n).abs())).unwrap_or(n.round().max(1.0));
        let nearest = if (denom - denom.round()).abs() < 1e-9 { format!("{}:1", denom.round()) } else { format!("{denom:.2}:1") };
        NearestScale { label, nearest, denom }
    }
}

/// Annotation sizes in drawing units for a printed size in millimetres, at a plot scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnnotationSizes {
    pub text_height: f64,
    pub arrow_size: f64,
    pub ext_offset: f64,
    pub ext_extend: f64,
    pub text_gap: f64,
    pub baseline_spacing: f64,
}

/// Printed sizes (mm) that [`sizes`] scales.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrintedSizes {
    pub text_mm: f64,
    pub arrow_mm: f64,
}

impl Default for PrintedSizes {
    fn default() -> Self {
        PrintedSizes { text_mm: 2.5, arrow_mm: 2.5 }
    }
}

pub fn sizes(printed: PrintedSizes, scale: PlotScale, drawn: Unit) -> AnnotationSizes {
    let k = scale.du_per_paper_mm(drawn);
    AnnotationSizes {
        text_height: printed.text_mm * k,
        arrow_size: printed.arrow_mm * k,
        ext_offset: 1.5 * k,
        ext_extend: 1.25 * k,
        text_gap: 0.9 * k,
        baseline_spacing: 7.0 * k,
    }
}

/// Give every dimension style the sizes (non-annotative; DIMSCALE 1) and the current text height.
pub fn apply_sizes(d: &mut Drawing, a: AnnotationSizes) {
    for st in &mut d.dim_styles {
        st.scale = 1.0;
        st.text_height = a.text_height;
        st.arrow_size = a.arrow_size;
        st.ext_offset = a.ext_offset;
        st.ext_extend = a.ext_extend;
        st.text_gap = a.text_gap;
        st.baseline_spacing = a.baseline_spacing;
    }
    d.header.set_f64("TEXTSIZE", a.text_height);
}

// ---------------------------------------------------------------------------------------------
// Collisions and layout

/// The named group of annotations the user placed by hand; automatic layout leaves them alone.
pub const MANUAL_GROUP: &str = "AAD-MANUAL";

fn is_text(e: &Entity) -> bool {
    matches!(e.kind, EntityKind::Text(_) | EntityKind::MText(_) | EntityKind::AttDef(_))
}

fn geometry_kind(e: &Entity) -> Option<&'static str> {
    Some(match e.kind {
        EntityKind::Line(_)
        | EntityKind::Arc(_)
        | EntityKind::Circle(_)
        | EntityKind::Ellipse(_)
        | EntityKind::LwPolyline(_)
        | EntityKind::Spline(_)
        | EntityKind::XLine(_)
        | EntityKind::Ray(_) => "text-geometry",
        EntityKind::Dimension(_) => "text-dimension",
        EntityKind::Insert(_) => "text-block",
        EntityKind::Leader(_) | EntityKind::MLeader(_) => "text-leader",
        _ => return None,
    })
}

/// One collision of a text with something else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub text: Handle,
    pub other: Handle,
    /// `text-text`, `text-geometry`, `text-dimension`, `text-block` or `text-leader`.
    pub kind: &'static str,
}

fn text_boxes(d: &Drawing, space: &Space) -> BTreeMap<u64, (Handle, Bounds2)> {
    let mut out = BTreeMap::new();
    if let Some(store) = d.space(space) {
        for e in store.iter().filter(|e| is_text(e) && d.is_visible(e)) {
            let b = entity_bounds(d, e, 0);
            if !b.is_empty() {
                out.insert(e.handle.0, (e.handle, b));
            }
        }
    }
    out
}

fn expand(b: Bounds2, by: f64) -> Bounds2 {
    b.expand(by)
}

/// Every collision of a text object in `space` with other text or drawing geometry, in handle
/// order. `clearance` is the minimum gap in drawing units.
pub fn conflicts(d: &Drawing, space: &Space, clearance: f64) -> Vec<Conflict> {
    let boxes = text_boxes(d, space);
    let mut out = Vec::new();
    for (h, b) in boxes.values() {
        let probe = expand(*b, clearance);
        for (oh, ob) in boxes.values() {
            if oh.0 > h.0 && probe.intersects(ob) {
                out.push(Conflict { text: *h, other: *oh, kind: "text-text" });
            }
        }
        for g in select_window(d, space, probe, true) {
            if g == *h {
                continue;
            }
            if let Some(e) = d.entity(g)
                && let Some(kind) = geometry_kind(e)
            {
                out.push(Conflict { text: *h, other: g, kind });
            }
        }
    }
    out.sort_by_key(|c| (c.text.0, c.other.0, c.kind));
    out.dedup();
    out
}

/// Settings of [`propose`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutParams {
    pub clearance: f64,
    /// Most steps away from the original position to try, in text heights.
    pub max_steps: u32,
    /// Also move text in the manual group.
    pub include_manual: bool,
}

/// A proposed translation of a text object.
#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    pub handle: Handle,
    pub dx: f64,
    pub dy: f64,
}

/// The outcome of [`propose`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub moves: Vec<Move>,
    /// Conflicting text for which no clear position was found within the limits.
    pub unresolved: Vec<Handle>,
    /// Conflicting text left where it is because the user placed it.
    pub skipped_manual: Vec<Handle>,
}

/// Compass directions in the fixed order candidate positions are tried.
const DIRS: [(f64, f64); 8] = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];

fn manual_set(d: &Drawing) -> BTreeSet<u64> {
    d.groups.iter().filter(|g| g.name.eq_ignore_ascii_case(MANUAL_GROUP)).flat_map(|g| g.members.iter().map(|h| h.0)).collect()
}

fn clear_at(d: &Drawing, space: &Space, me: Handle, b: Bounds2, others: &BTreeMap<u64, Bounds2>, clearance: f64) -> bool {
    let probe = expand(b, clearance);
    if others.iter().any(|(k, ob)| *k != me.0 && probe.intersects(ob)) {
        return false;
    }
    !select_window(d, space, probe, true).into_iter().any(|g| g != me && d.entity(g).is_some_and(|e| geometry_kind(e).is_some()))
}

/// Move conflicting text into the nearest clear position. Geometry and non-conflicting text are
/// never touched; text in the manual group is skipped unless `include_manual`.
pub fn propose(d: &Drawing, space: &Space, p: LayoutParams) -> Layout {
    let manual = manual_set(d);
    let mut boxes: BTreeMap<u64, Bounds2> = text_boxes(d, space).into_iter().map(|(k, (_, b))| (k, b)).collect();
    let mut texts: Vec<u64> = Vec::new();
    for c in conflicts(d, space, p.clearance) {
        texts.push(c.text.0);
        if c.kind == "text-text" {
            texts.push(c.other.0);
        }
    }
    texts.sort_unstable();
    texts.dedup();
    let mut out = Layout::default();
    for key in texts {
        let h = Handle(key);
        if manual.contains(&key) && !p.include_manual {
            out.skipped_manual.push(h);
            continue;
        }
        let Some(&b) = boxes.get(&key) else { continue };
        let step = (b.height()).max(b.width() / 8.0).max(1e-9);
        let mut found = None;
        'search: for ring in 1..=p.max_steps {
            for (dx, dy) in DIRS {
                let (ox, oy) = (dx * f64::from(ring) * step, dy * f64::from(ring) * step);
                let moved = Bounds2::new(b.min + Vec2::new(ox, oy), b.max + Vec2::new(ox, oy));
                if clear_at(d, space, h, moved, &boxes, p.clearance) {
                    found = Some((ox, oy, moved));
                    break 'search;
                }
            }
        }
        match found {
            Some((dx, dy, moved)) => {
                boxes.insert(key, moved);
                out.moves.push(Move { handle: h, dx, dy });
            }
            None => out.unresolved.push(h),
        }
    }
    out
}

/// Translate a text object by `(dx, dy)`.
pub fn translate_text(e: &mut Entity, dx: f64, dy: f64) {
    match &mut e.kind {
        EntityKind::Text(t) => {
            t.insert.x += dx;
            t.insert.y += dy;
            if let Some(a) = &mut t.align_pt {
                a.x += dx;
                a.y += dy;
            }
        }
        EntityKind::MText(t) => {
            t.insert.x += dx;
            t.insert.y += dy;
        }
        EntityKind::AttDef(a) => {
            a.text.insert.x += dx;
            a.text.insert.y += dy;
            if let Some(p) = &mut a.text.align_pt {
                p.x += dx;
                p.y += dy;
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_scale_picks_standard_that_fits() {
        let n = nearest_plot_scale(1.0 / 47.0);
        assert_eq!(n.nearest, "1:50");
        assert!(n.label.starts_with("1:"));
        let tight = nearest_plot_scale(1.0 / 100.0);
        assert_eq!(tight.nearest, "1:100");
        let one = nearest_plot_scale(1.0);
        assert_eq!(one.nearest, "1:1");
    }
}
