//! Title block — the labelled information box in the corner of a sheet.
//!
//! A [`TitleBlock`] is data: project, sheet title/number, date, scale, and who
//! drew and checked it, plus any firm-custom fields. [`TitleBlock::place`] draws
//! it into the document as a bordered stack of `LABEL: value` rows — ordinary
//! `LINE` and `TEXT` geometry, like a schedule table, so it exports to DXF and
//! `UNDO`s as one unit. Completeness is checked by [`crate::qa`].

use aadt_geometry::{Point2, Segment};
use aadt_model::{Document, EntityId, Geometry, Text};

use crate::sheet::Sheet;

/// Layer the title block is drawn on.
pub const TITLEBLOCK_LAYER: &str = "A-TTLB";

/// The information recorded in a sheet's title block. Standard fields are named;
/// `extra` carries firm-specific rows (client, address, job number, …).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TitleBlock {
    pub project: String,
    pub sheet_title: String,
    pub sheet_number: String,
    pub date: String,
    pub scale: String,
    pub drawn_by: String,
    pub checked_by: String,
    pub revision: String,
    /// Firm/project-custom rows, in the order they should appear.
    pub extra: Vec<(String, String)>,
}

impl TitleBlock {
    /// The fields a sheet must carry to be considered complete (used by QA).
    pub const REQUIRED: &'static [&'static str] =
        &["Project", "Sheet Title", "Sheet Number", "Scale"];

    /// The rows, in display order, as `(label, value)`.
    pub fn rows(&self) -> Vec<(String, String)> {
        let mut rows = vec![
            ("Project".to_string(), self.project.clone()),
            ("Sheet Title".to_string(), self.sheet_title.clone()),
            ("Sheet Number".to_string(), self.sheet_number.clone()),
            ("Date".to_string(), self.date.clone()),
            ("Scale".to_string(), self.scale.clone()),
            ("Drawn By".to_string(), self.drawn_by.clone()),
            ("Checked By".to_string(), self.checked_by.clone()),
            ("Revision".to_string(), self.revision.clone()),
        ];
        rows.extend(self.extra.iter().cloned());
        rows
    }

    /// Look up a row's value by its label (as it appears in [`rows`](Self::rows)).
    pub fn field(&self, label: &str) -> Option<&str> {
        match label {
            "Project" => Some(&self.project),
            "Sheet Title" => Some(&self.sheet_title),
            "Sheet Number" => Some(&self.sheet_number),
            "Date" => Some(&self.date),
            "Scale" => Some(&self.scale),
            "Drawn By" => Some(&self.drawn_by),
            "Checked By" => Some(&self.checked_by),
            "Revision" => Some(&self.revision),
            other => self
                .extra
                .iter()
                .find(|(l, _)| l == other)
                .map(|(_, v)| v.as_str()),
        }
    }

    /// Draw the title block into `doc` at `origin` (its bottom-left corner),
    /// returning every entity created. One `UNDO` (`TITLEBLOCK`) removes it.
    pub fn place(&self, doc: &mut Document, style: &TitleBlockStyle) -> Vec<EntityId> {
        let rows = self.rows();
        doc.transact("TITLEBLOCK", |d| emit(d, &rows, style))
    }

    /// Draw the title block anchored to the bottom-right corner of `sheet`'s
    /// border — the conventional placement — with a default style.
    pub fn place_on_sheet(&self, doc: &mut Document, sheet: &Sheet) -> Vec<EntityId> {
        let (_, max) = sheet.inner();
        let style = TitleBlockStyle {
            origin: Point2::new(max.x - TitleBlockStyle::DEFAULT_WIDTH, sheet.inner().0.y),
            ..TitleBlockStyle::default()
        };
        self.place(doc, &style)
    }
}

/// Placement and sizing for a title block, in the document's units (mm on a sheet).
#[derive(Debug, Clone)]
pub struct TitleBlockStyle {
    /// Bottom-left corner of the block.
    pub origin: Point2,
    pub width: f64,
    pub row_height: f64,
    pub text_height: f64,
    /// Fraction of the row width given to the label column.
    pub label_fraction: f64,
}

impl TitleBlockStyle {
    const DEFAULT_WIDTH: f64 = 180.0;
}

impl Default for TitleBlockStyle {
    fn default() -> Self {
        Self {
            origin: Point2::new(0.0, 0.0),
            width: Self::DEFAULT_WIDTH,
            row_height: 8.0,
            text_height: 2.5,
            label_fraction: 0.35,
        }
    }
}

/// Emit the block geometry: an outer box, a horizontal rule under each row, the
/// label/value divider, and the label + value text. Bottom-up so row 0 (the
/// first field) sits at the top, the way a title block reads.
fn emit(doc: &mut Document, rows: &[(String, String)], style: &TitleBlockStyle) -> Vec<EntityId> {
    let mut ids = Vec::new();
    let o = style.origin;
    let n = rows.len();
    let total_h = n as f64 * style.row_height;
    let label_w = style.width * style.label_fraction;
    let inset = (style.row_height - style.text_height) / 2.0;

    let mut line = |d: &mut Document, a: Point2, b: Point2| {
        ids.push(d.add_geometry(TITLEBLOCK_LAYER, Geometry::Line(Segment::new(a, b))));
    };

    // Outer box.
    line(doc, o, Point2::new(o.x + style.width, o.y));
    line(
        doc,
        Point2::new(o.x + style.width, o.y),
        Point2::new(o.x + style.width, o.y + total_h),
    );
    line(
        doc,
        Point2::new(o.x + style.width, o.y + total_h),
        Point2::new(o.x, o.y + total_h),
    );
    line(doc, Point2::new(o.x, o.y + total_h), o);

    // Row rules (skip the outer top/bottom, already drawn) and the label divider.
    for r in 1..n {
        let y = o.y + r as f64 * style.row_height;
        line(doc, Point2::new(o.x, y), Point2::new(o.x + style.width, y));
    }
    line(
        doc,
        Point2::new(o.x + label_w, o.y),
        Point2::new(o.x + label_w, o.y + total_h),
    );

    // Text, top row first.
    for (i, (label, value)) in rows.iter().enumerate() {
        let row_from_top = (n - 1 - i) as f64;
        let y = o.y + row_from_top * style.row_height + inset;
        ids.push(doc.add_geometry(
            TITLEBLOCK_LAYER,
            Geometry::Text(Text::new(
                label.clone(),
                Point2::new(o.x + inset, y),
                style.text_height,
                0.0,
            )),
        ));
        ids.push(doc.add_geometry(
            TITLEBLOCK_LAYER,
            Geometry::Text(Text::new(
                value.clone(),
                Point2::new(o.x + label_w + inset, y),
                style.text_height,
                0.0,
            )),
        ));
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TitleBlock {
        TitleBlock {
            project: "Riverside Depot".into(),
            sheet_title: "Ground Floor Plan".into(),
            sheet_number: "A-101".into(),
            date: "2026-07-20".into(),
            scale: "1:100".into(),
            drawn_by: "VH".into(),
            checked_by: "".into(),
            revision: "A".into(),
            extra: vec![("Client".into(), "Acme Co".into())],
        }
    }

    #[test]
    fn rows_include_standard_then_extra_fields() {
        let rows = sample().rows();
        assert_eq!(rows[0], ("Project".into(), "Riverside Depot".into()));
        assert_eq!(rows.last().unwrap(), &("Client".into(), "Acme Co".into()));
        assert_eq!(rows.len(), 9, "8 standard + 1 extra");
    }

    #[test]
    fn field_lookup_covers_standard_and_extra() {
        let tb = sample();
        assert_eq!(tb.field("Sheet Number"), Some("A-101"));
        assert_eq!(tb.field("Client"), Some("Acme Co"));
        assert_eq!(tb.field("Nonexistent"), None);
    }

    #[test]
    fn place_emits_box_rules_and_two_texts_per_row() {
        let mut doc = Document::new();
        let tb = sample();
        let ids = tb.place(&mut doc, &TitleBlockStyle::default());
        assert!(!ids.is_empty());
        assert_eq!(doc.len(), ids.len());

        let texts = doc
            .entities()
            .into_iter()
            .filter(|e| matches!(e.geometry, Geometry::Text(_)))
            .count();
        // Two texts (label + value) per row.
        assert_eq!(texts, sample().rows().len() * 2);

        // The project value must appear verbatim.
        assert!(doc.entities().into_iter().any(|e| matches!(
            &e.geometry,
            Geometry::Text(t) if t.value == "Riverside Depot"
        )));
    }

    #[test]
    fn place_is_one_undoable_unit() {
        let mut doc = Document::new();
        sample().place(&mut doc, &TitleBlockStyle::default());
        assert_eq!(doc.undo_label(), Some("TITLEBLOCK"));
        doc.undo();
        assert_eq!(doc.len(), 0, "one undo clears the whole block");
    }

    #[test]
    fn placement_is_deterministic() {
        let build = || {
            let mut doc = Document::new();
            sample().place(&mut doc, &TitleBlockStyle::default());
            doc.entities()
                .into_iter()
                .filter_map(|e| match &e.geometry {
                    Geometry::Text(t) => Some((t.value.clone(), t.position.x, t.position.y)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(build(), build());
    }

    #[test]
    fn place_on_sheet_sits_inside_the_border() {
        let mut doc = Document::new();
        let sheet = Sheet::a1_landscape();
        sample().place_on_sheet(&mut doc, &sheet);
        let (min, max) = sheet.inner();
        for e in doc.entities() {
            if let Geometry::Text(t) = &e.geometry {
                assert!(t.position.x >= min.x - 1.0 && t.position.x <= max.x + 1.0);
                assert!(t.position.y >= min.y - 1.0 && t.position.y <= max.y + 1.0);
            }
        }
    }
}
