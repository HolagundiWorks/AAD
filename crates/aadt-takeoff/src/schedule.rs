//! In-drawing schedule tables (spec §5.2).
//!
//! Places a computed takeoff into the drawing itself as ordinary `TEXT` and
//! `LINE` entities, rather than as a special table object. Two consequences
//! that are the point of doing it this way:
//!
//! - Every existing command (`ERASE`, `LIST`, `UNDO`) and every DXF consumer
//!   already understands the result — the schedule exports to DXF with no new
//!   entity type, and opens in any CAD package.
//! - Insertion goes through [`Document::transact`], so dropping a schedule is a
//!   **single undoable unit** exactly like a manual command (spec §4.1). An
//!   AADT-driven insertion is mechanically indistinguishable from a hand edit.
//!
//! The trade-off: the table is *geometry*, not a live object, so it does not
//! update itself when the drawing changes. [`LiveTakeoff`](crate::LiveTakeoff)
//! is the live path; re-inserting is the in-drawing one.

use aadt_geometry::{Point2, Segment};
use aadt_model::{Document, EntityId, Geometry, Text};

use crate::xlsx::Cell;
use crate::TakeoffTable;

/// Approximate glyph advance as a fraction of cap height, used only for
/// auto-sizing columns. AADT models no font metrics, so this is a deliberate
/// approximation — pass explicit [`ScheduleStyle::col_widths`] when a drawing
/// needs exact column widths.
const CHAR_WIDTH_RATIO: f64 = 0.6;

/// Placement and sizing for an inserted schedule.
#[derive(Debug, Clone)]
pub struct ScheduleStyle {
    /// Top-left corner of the table. Rows run downward from here, matching how
    /// a schedule reads on a sheet.
    pub origin: Point2,
    pub row_height: f64,
    pub text_height: f64,
    /// Explicit column widths. When empty, columns are auto-sized from their
    /// longest cell.
    pub col_widths: Vec<f64>,
    /// Layer for the text entities.
    pub text_layer: String,
    /// Layer for the grid lines.
    pub grid_layer: String,
    /// Whether to draw the ruling lines around cells.
    pub draw_grid: bool,
}

impl Default for ScheduleStyle {
    fn default() -> Self {
        Self {
            origin: Point2::new(0.0, 0.0),
            row_height: 0.4,
            text_height: 0.2,
            col_widths: Vec::new(),
            text_layer: "A-ANNO-TABL".to_string(),
            grid_layer: "A-ANNO-TABL".to_string(),
            draw_grid: true,
        }
    }
}

impl TakeoffTable {
    /// Insert this takeoff into `doc` as a schedule table, returning the ids of
    /// every entity created.
    ///
    /// The whole table is one transaction labelled `SCHEDULE`, so a single
    /// `UNDO` removes it completely.
    ///
    /// Entities are emitted in a fixed order — grid lines, then cell text row by
    /// row, left to right — so repeated insertion into equivalent documents
    /// produces identical output (spec §8).
    pub fn insert_schedule(&self, doc: &mut Document, style: &ScheduleStyle) -> Vec<EntityId> {
        let sheet = self.sheet();
        let grid = ScheduleGrid::new(&sheet.rows, style);
        doc.transact("SCHEDULE", |d| grid.emit(d, &sheet.rows, style))
    }
}

/// Resolved geometry of the table: column widths and overall extent.
struct ScheduleGrid {
    col_widths: Vec<f64>,
    total_width: f64,
    total_height: f64,
}

impl ScheduleGrid {
    fn new(rows: &[Vec<Cell>], style: &ScheduleStyle) -> Self {
        let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
        let col_widths = if style.col_widths.is_empty() {
            (0..cols)
                .map(|c| {
                    let longest = rows
                        .iter()
                        .filter_map(|r| r.get(c))
                        .map(|cell| cell_text(cell).chars().count())
                        .max()
                        .unwrap_or(0);
                    // Half a character of padding on each side.
                    (longest as f64 + 1.0) * style.text_height * CHAR_WIDTH_RATIO
                })
                .collect()
        } else {
            // Pad or truncate an explicit list so it always matches the grid.
            let mut w = style.col_widths.clone();
            w.resize(cols, style.text_height * 6.0);
            w
        };
        let total_width = col_widths.iter().sum();
        let total_height = rows.len() as f64 * style.row_height;
        Self {
            col_widths,
            total_width,
            total_height,
        }
    }

    /// Left edge x of column `c`, relative to the table origin.
    fn col_offset(&self, c: usize) -> f64 {
        self.col_widths.iter().take(c).sum()
    }

    fn emit(&self, doc: &mut Document, rows: &[Vec<Cell>], style: &ScheduleStyle) -> Vec<EntityId> {
        let mut ids = Vec::new();
        let o = style.origin;

        if style.draw_grid && !rows.is_empty() {
            // Horizontal rules, top to bottom.
            for r in 0..=rows.len() {
                let y = o.y - r as f64 * style.row_height;
                ids.push(doc.add_geometry(
                    &style.grid_layer,
                    Geometry::Line(Segment::new(
                        Point2::new(o.x, y),
                        Point2::new(o.x + self.total_width, y),
                    )),
                ));
            }
            // Vertical rules, left to right.
            for c in 0..=self.col_widths.len() {
                let x = o.x + self.col_offset(c);
                ids.push(doc.add_geometry(
                    &style.grid_layer,
                    Geometry::Line(Segment::new(
                        Point2::new(x, o.y),
                        Point2::new(x, o.y - self.total_height),
                    )),
                ));
            }
        }

        // Cell text, row by row.
        let inset = (style.row_height - style.text_height) / 2.0;
        for (r, row) in rows.iter().enumerate() {
            // Baseline sits inset up from the bottom of the row.
            let y = o.y - (r as f64 + 1.0) * style.row_height + inset;
            for (c, cell) in row.iter().enumerate() {
                let value = cell_text(cell);
                if value.is_empty() {
                    continue;
                }
                let x = o.x + self.col_offset(c) + inset;
                ids.push(doc.add_geometry(
                    &style.text_layer,
                    Geometry::Text(Text::new(value, Point2::new(x, y), style.text_height, 0.0)),
                ));
            }
        }
        ids
    }
}

fn cell_text(cell: &Cell) -> &str {
    match cell {
        Cell::Empty => "",
        Cell::Text(t) => t.as_str(),
        Cell::Number(n) => n.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_detect::{detect, DetectionConfig};
    use aadt_geometry::{Polyline, Segment as Seg};
    use aadt_model::Geometry as G;

    fn doc_with_a_wall_and_room() -> Document {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            G::Line(Seg::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        doc.add_geometry(
            "A-WALL",
            G::Line(Seg::new(Point2::new(0.0, 0.2), Point2::new(10.0, 0.2))),
        );
        doc.add_geometry(
            "A-ROOM",
            G::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(4.0, 0.0),
                    Point2::new(4.0, 3.0),
                    Point2::new(0.0, 3.0),
                ],
                true,
            )),
        );
        doc
    }

    fn takeoff_of(doc: &Document) -> TakeoffTable {
        TakeoffTable::from_detection(&detect(doc, &DetectionConfig::default()))
    }

    #[test]
    fn schedule_inserts_text_and_grid() {
        let mut doc = doc_with_a_wall_and_room();
        let table = takeoff_of(&doc);
        let before = doc.len();

        let ids = table.insert_schedule(&mut doc, &ScheduleStyle::default());
        assert!(!ids.is_empty());
        assert_eq!(doc.len(), before + ids.len());

        let added: Vec<_> = doc
            .entities()
            .into_iter()
            .filter(|e| ids.contains(&e.id))
            .collect();
        let texts = added
            .iter()
            .filter(|e| matches!(e.geometry, G::Text(_)))
            .count();
        let lines = added
            .iter()
            .filter(|e| matches!(e.geometry, G::Line(_)))
            .count();
        assert!(texts > 0, "schedule must place text");
        assert!(lines > 0, "schedule must rule the grid");

        // Header text must be present verbatim.
        assert!(added.iter().any(|e| matches!(
            &e.geometry,
            G::Text(t) if t.value == "category"
        )));
    }

    #[test]
    fn schedule_is_one_undoable_unit() {
        // The point of routing through transact: one UNDO removes the lot.
        let mut doc = doc_with_a_wall_and_room();
        let table = takeoff_of(&doc);
        let before = doc.len();

        table.insert_schedule(&mut doc, &ScheduleStyle::default());
        assert!(doc.len() > before);
        assert_eq!(doc.undo_label(), Some("SCHEDULE"));

        doc.undo();
        assert_eq!(doc.len(), before, "one undo must remove the whole table");

        doc.redo();
        assert!(doc.len() > before, "redo restores it");
    }

    #[test]
    fn schedule_placement_is_deterministic() {
        let build = || {
            let mut doc = doc_with_a_wall_and_room();
            let table = takeoff_of(&doc);
            table.insert_schedule(&mut doc, &ScheduleStyle::default());
            doc.entities()
                .into_iter()
                .filter_map(|e| match &e.geometry {
                    G::Text(t) => Some((t.value.clone(), t.position.x, t.position.y)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(build(), build());
    }

    #[test]
    fn rows_run_downward_from_the_origin() {
        let mut doc = doc_with_a_wall_and_room();
        let table = takeoff_of(&doc);
        let style = ScheduleStyle {
            origin: Point2::new(100.0, 50.0),
            ..Default::default()
        };
        table.insert_schedule(&mut doc, &style);

        let ys: Vec<f64> = doc
            .entities()
            .into_iter()
            .filter_map(|e| match &e.geometry {
                G::Text(t) => Some(t.position.y),
                _ => None,
            })
            .collect();
        assert!(!ys.is_empty());
        // Everything sits at or below the origin, and to its right.
        assert!(ys.iter().all(|&y| y < 50.0 && y > 50.0 - 20.0));
    }

    #[test]
    fn explicit_column_widths_are_honoured() {
        let mut doc = doc_with_a_wall_and_room();
        let table = takeoff_of(&doc);
        let style = ScheduleStyle {
            origin: Point2::new(0.0, 0.0),
            col_widths: vec![2.0; 10],
            draw_grid: false,
            ..Default::default()
        };
        table.insert_schedule(&mut doc, &style);

        // Second column's text starts one 2.0-wide column in, plus the inset.
        let inset = (style.row_height - style.text_height) / 2.0;
        let xs: Vec<f64> = doc
            .entities()
            .into_iter()
            .filter_map(|e| match &e.geometry {
                G::Text(t) if t.value == "ref" => Some(t.position.x),
                _ => None,
            })
            .collect();
        assert_eq!(xs.len(), 1, "one 'ref' header cell");
        assert!((xs[0] - (2.0 + inset)).abs() < 1e-9, "got {}", xs[0]);
    }

    #[test]
    fn grid_can_be_suppressed() {
        let mut doc = doc_with_a_wall_and_room();
        let table = takeoff_of(&doc);
        let wall_lines = doc.len();
        let style = ScheduleStyle {
            draw_grid: false,
            ..Default::default()
        };
        let ids = table.insert_schedule(&mut doc, &style);
        let lines = doc
            .entities()
            .into_iter()
            .filter(|e| ids.contains(&e.id) && matches!(e.geometry, G::Line(_)))
            .count();
        assert_eq!(lines, 0, "no ruling lines when the grid is off");
        assert!(doc.len() > wall_lines, "text was still placed");
    }
}
